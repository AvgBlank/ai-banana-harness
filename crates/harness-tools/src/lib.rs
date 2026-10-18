//! Host tools the agent can call: `read_file`, `write_file`, `edit`, `glob`, `grep` and `bash`,
//! each gated by the [`guardrail`] permission engine. Per the spec, dispatch across all tool
//! sources is by enumeration, not trait objects — [`HostTool`] is that enumeration for this
//! crate; `harness-core` routes MCP tools through the same
//! [`sanitize_name`]/[`strip_schema`]/[`ToolResult`].

mod bash;
mod container;
mod edit;
mod glob_tool;
mod grep_tool;
mod guardrail;
mod large_output;
pub mod pattern;
mod read_file;
mod runner;
mod write_file;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use similar::TextDiff;

pub use container::{
    COMMAND_LABEL, ContainerConfig, ContainerNetwork, ContainerRunner, OWNER_LABEL,
};
pub use guardrail::{Action, AskOverrides, AskRequest, PermissionConfig, Rules};
pub use runner::{CancelToken, CommandOutput, CommandRequest, CommandRunner, Finish, HostRunner};

/// The `[tools]` config section.
#[derive(Clone, Debug, Deserialize)]
pub struct ToolsConfig {
    /// `bash`'s timeout when a call doesn't set one; clamped to 600 either way.
    pub bash_timeout_secs: u64,
    /// `bash`'s output cap; the middle of longer output is dropped.
    pub output_max_bytes: usize,
}

/// What every tool call runs against.
pub struct ToolContext<'a> {
    pub workspace: &'a Path,
    pub tools: &'a ToolsConfig,
    pub permission: &'a PermissionConfig,
    /// Environment variables removed from `bash` subprocesses, e.g. the provider key.
    pub hidden_env: &'a [&'a str],
    /// `ask` rules already resolved to allow, in memory only; see [`AskOverrides`].
    pub ask_overrides: &'a AskOverrides,
    /// How `bash` commands are run.
    pub runner: &'a dyn CommandRunner,
    /// Set when the turn is cancelled; a running command stops.
    pub cancel: &'a CancelToken,
}

impl ToolContext<'_> {
    /// A path argument resolved and permitted for `key`, or the blocked/needs-approval result to
    /// return.
    fn path(&self, key: guardrail::Key, requested: &str) -> Result<PathBuf, ToolResult> {
        map_verdict(self.permission.path(
            key,
            guardrail::InternalPath {
                workspace: self.workspace,
                requested: Path::new(requested),
            },
            self.ask_overrides,
        ))
    }
}

/// Converts a guardrail verdict into the `ToolResult` a host tool returns instead of running.
pub(crate) fn map_verdict<T>(result: Result<T, guardrail::Verdict>) -> Result<T, ToolResult> {
    result.map_err(|verdict| match verdict {
        guardrail::Verdict::Deny(rule) => ToolResult::blocked(&rule),
        guardrail::Verdict::Ask(ask) => ToolResult::ask(ask),
    })
}

pub use large_output::cap_large_outputs;

/// One of the fixed host tools. Dispatch is a `match` in [`HostTool::call`], not a trait object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTool {
    ReadFile,
    WriteFile,
    Edit,
    Glob,
    Grep,
    Bash,
}

impl HostTool {
    pub const ALL: [HostTool; 6] = [
        HostTool::ReadFile,
        HostTool::WriteFile,
        HostTool::Edit,
        HostTool::Glob,
        HostTool::Grep,
        HostTool::Bash,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HostTool::ReadFile => "read_file",
            HostTool::WriteFile => "write_file",
            HostTool::Edit => "edit",
            HostTool::Glob => "glob",
            HostTool::Grep => "grep",
            HostTool::Bash => "bash",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        HostTool::ALL.into_iter().find(|tool| tool.name() == name)
    }

    pub fn description(self) -> &'static str {
        match self {
            HostTool::ReadFile => read_file::DESCRIPTION,
            HostTool::WriteFile => write_file::DESCRIPTION,
            HostTool::Edit => edit::DESCRIPTION,
            HostTool::Glob => glob_tool::DESCRIPTION,
            HostTool::Grep => grep_tool::DESCRIPTION,
            HostTool::Bash => bash::DESCRIPTION,
        }
    }

    /// The full JSON schema for this tool's arguments, before [`strip_schema`] reduces it to
    /// what a provider is sent.
    pub fn parameters(self) -> Value {
        match self {
            HostTool::ReadFile => read_file::parameters(),
            HostTool::WriteFile => write_file::parameters(),
            HostTool::Edit => edit::parameters(),
            HostTool::Glob => glob_tool::parameters(),
            HostTool::Grep => grep_tool::parameters(),
            HostTool::Bash => bash::parameters(),
        }
    }

    pub fn call(self, ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
        match self {
            HostTool::ReadFile => read_file::call(ctx, arguments),
            HostTool::WriteFile => write_file::call(ctx, arguments),
            HostTool::Edit => edit::call(ctx, arguments),
            HostTool::Glob => glob_tool::call(ctx, arguments),
            HostTool::Grep => grep_tool::call(ctx, arguments),
            HostTool::Bash => bash::call(ctx, arguments),
        }
    }
}

/// A tool call's outcome: the text the model sees, whether a guardrail blocked it, and whether
/// the call failed (which [`cap_large_outputs`] truncates instead of spilling).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub content: String,
    pub blocked: bool,
    pub failed: bool,
    /// A successful file change's actual before/after diff, for display only.
    pub diff: Option<String>,
    /// Set instead of running the call: an `ask` rule matched and needs a required-action
    /// round trip before the caller can retry with it resolved. `content` is not meaningful.
    /// Boxed so an ordinary `ToolResult` (the overwhelming majority) stays small; clippy's
    /// `result_large_err` flags every `Result<_, ToolResult>` otherwise.
    pub needs_approval: Option<Box<AskRequest>>,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            blocked: false,
            failed: false,
            diff: None,
            needs_approval: None,
        }
    }

    /// An already-rendered tool failure, such as an MCP error result.
    pub fn failure(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            blocked: false,
            failed: true,
            diff: None,
            needs_approval: None,
        }
    }

    /// A tool-level failure (bad arguments, missing file, ...), rendered like the reference
    /// implementation's tool errors: `{"error": message}`.
    pub fn error(message: impl AsRef<str>) -> Self {
        Self {
            content: serde_json::json!({ "error": message.as_ref() }).to_string(),
            blocked: false,
            failed: true,
            diff: None,
            needs_approval: None,
        }
    }

    /// A guardrail rejection. The exact wording is part of the contract the model is told to
    /// expect.
    pub fn blocked(rule: &str) -> Self {
        Self {
            content: format!("Blocked by guardrail: {rule}"),
            blocked: true,
            failed: true,
            diff: None,
            needs_approval: None,
        }
    }

    /// An `ask` rule matched; the caller (harness-core) raises the required action and retries.
    pub fn ask(ask: AskRequest) -> Self {
        Self {
            content: format!("Approval required: {}", ask.rule),
            blocked: false,
            failed: true,
            diff: None,
            needs_approval: Some(Box::new(ask)),
        }
    }

    /// The user denied an approval-required call.
    pub fn denied(reason: &str) -> Self {
        Self {
            content: format!("User denied tool call: {reason}"),
            blocked: true,
            failed: true,
            diff: None,
            needs_approval: None,
        }
    }
}

pub(crate) struct FileDiffInput<'a> {
    pub before: &'a str,
    pub after: &'a str,
}

/// A display diff made from the contents on disk, with a small amount of unchanged context.
pub(crate) fn file_diff(input: FileDiffInput<'_>) -> Option<String> {
    if input.before == input.after {
        return None;
    }
    Some(
        TextDiff::from_lines(input.before, input.after)
            .unified_diff()
            .header("before", "after")
            .to_string(),
    )
}

/// Looks a tool up by (possibly sanitised) name and runs it, or reports it as unknown. The one
/// entry point a caller needs; it owns the closed-set dispatch so callers never match on
/// [`HostTool`] themselves.
pub struct HostToolRequest<'a> {
    pub name: &'a str,
    pub arguments_json: &'a str,
}

pub fn call_by_name(ctx: &ToolContext<'_>, request: HostToolRequest<'_>) -> ToolResult {
    let HostToolRequest {
        name,
        arguments_json,
    } = request;
    let Some(tool) = HostTool::from_name(name) else {
        return ToolResult::error(format!("unknown tool \"{name}\""));
    };
    let arguments: Value = serde_json::from_str(arguments_json).unwrap_or(Value::Null);
    tool.call(ctx, &arguments)
}

/// A tool definition ready for the wire: a sanitised name and a schema reduced to
/// `properties`/`required` (plus `type: object`, which every provider expects).
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Every host tool's [`ToolSpec`], names de-duplicated within the batch. `harness-core` seeds
/// its own name pool with these before sanitising MCP and meta-tool names.
pub fn host_tool_specs() -> Vec<ToolSpec> {
    let mut used = HashSet::new();
    HostTool::ALL
        .into_iter()
        .map(|tool| ToolSpec {
            name: sanitize_name(tool.name(), &mut used),
            description: tool.description().to_string(),
            parameters: strip_schema(&tool.parameters()),
        })
        .collect()
}

/// Sanitises to `[A-Za-z0-9_-]`, caps at 64 characters, and de-duplicates within `used` with a
/// numeric suffix (1-50), matching the reference implementation's `getUniqueSanitizedToolName`.
pub fn sanitize_name(name: &str, used: &mut HashSet<String>) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if !used.contains(&cleaned) {
        used.insert(cleaned.clone());
        return cleaned;
    }
    let base: String = cleaned.chars().take(62).collect();
    for suffix in 1..=50 {
        let candidate = format!("{base}{suffix}");
        if !used.contains(&candidate) {
            used.insert(candidate.clone());
            return candidate;
        }
    }
    // More than 50 duplicates of one name is unreachable with today's tool sources; fall back to
    // a name that is at least unique, rather than erroring a whole turn over it.
    let fallback = format!("{base}_{}", used.len());
    used.insert(fallback.clone());
    fallback
}

/// Keeps only `properties` and `required` from a tool's JSON schema (defaulting to `{}`/`[]`),
/// dropping `$defs`, `additionalProperties` and everything else a provider doesn't need.
pub fn strip_schema(schema: &Value) -> Value {
    let object = schema.as_object();
    let properties = object
        .and_then(|o| o.get("properties"))
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let required = object
        .and_then(|o| o.get("required"))
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}
