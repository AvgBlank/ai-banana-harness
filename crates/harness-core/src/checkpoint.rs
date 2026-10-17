//! A git snapshot of the workspace, taken before a turn's first mutating tool call, so `/undo`
//! can restore it. Shells out to `git`; this module owns that plumbing, and `changes` shares it
//! to diff the workspace and to diff a turn against its snapshot.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};

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
pub(crate) fn git(workspace: &Path, args: &[&str]) -> Option<String> {
    let output = run(workspace, args).ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn run(workspace: &Path, args: &[&str]) -> Result<std::process::Output> {
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

/// Like [`run`], but reads at most `limit` bytes of stdout, for output that can be huge (a diff
/// of a generated file). Past the limit the process is killed instead of read to the end, so the
/// cost is `limit` bytes of memory, not the output's size; the flag says whether it was cut. An
/// exit code outside `ok_codes` is an error unless the output was cut (`git diff --no-index`
/// exits 1 when the files differ). Stderr is discarded, so an error names only the exit code.
pub(crate) fn run_limited(
    workspace: &Path,
    args: &[&str],
    limit: usize,
    ok_codes: &[i32],
) -> Result<(Vec<u8>, bool)> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("running git {args:?}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("git {args:?} has no stdout"))?;
    let mut out = Vec::new();
    stdout
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .with_context(|| format!("reading git {args:?}"))?;
    let cut = out.len() > limit;
    if cut {
        out.truncate(limit);
        let _ = child.kill();
    }
    let status = child
        .wait()
        .with_context(|| format!("waiting for git {args:?}"))?;
    if !cut && !status.code().is_some_and(|code| ok_codes.contains(&code)) {
        bail!("git {args:?} failed: {status}");
    }
    Ok((out, cut))
}
