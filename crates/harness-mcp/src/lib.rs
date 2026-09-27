//! MCP client over rmcp. Callers see only this crate's boundary types, never rmcp's.

mod oauth;

pub use oauth::{OAuthContext, OAuthTokenStore};

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result, anyhow, bail};
use rmcp::model::{CallToolRequestParams, CallToolResult, PaginatedRequestParams};
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{RoleClient, ServiceExt};
use serde::Deserialize;
use serde_json::{Map, Value};

/// One `[mcp.<name>]` config entry.
#[derive(Clone, Debug, Deserialize)]
pub struct McpServerConfig {
    #[serde(flatten)]
    pub transport: McpTransport,
    /// Register this server's tools directly instead of behind the deferred meta-tools.
    #[serde(default)]
    pub preload: bool,
    /// Shown in the system prompt for a deferred server, which isn't connected at that point.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransport {
    /// A stdio server: `command[0]` run with the rest as arguments. `env` values may contain
    /// `{env:VAR}`, expanded only when the server is started.
    Local {
        command: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// A streamable-HTTP server. `headers` values may contain `{env:VAR}`, expanded only when
    /// the server is started. When `oauth` is set, headers are sent alongside the OAuth bearer
    /// token rather than instead of it.
    Remote {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        oauth: bool,
    },
}

/// A tool as the server lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// A tool call's result, already rendered to the text the model sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallOutcome {
    pub content: String,
    pub is_error: bool,
}

/// A connected server. Dropping it stops the connection and kills a stdio child.
pub struct McpClient {
    service: RunningService<RoleClient, ()>,
}

impl McpClient {
    /// Connects to `config`'s server (spawning a child for `Local`, or opening a streamable-HTTP
    /// connection for `Remote`) and completes the MCP handshake. `oauth` is required, and used,
    /// only for a `Remote` server with `oauth: true`.
    pub async fn connect(
        config: &McpServerConfig,
        cwd: &Path,
        oauth: Option<OAuthContext<'_>>,
    ) -> Result<Self> {
        let service = match &config.transport {
            McpTransport::Local { command, env } => connect_local(command, env, cwd).await?,
            McpTransport::Remote {
                url,
                headers,
                oauth: needs_oauth,
            } => connect_remote(url, headers, *needs_oauth, oauth).await?,
        };
        Ok(Self { service })
    }

    /// Every tool, following `next_cursor` pages. A cursor seen before ends the listing, so a
    /// misbehaving server can't loop it forever.
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let mut tools = Vec::new();
        let mut seen = HashSet::new();
        let mut cursor = None;
        loop {
            let page = self
                .service
                .list_tools(Some(PaginatedRequestParams::default().with_cursor(cursor)))
                .await
                .context("tools/list failed")?;
            tools.extend(page.tools.into_iter().map(|tool| McpTool {
                name: tool.name.into_owned(),
                description: tool.description.map(|d| d.into_owned()).unwrap_or_default(),
                input_schema: Value::Object((*tool.input_schema).clone()),
            }));
            match page.next_cursor {
                Some(next) if seen.insert(next.clone()) => cursor = Some(next),
                _ => return Ok(tools),
            }
        }
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Map<String, Value>,
    ) -> Result<CallOutcome> {
        let result = self
            .service
            .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(arguments))
            .await
            .with_context(|| format!("tools/call {name} failed"))?;
        Ok(render(&result))
    }
}

async fn connect_local(
    command: &[String],
    env: &BTreeMap<String, String>,
    cwd: &Path,
) -> Result<RunningService<RoleClient, ()>> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("`command` is empty"))?;
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).current_dir(cwd);
    for (key, value) in env {
        cmd.env(key, expand_env(value)?);
    }
    // The server's stderr would scribble over the TUI.
    let (transport, _) = TokioChildProcess::builder(cmd)
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("starting `{program}`"))?;
    ().serve(transport).await.context("MCP handshake failed")
}

async fn connect_remote(
    url: &str,
    headers: &BTreeMap<String, String>,
    needs_oauth: bool,
    oauth: Option<OAuthContext<'_>>,
) -> Result<RunningService<RoleClient, ()>> {
    // Idempotent: harness-llm's ChatClient also installs this, but a remote MCP server can be
    // the first thing to build a reqwest client (e.g. in a test with no LLM client at all).
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut expanded = std::collections::HashMap::with_capacity(headers.len());
    for (name, value) in headers {
        let header_name = http::HeaderName::try_from(name.as_str())
            .with_context(|| format!("`{name}` is not a valid header name"))?;
        let header_value = http::HeaderValue::from_str(&expand_env(value)?)
            .with_context(|| format!("`{name}`'s value is not a valid header value"))?;
        expanded.insert(header_name, header_value);
    }

    if !needs_oauth {
        let config =
            StreamableHttpClientTransportConfig::with_uri(url.to_string()).custom_headers(expanded);
        let transport = StreamableHttpClientTransport::from_config(config);
        return ().serve(transport).await.context("MCP handshake failed");
    }

    let ctx = oauth.ok_or_else(|| anyhow!("server `{url}` needs `oauth` but none was supplied"))?;
    oauth::connect(url, expanded, ctx).await
}

/// The reference implementation's rendering: an error becomes `{"error": text}`, structured
/// content is JSON-encoded, otherwise text parts are joined with `\n` (falling back to the
/// JSON-encoded content when there is no text at all).
fn render(result: &CallToolResult) -> CallOutcome {
    let text = result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|t| t.text.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    let text_or_json = if text.is_empty() {
        serde_json::to_string(&result.content).unwrap_or_default()
    } else {
        text
    };
    let is_error = result.is_error.unwrap_or(false);
    let content = if is_error {
        serde_json::json!({ "error": text_or_json }).to_string()
    } else if let Some(structured) = &result.structured_content {
        structured.to_string()
    } else {
        text_or_json
    };
    CallOutcome { content, is_error }
}

/// Replaces every `{env:VAR}` with that variable's value. An unset variable is an error rather
/// than an empty string, so a missing secret fails loudly at startup instead of as a confusing
/// auth error later.
pub fn expand_env(value: &str) -> Result<String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("{env:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "{env:".len()..];
        let Some(end) = after.find('}') else {
            bail!("unterminated `{{env:` in `{value}`");
        };
        let var = &after[..end];
        let resolved = std::env::var(var)
            .with_context(|| format!("environment variable `{var}` is not set"))?;
        out.push_str(&resolved);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
