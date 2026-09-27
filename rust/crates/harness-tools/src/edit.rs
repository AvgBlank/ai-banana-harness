use serde_json::Value;

use crate::guardrail::Key;
use crate::{ToolContext, ToolResult};

pub const DESCRIPTION: &str = "Replace `old_string` with `new_string` in a workspace file. `old_string` must match \
the file exactly (whitespace included) and exactly once, unless `replace_all` is true; add surrounding lines \
to make it unique.";

pub fn parameters() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Path relative to the workspace" },
            "old_string": { "type": "string", "description": "The exact text to replace" },
            "new_string": { "type": "string", "description": "The replacement text" },
            "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of requiring exactly one" },
        },
        "required": ["path", "old_string", "new_string"],
    })
}

pub fn call(ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
    let text = |key| arguments.get(key).and_then(Value::as_str);
    let (Some(path), Some(old), Some(new)) = (text("path"), text("old_string"), text("new_string"))
    else {
        return ToolResult::error(
            "edit requires string \"path\", \"old_string\" and \"new_string\" arguments",
        );
    };
    let replace_all = arguments
        .get("replace_all")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if old.is_empty() {
        return ToolResult::error("old_string must not be empty; use write_file to create a file");
    }
    if old == new {
        return ToolResult::error("old_string and new_string are identical; nothing to change");
    }
    let resolved = match ctx.path(Key::Edit, path) {
        Ok(resolved) => resolved,
        Err(blocked) => return blocked,
    };
    let content = match std::fs::read_to_string(&resolved) {
        Ok(content) => content,
        Err(e) => return ToolResult::error(format!("reading {}: {e}", resolved.display())),
    };
    let count = content.matches(old).count();
    if count == 0 {
        return ToolResult::error(format!("old_string was not found in {path}"));
    }
    if count > 1 && !replace_all {
        return ToolResult::error(format!(
            "old_string matches {count} times in {path}; include more surrounding context to make \
             it unique, or set replace_all to replace every occurrence"
        ));
    }
    let updated = content.replace(old, new);
    match std::fs::write(&resolved, updated) {
        Ok(()) => ToolResult::ok(format!("Edited {path}: replaced {count} occurrence(s)")),
        Err(e) => ToolResult::error(format!("writing {}: {e}", resolved.display())),
    }
}
