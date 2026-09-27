use serde_json::Value;

use crate::guardrail::Key;
use crate::{ToolContext, ToolResult};

pub const DESCRIPTION: &str = "Write `content` to a file in the workspace, replacing it if it exists. Parent \
directories are created as needed. Prefer `edit` for changing part of an existing file.";

pub fn parameters() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Path relative to the workspace" },
            "content": { "type": "string", "description": "The complete new file content" },
        },
        "required": ["path", "content"],
    })
}

pub fn call(ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
    let (Some(path), Some(content)) = (
        arguments.get("path").and_then(Value::as_str),
        arguments.get("content").and_then(Value::as_str),
    ) else {
        return ToolResult::error("write_file requires string \"path\" and \"content\" arguments");
    };
    let resolved = match ctx.path(Key::Edit, path) {
        Ok(resolved) => resolved,
        Err(blocked) => return blocked,
    };
    if let Some(parent) = resolved.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ToolResult::error(format!("creating {}: {e}", parent.display()));
    }
    let verb = if resolved.exists() {
        "Overwrote"
    } else {
        "Created"
    };
    match std::fs::write(&resolved, content) {
        Ok(()) => ToolResult::ok(format!("{verb} {path} ({} bytes)", content.len())),
        Err(e) => ToolResult::error(format!("writing {}: {e}", resolved.display())),
    }
}
