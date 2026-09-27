use serde_json::Value;

use crate::guardrail::Key;
use crate::{ToolContext, ToolResult};

pub const DESCRIPTION: &str = "Find files in the workspace whose path matches a glob pattern (e.g. `**/*.rs`). \
Respects .gitignore.";

pub fn parameters() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "pattern": { "type": "string", "description": "Glob pattern, matched against paths relative to the workspace" },
        },
        "required": ["pattern"],
    })
}

pub fn call(ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
    let Some(pattern) = arguments.get("pattern").and_then(Value::as_str) else {
        return ToolResult::error("glob requires a string \"pattern\" argument");
    };
    if let Err(rule) = ctx.permission.check(Key::Glob, pattern) {
        return ToolResult::blocked(&rule);
    }
    let workspace = ctx.workspace;
    let matcher = match globset::Glob::new(pattern) {
        Ok(glob) => glob.compile_matcher(),
        Err(e) => return ToolResult::error(format!("invalid glob pattern: {e}")),
    };

    let mut matches = Vec::new();
    for entry in ignore::WalkBuilder::new(workspace).build() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let relative = entry.path().strip_prefix(workspace).unwrap_or(entry.path());
        if matcher.is_match(relative) {
            matches.push(relative.to_path_buf());
        }
    }
    matches.sort();
    if matches.is_empty() {
        return ToolResult::ok("no matches");
    }
    let listing = matches
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    ToolResult::ok(listing)
}
