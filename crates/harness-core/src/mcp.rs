//! The configured MCP servers for one harness, connected lazily: a preload server when a turn
//! first builds its tool list, any other server when a deferred call first targets it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use harness_mcp::{
    McpClient, McpServerConfig, McpTool, McpTransport, OAuthContext, OAuthTokenStore,
};
use harness_tools::{PermissionConfig, ToolResult};
use serde_json::{Map, Value};
use tokio::sync::{OnceCell, mpsc};

/// Reports `(server, login_url)` the moment a fresh OAuth browser flow starts, so the caller can
/// stream `mcp.auth_required` while the connection is still in progress.
pub(crate) type McpAuthNotifier = mpsc::UnboundedSender<(String, String)>;

/// A server's connection state, for the TUI's status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerStatus {
    pub name: String,
    pub state: McpServerState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpServerState {
    /// Configured but not started yet.
    Idle,
    Connected {
        tools: usize,
    },
    /// The last connection attempt failed; the next call that needs the server retries.
    Failed(String),
}

pub(crate) struct McpTarget<'a> {
    pub server: &'a str,
    pub tool: &'a str,
}

pub(crate) struct McpRegistry {
    servers: BTreeMap<String, Server>,
    permission: PermissionConfig,
    workspace: PathBuf,
    oauth_tokens: Arc<dyn OAuthTokenStore>,
}

struct Server {
    config: McpServerConfig,
    /// Single-flight: concurrent callers share one connection attempt. A failure isn't cached.
    connection: OnceCell<Arc<Connection>>,
    last_error: Mutex<Option<String>>,
}

struct Connection {
    client: McpClient,
    tools: Vec<McpTool>,
}

impl McpRegistry {
    pub(crate) fn new(
        servers: &BTreeMap<String, McpServerConfig>,
        permission: PermissionConfig,
        workspace: PathBuf,
        oauth_tokens: Arc<dyn OAuthTokenStore>,
    ) -> Self {
        let servers = servers
            .iter()
            .map(|(name, config)| {
                let server = Server {
                    config: config.clone(),
                    connection: OnceCell::new(),
                    last_error: Mutex::new(None),
                };
                (name.clone(), server)
            })
            .collect();
        Self {
            servers,
            permission,
            workspace,
            oauth_tokens,
        }
    }

    /// Servers behind the deferred meta-tools, with their configured descriptions.
    pub(crate) fn deferred_servers(&self) -> Vec<(&str, Option<&str>)> {
        self.servers
            .iter()
            .filter(|(_, server)| !server.config.preload)
            .map(|(name, server)| (name.as_str(), server.config.description.as_deref()))
            .collect()
    }

    pub(crate) fn status(&self) -> Vec<McpServerStatus> {
        self.servers
            .iter()
            .map(|(name, server)| {
                let state = if let Some(connection) = server.connection.get() {
                    McpServerState::Connected {
                        tools: connection.tools.len(),
                    }
                } else if let Some(error) = server.last_error.lock().ok().and_then(|e| e.clone()) {
                    McpServerState::Failed(error)
                } else {
                    McpServerState::Idle
                };
                McpServerStatus {
                    name: name.clone(),
                    state,
                }
            })
            .collect()
    }

    /// Connects every preload server and returns its tools, sorted by server then tool name. A
    /// server that fails to start is skipped with a warning rather than failing the turn.
    pub(crate) async fn preloaded_tools(
        &self,
        notifier: &McpAuthNotifier,
    ) -> Vec<(String, McpTool)> {
        let mut tools = Vec::new();
        for (name, server) in &self.servers {
            if !server.config.preload {
                continue;
            }
            match self.connection(name, notifier).await {
                Ok(connection) => {
                    let mut server_tools = connection.tools.clone();
                    server_tools.sort_by(|a, b| a.name.cmp(&b.name));
                    tools.extend(server_tools.into_iter().map(|tool| (name.clone(), tool)));
                }
                Err(e) => eprintln!("warning: MCP server `{name}`: {e:#}"),
            }
        }
        tools
    }

    /// Tool names only, as `list_tools` returns them.
    pub(crate) async fn list_tools(&self, server: &str, notifier: &McpAuthNotifier) -> ToolResult {
        match self.connection(server, notifier).await {
            Ok(connection) => {
                let names: Vec<_> = connection.tools.iter().map(|t| t.name.as_str()).collect();
                ToolResult::ok(format!("{server}:\n  {}", names.join(", ")))
            }
            Err(e) => ToolResult::error(format!("{e:#}")),
        }
    }

    /// A tool's description and input schema, as `get_tool_info` returns them.
    pub(crate) async fn tool_info(
        &self,
        target: McpTarget<'_>,
        notifier: &McpAuthNotifier,
    ) -> ToolResult {
        let connection = match self.connection(target.server, notifier).await {
            Ok(connection) => connection,
            Err(e) => return ToolResult::error(format!("{e:#}")),
        };
        match connection.tools.iter().find(|t| t.name == target.tool) {
            Some(found) => ToolResult::ok(
                serde_json::json!({
                    "description": found.description,
                    "inputSchema": found.input_schema,
                })
                .to_string(),
            ),
            None => ToolResult::error(format!(
                "Tool '{}' not found on server '{}'",
                target.tool, target.server
            )),
        }
    }

    /// Calls a server's tool after the permission check, which runs first so a denied call
    /// never starts the server.
    pub(crate) async fn call(
        &self,
        target: McpTarget<'_>,
        arguments: Map<String, Value>,
        notifier: &McpAuthNotifier,
    ) -> ToolResult {
        if let Err(rule) = self.permission.check_mcp(target.tool) {
            return ToolResult::blocked(&rule);
        }
        let connection = match self.connection(target.server, notifier).await {
            Ok(connection) => connection,
            Err(e) => return ToolResult::error(format!("{e:#}")),
        };
        match connection.client.call_tool(target.tool, arguments).await {
            Ok(outcome) if outcome.is_error => ToolResult::failure(outcome.content),
            Ok(outcome) => ToolResult::ok(outcome.content),
            Err(e) => ToolResult::error(format!("{e:#}")),
        }
    }

    async fn connection(&self, name: &str, notifier: &McpAuthNotifier) -> Result<Arc<Connection>> {
        let server = self
            .servers
            .get(name)
            .ok_or_else(|| anyhow!("MCP server '{name}' not found"))?;
        let needs_oauth = matches!(
            server.config.transport,
            McpTransport::Remote { oauth: true, .. }
        );
        let oauth = needs_oauth.then(|| {
            let notifier = notifier.clone();
            let server_name = name.to_string();
            OAuthContext {
                server_name: name,
                token_store: Arc::clone(&self.oauth_tokens),
                on_auth_url: Box::new(move |url: &str| {
                    let _ = notifier.send((server_name.clone(), url.to_string()));
                }),
            }
        });
        let result = server
            .connection
            .get_or_try_init(|| async {
                let client = McpClient::connect(&server.config, &self.workspace, oauth).await?;
                let tools = client.list_tools().await?;
                Ok::<_, anyhow::Error>(Arc::new(Connection { client, tools }))
            })
            .await
            .map(Arc::clone)
            .with_context(|| format!("connecting to MCP server '{name}'"));
        if let Ok(mut last_error) = server.last_error.lock() {
            *last_error = result.as_ref().err().map(|e| format!("{e:#}"));
        }
        result
    }
}
