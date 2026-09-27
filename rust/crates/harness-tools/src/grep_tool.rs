use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use grep_searcher::sinks::UTF8;
use serde_json::Value;

use crate::guardrail::Key;
use crate::{ToolContext, ToolResult};

pub const DESCRIPTION: &str = "Search file contents in the workspace for a regular expression. Optionally scoped to a \
`path` and filtered to files matching a `glob`. Respects .gitignore.";

pub fn parameters() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "pattern": { "type": "string", "description": "Regular expression to search for" },
            "path": { "type": "string", "description": "Directory or file to search, relative to the workspace; defaults to the whole workspace" },
            "glob": { "type": "string", "description": "Only search files whose path matches this glob" },
        },
        "required": ["pattern"],
    })
}

pub fn call(ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
    let Some(pattern) = arguments.get("pattern").and_then(Value::as_str) else {
        return ToolResult::error("grep requires a string \"pattern\" argument");
    };
    if let Err(result) =
        crate::map_verdict(ctx.permission.check(Key::Grep, pattern, ctx.ask_overrides))
    {
        return result;
    }
    let workspace = ctx.workspace;
    let matcher = match RegexMatcher::new(pattern) {
        Ok(m) => m,
        Err(e) => return ToolResult::error(format!("invalid pattern: {e}")),
    };
    let base = match arguments.get("path").and_then(Value::as_str) {
        // Searching a path reads it, so the `read` rules (and confinement) decide.
        Some(path) => match ctx.path(Key::Read, path) {
            Ok(resolved) => resolved,
            Err(blocked) => return blocked,
        },
        None => workspace.to_path_buf(),
    };
    let file_glob = match arguments.get("glob").and_then(Value::as_str) {
        Some(pattern) => match globset::Glob::new(pattern) {
            Ok(glob) => Some(glob.compile_matcher()),
            Err(e) => return ToolResult::error(format!("invalid glob pattern: {e}")),
        },
        None => None,
    };

    let mut hits: Vec<(String, u64, String)> = Vec::new();
    for entry in ignore::WalkBuilder::new(&base).build() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let relative = entry.path().strip_prefix(workspace).unwrap_or(entry.path());
        if let Some(glob) = &file_glob
            && !glob.is_match(relative)
        {
            continue;
        }
        let display = relative.display().to_string();
        // Files the `read` rules deny (e.g. `.env`) are skipped rather than leaked through grep;
        // an `ask` rule is skipped too rather than raising an approval per file.
        if ctx
            .permission
            .check(Key::Read, &display, ctx.ask_overrides)
            .is_err()
        {
            continue;
        }
        // A binary or unreadable file just yields no hits; grep keeps going over the rest.
        let _ = Searcher::new().search_path(
            &matcher,
            entry.path(),
            UTF8(|line_number, line| {
                hits.push((
                    display.clone(),
                    line_number,
                    line.trim_end_matches('\n').to_string(),
                ));
                Ok(true)
            }),
        );
    }
    hits.sort();
    if hits.is_empty() {
        return ToolResult::ok("no matches");
    }
    let listing = hits
        .iter()
        .map(|(file, line, text)| format!("{file}:{line}:{text}"))
        .collect::<Vec<_>>()
        .join("\n");
    ToolResult::ok(listing)
}
