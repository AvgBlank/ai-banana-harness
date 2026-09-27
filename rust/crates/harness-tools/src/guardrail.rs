//! Confines host-tool paths to the workspace and denies reading secrets. Ticket 04 adds the
//! bash denylist and the full allow/ask/deny permission engine on top of the same
//! [`pattern::glob_match`] matcher; this ticket only needs the two rules below.

use std::path::{Path, PathBuf};

use crate::pattern::glob_match;

/// File-name patterns denied for reading, checked against the resolved path's final component.
const DENIED_READ_PATTERNS: &[&str] = &[".env*"];

/// Resolves `requested` against `workspace` (joining if relative) and confirms the result stays
/// inside the workspace, following symlinks. Returns the guardrail rule name on an escape.
///
/// A path that doesn't exist yet can't be canonicalised; it is returned unresolved; the caller's
/// own filesystem call then reports "not found" rather than a guardrail block.
pub fn confine(workspace: &Path, requested: &Path) -> Result<PathBuf, String> {
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        workspace.join(requested)
    };
    let Ok(canonical) = candidate.canonicalize() else {
        return Ok(candidate);
    };
    let Ok(workspace_canonical) = workspace.canonicalize() else {
        return Err("workspace confinement".to_string());
    };
    if canonical.starts_with(&workspace_canonical) {
        Ok(canonical)
    } else {
        Err("workspace confinement".to_string())
    }
}

/// `Some(rule)` when `path`'s file name is denied for reading (currently just `.env*`).
pub fn denied_read_rule(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?;
    DENIED_READ_PATTERNS
        .iter()
        .any(|pattern| glob_match(pattern, name))
        .then_some("reading .env files")
}
