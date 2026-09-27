//! Host tools the agent can call: `read_file`, `glob` and `grep`, each confined to the
//! workspace by the [`guardrail`] module. Ticket 04 adds `write_file`, `edit` and `bash`
//! alongside these. Per the spec, dispatch across all tool sources is by enumeration, not trait
//! objects — [`HostTool`] is that enumeration for this crate; `harness-core` routes MCP tools
//! through the same [`sanitize_name`]/[`strip_schema`]/[`ToolResult`].

mod glob_tool;
mod grep_tool;
mod guardrail;
pub mod pattern;
mod read_file;

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

/// One of the fixed host tools. Dispatch is a `match` in [`HostTool::call`], not a trait object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTool {
    ReadFile,
    Glob,
    Grep,
}

impl HostTool {
    pub const ALL: [HostTool; 3] = [HostTool::ReadFile, HostTool::Glob, HostTool::Grep];

    pub fn name(self) -> &'static str {
        match self {
            HostTool::ReadFile => "read_file",
            HostTool::Glob => "glob",
            HostTool::Grep => "grep",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        HostTool::ALL.into_iter().find(|tool| tool.name() == name)
    }

    pub fn description(self) -> &'static str {
        match self {
            HostTool::ReadFile => read_file::DESCRIPTION,
            HostTool::Glob => glob_tool::DESCRIPTION,
            HostTool::Grep => grep_tool::DESCRIPTION,
        }
    }

    /// The full JSON schema for this tool's arguments, before [`strip_schema`] reduces it to
    /// what a provider is sent.
    pub fn parameters(self) -> Value {
        match self {
            HostTool::ReadFile => read_file::parameters(),
            HostTool::Glob => glob_tool::parameters(),
            HostTool::Grep => grep_tool::parameters(),
        }
    }

    pub fn call(self, workspace: &Path, arguments: &Value) -> ToolResult {
        match self {
            HostTool::ReadFile => read_file::call(workspace, arguments),
            HostTool::Glob => glob_tool::call(workspace, arguments),
            HostTool::Grep => grep_tool::call(workspace, arguments),
        }
    }
}

/// A tool call's outcome: the text the model sees, and whether a guardrail blocked it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub content: String,
    pub blocked: bool,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            blocked: false,
        }
    }

    /// A tool-level failure (bad arguments, missing file, ...), rendered like the reference
    /// implementation's tool errors: `{"error": message}`.
    pub fn error(message: impl AsRef<str>) -> Self {
        Self {
            content: serde_json::json!({ "error": message.as_ref() }).to_string(),
            blocked: false,
        }
    }

    /// A guardrail rejection. The exact wording is part of the contract the model is told to
    /// expect.
    pub fn blocked(rule: &str) -> Self {
        Self {
            content: format!("Blocked by guardrail: {rule}"),
            blocked: true,
        }
    }
}

/// Looks a tool up by (possibly sanitised) name and runs it, or reports it as unknown. The one
/// entry point a caller needs; it owns the closed-set dispatch so callers never match on
/// [`HostTool`] themselves.
pub fn call_by_name(workspace: &Path, name: &str, arguments_json: &str) -> ToolResult {
    let Some(tool) = HostTool::from_name(name) else {
        return ToolResult::error(format!("unknown tool \"{name}\""));
    };
    let arguments: Value = serde_json::from_str(arguments_json).unwrap_or(Value::Null);
    tool.call(workspace, &arguments)
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
