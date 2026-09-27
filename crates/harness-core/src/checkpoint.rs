//! A git snapshot of the workspace, taken before a turn's first mutating tool call, so `/undo`
//! can restore it. Shells out to `git`, the same approach `harness-tui::workspace_changes` uses
//! for its (display-only) diff.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// A turn's pre-mutation snapshot: the tracked-file state (as a stash-style commit, anchored so
/// it survives garbage collection) plus which paths were untracked at the time.
pub(crate) struct Checkpoint {
    pub(crate) head_sha: String,
    /// `None` when the worktree matched `HEAD` exactly, so there was nothing to stash.
    pub(crate) stash_ref: Option<String>,
    untracked: Vec<String>,
}

impl Checkpoint {
    pub(crate) fn untracked_json(&self) -> String {
        serde_json::to_string(&self.untracked).unwrap_or_else(|_| "[]".to_string())
    }
}

/// `None` when `workspace` isn't a git repository (or has no commits yet) — the caller takes no
/// checkpoint and `/undo` later reports nothing available.
pub(crate) fn capture(workspace: &Path, turn_id: &str) -> Option<Checkpoint> {
    let head_sha = git(workspace, &["rev-parse", "HEAD"])?;
    // `stash create` snapshots tracked changes as a commit without touching the index, the
    // worktree or the stash list; an empty worktree-vs-HEAD diff yields an empty string.
    let stash_ref = git(workspace, &["stash", "create"]).filter(|s| !s.is_empty());
    if let Some(stash_ref) = &stash_ref {
        // The commit `stash create` returns is otherwise unreferenced and a `git gc` could
        // collect it before `/undo` runs; anchoring it under our own ref keeps it alive.
        let _ = run(
            workspace,
            &["update-ref", &checkpoint_ref(turn_id), stash_ref],
        );
    }
    let untracked = git(workspace, &["ls-files", "--others", "--exclude-standard"])
        .map(|out| out.lines().map(str::to_string).collect())
        .unwrap_or_default();
    Some(Checkpoint {
        head_sha,
        stash_ref,
        untracked,
    })
}

/// Restores `workspace`'s tracked files to the checkpoint and deletes any file that's still
/// untracked now but wasn't at checkpoint time (i.e. the turn created it).
// ponytail: doesn't handle a tracked file the turn deleted, or one it renamed; the fixture-repo
// acceptance tests (edit + create) don't exercise those, and no ticket asks for them yet.
pub(crate) fn restore(workspace: &Path, row: &harness_store::Checkpoint) -> Result<()> {
    let target = row.stash_ref.as_deref().unwrap_or(&row.head_sha);
    run(workspace, &["checkout", target, "--", "."])
        .with_context(|| format!("git checkout {target} -- . in {}", workspace.display()))?;

    let before: HashSet<String> = serde_json::from_str(&row.untracked_json)
        .with_context(|| format!("parsing checkpoint {}'s untracked-files list", row.turn_id))?;
    let now = git(workspace, &["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();
    for path in now.lines() {
        if !before.contains(path) {
            let _ = std::fs::remove_file(workspace.join(path));
        }
    }
    Ok(())
}

fn checkpoint_ref(turn_id: &str) -> String {
    format!("refs/banana-harness/checkpoint/{turn_id}")
}

/// Runs `git` in `workspace` and returns trimmed stdout, or `None` on a non-zero exit (not a git
/// repo, no commits yet, `git` missing, ...).
fn git(workspace: &Path, args: &[&str]) -> Option<String> {
    let output = run(workspace, args).ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run(workspace: &Path, args: &[&str]) -> Result<std::process::Output> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .with_context(|| format!("running git {args:?}"))?;
    if !output.status.success() {
        bail!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}
