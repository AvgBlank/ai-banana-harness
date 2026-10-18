//! One turn's tool list and the routing from a (sanitised) tool name to its source.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use harness_llm::ToolDefinition;
use harness_tools::{
    AskOverrides, CancelToken, HostRunner, HostToolRequest, PermissionConfig, ToolContext,
    ToolResult, ToolsConfig, sanitize_name, strip_schema,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::mcp::{McpAuthNotifier, McpRegistry, McpTarget};

pub(crate) struct ToolInvocation<'a> {
    pub name: &'a str,
    pub arguments_json: &'a str,
    /// Where a remote MCP server's OAuth login URL is reported the moment it starts. Owned
    /// (not borrowed) so a call can move into its own `tokio::spawn`ed task.
    pub mcp_auth: McpAuthNotifier,
    /// The turn's cancellation: a running host command stops when it fires. Owned, like
    /// `mcp_auth`, so a call can move into its own task.
    pub cancel: CancellationToken,
}

/// Where a tool call goes. Closed set, dispatched by `match`.
#[derive(Clone, Debug)]
enum ToolSource {
    Host,
    Mcp { server: String, tool: String },
    Deferred(MetaTool),
}

#[derive(Clone, Copy, Debug)]
enum MetaTool {
    ListTools,
    GetToolInfo,
    CallTool,
}

impl MetaTool {
    const ALL: [MetaTool; 3] = [
        MetaTool::ListTools,
        MetaTool::GetToolInfo,
        MetaTool::CallTool,
    ];

    fn name(self) -> &'static str {
        match self {
            MetaTool::ListTools => "list_tools",
            MetaTool::GetToolInfo => "get_tool_info",
            MetaTool::CallTool => "call_tool",
        }
    }

    fn description(self) -> &'static str {
        match self {
            MetaTool::ListTools => {
                "List tool names available on an MCP server.\nReturns tool names only; use \
                 get_tool_info to get full details before calling a tool."
            }
            MetaTool::GetToolInfo => {
                "Get the description and inputSchema of a specific tool on an MCP server.\nThe \
                 Agent MUST call list_tools first to know which tools are available."
            }
            MetaTool::CallTool => {
                "Call a tool on an MCP server.\nThe Agent MUST call list_tools and get_tool_info \
                 before calling a tool."
            }
        }
    }

    fn parameters(self) -> Value {
        let server = json!({ "type": "string", "description": "Name of the MCP server." });
        let tool = json!({ "type": "string", "description": "Name of the tool." });
        match self {
            MetaTool::ListTools => json!({
                "properties": { "mcp_server": server },
                "required": ["mcp_server"],
            }),
            MetaTool::GetToolInfo => json!({
                "properties": { "mcp_server": server, "tool_name": tool },
                "required": ["mcp_server", "tool_name"],
            }),
            MetaTool::CallTool => json!({
                "properties": {
                    "mcp_server": server,
                    "tool_name": tool,
                    "input": { "type": "object", "description": "Arguments to pass to the tool." },
                },
                "required": ["mcp_server", "tool_name"],
            }),
        }
    }
}

#[derive(Deserialize)]
struct MetaArgs {
    mcp_server: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    input: Map<String, Value>,
}

pub(crate) struct TurnTools {
    pub(crate) definitions: Vec<ToolDefinition>,
    routes: HashMap<String, ToolSource>,
    mcp: Arc<McpRegistry>,
    workspace: PathBuf,
    tools_config: ToolsConfig,
    permission: PermissionConfig,
    /// `ask` rules allowed "for session"; lives as long as the harness (shared across turns),
    /// not just this one.
    ask_overrides: Arc<AskOverrides>,
}

impl TurnTools {
    /// Host tools, then the deferred meta-tools (when any server is deferred), then preloaded
    /// MCP tools, all sharing one name pool so a later source is suffixed on a collision.
    pub(crate) async fn build(
        mcp: Arc<McpRegistry>,
        workspace: PathBuf,
        tools_config: ToolsConfig,
        permission: PermissionConfig,
        ask_overrides: Arc<AskOverrides>,
        builtin_definitions: &[ToolDefinition],
        mcp_auth: McpAuthNotifier,
    ) -> Self {
        let mut definitions = Vec::new();
        let mut routes = HashMap::new();
        let mut used = HashSet::new();
        for spec in harness_tools::host_tool_specs() {
            used.insert(spec.name.clone());
            routes.insert(spec.name.clone(), ToolSource::Host);
            definitions.push(ToolDefinition {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            });
        }
        if !mcp.deferred_servers().is_empty() {
            for meta in MetaTool::ALL {
                let name = sanitize_name(meta.name(), &mut used);
                routes.insert(name.clone(), ToolSource::Deferred(meta));
                definitions.push(ToolDefinition {
                    name,
                    description: meta.description().to_string(),
                    parameters: strip_schema(&meta.parameters()),
                });
            }
        }
        for builtin in builtin_definitions {
            used.insert(builtin.name.clone());
        }
        for (server, tool) in mcp.preloaded_tools(&mcp_auth).await {
            let name = sanitize_name(&tool.name, &mut used);
            definitions.push(ToolDefinition {
                name: name.clone(),
                description: format!("mcp server: {server}\n{}", tool.description),
                parameters: strip_schema(&tool.input_schema),
            });
            routes.insert(
                name,
                ToolSource::Mcp {
                    server,
                    tool: tool.name,
                },
            );
        }
        Self {
            definitions,
            routes,
            mcp,
            workspace,
            tools_config,
            permission,
            ask_overrides,
        }
    }

    /// Allows an `ask` rule for the rest of the harness's lifetime (not just this turn).
    pub(crate) fn allow_for_session(&self, key: &str, pattern: &str) {
        self.ask_overrides.allow(key, pattern);
    }

    /// `once_override` bypasses one specific `ask` rule for this single call, without
    /// remembering it (used for "allow once"); `None` checks normally, which also covers "allow
    /// for session" once [`Self::allow_for_session`] has recorded it.
    pub(crate) async fn call(
        &self,
        invocation: ToolInvocation<'_>,
        once_override: Option<(String, String)>,
    ) -> ToolResult {
        let ToolInvocation {
            name,
            arguments_json,
            mcp_auth,
            cancel,
        } = invocation;
        match self.routes.get(name) {
            // An unknown name goes to the host tools, which report it as unknown.
            None | Some(ToolSource::Host) => {
                let (workspace, name, arguments) = (
                    self.workspace.clone(),
                    name.to_string(),
                    arguments_json.to_string(),
                );
                let tools_config = self.tools_config.clone();
                let permission = self.permission.clone();
                let ask_overrides = Arc::clone(&self.ask_overrides);
                // The blocking call polls this flag; the turn's token is async, so a watcher sets
                // it. A turn cancelled before the call starts sets it up front, and the watcher
                // is dropped as soon as the call is over.
                let stop = CancelToken::new();
                if cancel.is_cancelled() {
                    stop.cancel();
                }
                let watcher = tokio::spawn({
                    let (stop, cancel) = (stop.clone(), cancel.clone());
                    async move {
                        cancel.cancelled().await;
                        stop.cancel();
                    }
                });
                // Blocking filesystem/process work, on its own OS thread so calls run in parallel.
                let result = tokio::task::spawn_blocking(move || {
                    let once;
                    let ask_overrides: &AskOverrides = match &once_override {
                        Some((key, pattern)) => {
                            once = AskOverrides::new();
                            once.allow(key, pattern);
                            &once
                        }
                        None => &ask_overrides,
                    };
                    let ctx = ToolContext {
                        workspace: &workspace,
                        tools: &tools_config,
                        permission: &permission,
                        hidden_env: &[crate::API_KEY_ENV],
                        ask_overrides,
                        runner: &HostRunner,
                        cancel: &stop,
                    };
                    harness_tools::call_by_name(
                        &ctx,
                        HostToolRequest {
                            name: &name,
                            arguments_json: &arguments,
                        },
                    )
                })
                .await
                .unwrap_or_else(|e| ToolResult::error(format!("tool task failed: {e}")));
                watcher.abort();
                result
            }
            Some(ToolSource::Mcp { server, tool }) => {
                // Bad or empty JSON becomes `{}`, and the server validates what it gets.
                let arguments = serde_json::from_str(arguments_json).unwrap_or_default();
                self.mcp
                    .call(McpTarget { server, tool }, arguments, &mcp_auth)
                    .await
            }
            Some(ToolSource::Deferred(meta)) => {
                let args: MetaArgs = match serde_json::from_str(arguments_json) {
                    Ok(args) => args,
                    Err(e) => return ToolResult::error(format!("invalid arguments: {e}")),
                };
                match meta {
                    MetaTool::ListTools => self.mcp.list_tools(&args.mcp_server, &mcp_auth).await,
                    MetaTool::GetToolInfo => {
                        self.mcp
                            .tool_info(
                                McpTarget {
                                    server: &args.mcp_server,
                                    tool: &args.tool_name,
                                },
                                &mcp_auth,
                            )
                            .await
                    }
                    MetaTool::CallTool => {
                        self.mcp
                            .call(
                                McpTarget {
                                    server: &args.mcp_server,
                                    tool: &args.tool_name,
                                },
                                args.input,
                                &mcp_auth,
                            )
                            .await
                    }
                }
            }
        }
    }
}
