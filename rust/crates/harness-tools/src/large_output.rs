//! Keeps one tool step's results from flooding the context, ported from TrueForge's
//! `LargeToolResponse`. Oversized results are spilled to a self-git-ignoring folder in the
//! workspace and replaced with a pointer the model can follow with `read_file`/`grep`.

use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{ToolResult, guardrail, sanitize_name};

/// A batch under this many tokens passes untouched; any single result this big is blocked.
const INDIVIDUAL_TOKEN_THRESHOLD: f64 = 6_000.0;
/// After that, the largest remaining results are blocked until the batch is under this.
const TOTAL_TOKEN_THRESHOLD: f64 = 10_000.0;
const PREVIEW_CHARS: usize = 100;
const FAILURE_TRUNCATION_CHARS: usize = 500;
/// Workspace-relative; the folder carries its own `.gitignore` of `*`.
const SPILL_DIR: &str = ".banana-harness/tool-output";

/// Rewrites oversized results in `batch` (tool name, result) in place, in the batch's order.
pub fn cap_large_outputs(workspace: &Path, batch: &mut [(String, ToolResult)]) {
    let sizes: Vec<f64> = batch.iter().map(|(_, r)| tokens(&r.content)).collect();
    let mut total: f64 = sizes.iter().sum();
    if total < INDIVIDUAL_TOKEN_THRESHOLD {
        return;
    }
    let (oversized, mut rest): (Vec<usize>, Vec<usize>) =
        (0..batch.len()).partition(|&i| sizes[i] >= INDIVIDUAL_TOKEN_THRESHOLD);
    rest.sort_by(|&a, &b| sizes[b].total_cmp(&sizes[a]));
    for i in oversized {
        total -= block(workspace, i, &mut batch[i]);
    }
    for i in rest {
        if total < TOTAL_TOKEN_THRESHOLD {
            break;
        }
        total -= block(workspace, i, &mut batch[i]);
    }
}

/// Replaces one result's content and returns the tokens that saved.
fn block(workspace: &Path, index: usize, (name, result): &mut (String, ToolResult)) -> f64 {
    let original = std::mem::take(&mut result.content);
    result.content = if result.failed {
        original.chars().take(FAILURE_TRUNCATION_CHARS).collect()
    } else {
        let preview = preview(&original);
        match spill(workspace, name, index, &original) {
            Ok(path) => format!(
                "Content too large. Result saved to: {path}.\n\nThe Agent can do the following to handle it:\n\
                 1. Use read_file with offset and limit to read parts of the file, or grep with this path to extract data.\n\n\
                 Preview (first and last {PREVIEW_CHARS} chars):\n{preview}"
            ),
            // Losing the full content beats failing the step; the preview still survives.
            Err(_) => format!(
                "Content too big. To handle a large tool call the Agent can do the following:\n\
                 1. Check if some parameter can be passed in order to reduce the output size.\n\n\
                 Preview (first and last {PREVIEW_CHARS} chars):\n{preview}"
            ),
        }
    };
    (tokens(&original) - tokens(&result.content)).max(0.0)
}

/// Writes `content` under [`SPILL_DIR`] and returns its workspace-relative path.
fn spill(workspace: &Path, name: &str, index: usize, content: &str) -> std::io::Result<String> {
    let dir = workspace.join(SPILL_DIR);
    guardrail::confine(guardrail::InternalPath {
        workspace,
        requested: &dir,
    })
    .map_err(std::io::Error::other)?;
    std::fs::create_dir_all(&dir)?;
    guardrail::confine(guardrail::InternalPath {
        workspace,
        requested: &dir,
    })
    .map_err(std::io::Error::other)?;
    let gitignore = dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, "*\n")?;
    }
    let tool: String = sanitize_name(name, &mut HashSet::new())
        .chars()
        .take(40)
        .collect();
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let file = format!("{tool}_{index}_{epoch}.txt");
    std::fs::write(dir.join(&file), content)?;
    Ok(format!("{SPILL_DIR}/{file}"))
}

/// Estimated as characters/4, like the reference.
fn tokens(text: &str) -> f64 {
    text.chars().count() as f64 / 4.0
}

fn preview(content: &str) -> String {
    let count = content.chars().count();
    if count <= PREVIEW_CHARS * 2 {
        return content.to_string();
    }
    let head: String = content.chars().take(PREVIEW_CHARS).collect();
    let tail: String = content.chars().skip(count - PREVIEW_CHARS).collect();
    format!("{head}\n...\n{tail}")
}
