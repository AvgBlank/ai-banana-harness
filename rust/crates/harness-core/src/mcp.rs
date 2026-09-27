//! The configured MCP servers for one harness, connected lazily: a preload server when a turn
//! first builds its tool list, any other server when a deferred call first targets it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use harness_mcp::{McpClient, McpServerConfig, McpTool};
use harness_tools::ToolResult;
use serde_json::{Map, Value};
use tokio::sync::OnceCell;

use crate::config::{PatternRules, PermissionAction};

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

pub(crate) struct McpRegistry {
    servers: BTreeMap<String, Server>,
    permission: PatternRules,
    workspace: PathBuf,
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
        permission: PatternRules,
        workspace: PathBuf,
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
    pub(crate) async fn preloaded_tools(&self) -> Vec<(String, McpTool)> {
        let mut tools = Vec::new();
        for (name, server) in &self.servers {
            if !server.config.preload {
                continue;
            }
            match self.connection(name).await {
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
    pub(crate) async fn list_tools(&self, server: &str) -> ToolResult {
        match self.connection(server).await {
            Ok(connection) => {
                let names: Vec<_> = connection.tools.iter().map(|t| t.name.as_str()).collect();
                ToolResult::ok(format!("{server}:\n  {}", names.join(", ")))
            }
            Err(e) => ToolResult::error(format!("{e:#}")),
        }
    }

    /// A tool's description and input schema, as `get_tool_info` returns them.
    pub(crate) async fn tool_info(&self, server: &str, tool: &str) -> ToolResult {
        let connection = match self.connection(server).await {
            Ok(connection) => connection,
            Err(e) => return ToolResult::error(format!("{e:#}")),
        };
        match connection.tools.iter().find(|t| t.name == tool) {
            Some(found) => ToolResult::ok(
                serde_json::json!({
                    "description": found.description,
                    "inputSchema": found.input_schema,
                })
                .to_string(),
            ),
            None => ToolResult::error(format!("Tool '{tool}' not found on server '{server}'")),
        }
    }

    /// Calls a server's tool after the permission check, which runs first so a denied call
    /// never starts the server.
    pub(crate) async fn call(
        &self,
        server: &str,
        tool: &str,
        arguments: Map<String, Value>,
    ) -> ToolResult {
        if let Some((pattern, PermissionAction::Deny | PermissionAction::Ask)) =
            self.permission.resolve(tool)
        {
            // `ask` behaves as deny until approvals exist (ticket 05).
            return ToolResult::blocked(&format!("mcp permission `{pattern}`"));
        }
        let connection = match self.connection(server).await {
            Ok(connection) => connection,
            Err(e) => return ToolResult::error(format!("{e:#}")),
        };
        match connection.client.call_tool(tool, arguments).await {
            Ok(outcome) => ToolResult::ok(outcome.content),
            Err(e) => ToolResult::error(format!("{e:#}")),
        }
    }

    async fn connection(&self, name: &str) -> Result<Arc<Connection>> {
        let server = self
            .servers
            .get(name)
            .ok_or_else(|| anyhow!("MCP server '{name}' not found"))?;
        let result = server
            .connection
            .get_or_try_init(|| async {
                let client = McpClient::connect(&server.config, &self.workspace).await?;
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
