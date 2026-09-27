use serde_json::Value;

use crate::guardrail::Key;
use crate::{ToolContext, ToolResult};

pub const DESCRIPTION: &str = "Read a file from the workspace. Returns its lines, numbered from 1, starting at `offset` \
(default 0) and up to `limit` lines (default 2000).";

pub fn parameters() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Path relative to the workspace" },
            "offset": { "type": "integer", "description": "0-based line to start from" },
            "limit": { "type": "integer", "description": "Maximum number of lines to return" },
        },
        "required": ["path"],
    })
}

pub fn call(ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
    let Some(path) = arguments.get("path").and_then(Value::as_str) else {
        return ToolResult::error("read_file requires a string \"path\" argument");
    };
    let resolved = match ctx.path(Key::Read, path) {
        Ok(resolved) => resolved,
        Err(blocked) => return blocked,
    };
    let content = match std::fs::read_to_string(&resolved) {
        Ok(content) => content,
        Err(e) => return ToolResult::error(format!("reading {}: {e}", resolved.display())),
    };
    let offset = arguments.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(2000) as usize;
    let numbered: Vec<String> = content
        .lines()
        .enumerate()
        .skip(offset)
        .take(limit)
        .map(|(i, line)| format!("{:>6}\t{line}", i + 1))
        .collect();
    if numbered.is_empty() {
        return ToolResult::ok("(no lines in range)");
    }
    ToolResult::ok(numbered.join("\n"))
}
