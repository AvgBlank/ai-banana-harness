//! A workspace's changes as files with real patches: against `HEAD` for the workspace, and
//! against a turn's checkpoint for that turn. Counts come from git's own `--numstat`, never from
//! scanning a patch: a hunk line like `--- x` (a removed line that starts with `--`) cannot be
//! told from a file header by its prefix.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

use crate::checkpoint::{git, run, run_limited};

/// Patches over this size are cut and flagged `truncated`; the counts stay exact.
// ponytail: a patch is read up to this size only; raise it, or stream a file's patch on demand,
// if real diffs hit the cap.
const MAX_PATCH_BYTES: usize = 256 * 1024;

/// What git is asked for on every diff: no external diff or text conversion (a repository's own
/// config can name commands for both), no color, and fixed prefixes whatever the user's config.
const DIFF_FLAGS: [&str; 6] = [
    "--no-ext-diff",
    "--no-textconv",
    "--no-color",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "-M",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// One changed file. A renamed file is listed under its new path; its old path is in the patch's
/// `rename from` header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileChange {
    /// Relative to the workspace.
    pub path: String,
    pub status: ChangeStatus,
    pub added: u64,
    pub removed: u64,
    /// The file's unified diff exactly as git prints it, headers included. `None` for a binary
    /// file or when git printed nothing.
    pub patch: Option<String>,
    /// The patch was over the size limit and is cut at a line boundary.
    pub truncated: bool,
}

/// The workspace's uncommitted changes against `HEAD`, plus untracked files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorkspaceChanges {
    /// `false` when the workspace is not in a git repository (or git is missing): there is
    /// nothing to compare against, which is different from no changes.
    pub git: bool,
    pub files: Vec<FileChange>,
}

/// A turn's changes against its checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TurnChanges {
    /// `false` when the turn has no checkpoint (it never changed the workspace, or the
    /// workspace is not a git repository).
    pub checkpoint: bool,
    pub files: Vec<FileChange>,
}

pub(crate) fn workspace_changes(workspace: &Path) -> Result<WorkspaceChanges> {
    if git(workspace, &["rev-parse", "--is-inside-work-tree"]).as_deref() != Some("true") {
        return Ok(WorkspaceChanges {
            git: false,
            files: Vec::new(),
        });
    }
    let baseline = head_or_empty_tree(workspace)?;
    let files = changes_since(workspace, &baseline, &HashSet::new())?;
    Ok(WorkspaceChanges { git: true, files })
}

/// What changed since `checkpoint`: tracked files against the snapshot's tree (so edits made
/// before the turn are not counted, and `bash` edits are), plus files created since.
// ponytail: a file that was already untracked when the turn started is not in the snapshot, so
// edits to it cannot be compared and it is left out (undo cannot restore it either).
pub(crate) fn changes_since_checkpoint(
    workspace: &Path,
    checkpoint: &harness_store::Checkpoint,
) -> Result<Vec<FileChange>> {
    let baseline = checkpoint
        .stash_ref
        .as_deref()
        .unwrap_or(&checkpoint.head_sha);
    let untracked_before: HashSet<String> = serde_json::from_str(&checkpoint.untracked_json)
        .with_context(|| {
            format!(
                "parsing checkpoint {}'s untracked-files list",
                checkpoint.turn_id
            )
        })?;
    changes_since(workspace, baseline, &untracked_before)
}

/// `HEAD`, or the empty tree in a repository with no commits yet, so its files count as added.
fn head_or_empty_tree(workspace: &Path) -> Result<String> {
    if git(workspace, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_some() {
        return Ok("HEAD".to_string());
    }
    git(workspace, &["hash-object", "-t", "tree", "/dev/null"])
        .ok_or_else(|| anyhow!("git could not name the empty tree"))
}

// ponytail: one git process per changed or untracked file (about 10 ms each); batch them into a
// single `git diff` if large working trees make this lag.
fn changes_since(
    workspace: &Path,
    baseline: &str,
    untracked_before: &HashSet<String>,
) -> Result<Vec<FileChange>> {
    let entries = name_status(workspace, baseline)?;
    let counts = numstat(
        workspace,
        &[
            &["diff", "--numstat", "-z", "--relative"],
            &DIFF_FLAGS[..],
            &[baseline],
        ]
        .concat(),
    )?;

    let mut files = Vec::new();
    for entry in entries {
        let counted = counts.get(&entry.path).copied().flatten();
        let binary = counts.get(&entry.path).is_some_and(Option::is_none);
        let (patch, truncated) = if binary {
            (None, false)
        } else {
            let mut args = vec!["diff", "--relative"];
            args.extend(DIFF_FLAGS);
            args.extend([baseline, "--"]);
            args.extend(entry.old.as_deref());
            args.push(&entry.path);
            patch_of(workspace, &args)?
        };
        let (added, removed) = counted.unwrap_or_default();
        files.push(FileChange {
            path: entry.path,
            status: entry.status,
            added,
            removed,
            patch,
            truncated,
        });
    }

    let others = run(
        workspace,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?;
    for path in String::from_utf8_lossy(&others.stdout).split_terminator('\0') {
        if !untracked_before.contains(path)
            && let Some(file) = untracked_file(workspace, path)?
        {
            files.push(file);
        }
    }
    Ok(files)
}

struct Entry {
    status: ChangeStatus,
    /// The path before a rename.
    old: Option<String>,
    path: String,
}

/// The changed tracked files against `baseline`, in git's order. `-z` keeps unusual file names
/// intact; a rename lists its old path, then its new one.
fn name_status(workspace: &Path, baseline: &str) -> Result<Vec<Entry>> {
    let args = [
        &["diff", "--name-status", "-z", "--relative"][..],
        &DIFF_FLAGS[..],
        &[baseline],
    ]
    .concat();
    let output = run(workspace, &args)?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.split_terminator('\0');
    let mut entries = Vec::new();
    while let Some(code) = fields.next() {
        let status = match code.chars().next() {
            Some('A') => ChangeStatus::Added,
            Some('D') => ChangeStatus::Deleted,
            Some('R') => ChangeStatus::Renamed,
            _ => ChangeStatus::Modified,
        };
        let first = fields
            .next()
            .context("git name-status output ended inside a record")?;
        let (old, path) = if status == ChangeStatus::Renamed {
            let new = fields
                .next()
                .context("git name-status output ended inside a rename")?;
            (Some(first.to_string()), new.to_string())
        } else {
            (None, first.to_string())
        };
        entries.push(Entry { status, old, path });
    }
    Ok(entries)
}

/// Added and removed line counts by path; `None` for a binary file, which git reports as `-`.
fn numstat(workspace: &Path, args: &[&str]) -> Result<HashMap<String, Option<(u64, u64)>>> {
    // Small output, and `git diff --no-index` exits 1 when the files differ.
    let (bytes, _) = run_limited(workspace, args, MAX_PATCH_BYTES, &[0, 1])?;
    parse_numstat(&String::from_utf8_lossy(&bytes))
}

/// `-z` numstat: `added<TAB>removed<TAB>path<NUL>`, or for a rename (and for `--no-index`) an
/// empty path followed by `old<NUL>new<NUL>`.
fn parse_numstat(text: &str) -> Result<HashMap<String, Option<(u64, u64)>>> {
    let mut fields = text.split_terminator('\0');
    let mut counts = HashMap::new();
    while let Some(record) = fields.next() {
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            bail!("git numstat printed an unreadable record");
        };
        let path = if path.is_empty() {
            fields
                .next()
                .context("git numstat output ended inside a rename")?;
            fields
                .next()
                .context("git numstat output ended inside a rename")?
        } else {
            path
        };
        let counted = added.parse().ok().zip(removed.parse().ok());
        counts.insert(path.to_string(), counted);
    }
    Ok(counts)
}

/// An untracked file as an added one, diffed against nothing. `None` for a directory (an
/// embedded repository is listed by git as `dir/`) or a file that vanished since the listing.
fn untracked_file(workspace: &Path, path: &str) -> Result<Option<FileChange>> {
    let Ok(metadata) = std::fs::symlink_metadata(workspace.join(path)) else {
        return Ok(None);
    };
    if metadata.is_dir() {
        return Ok(None);
    }
    let against_nothing = |extra: &[&'static str]| {
        let mut args = vec!["diff", "--no-index"];
        args.extend(extra);
        args.extend(DIFF_FLAGS);
        args.extend(["--", "/dev/null", path]);
        args
    };
    let counts = numstat(workspace, &against_nothing(&["--numstat", "-z"]))?;
    let counted = counts.get(path).copied().flatten();
    let binary = counts.get(path).is_some_and(Option::is_none);
    let (patch, truncated) = if binary {
        (None, false)
    } else {
        patch_of(workspace, &against_nothing(&[]))?
    };
    let (added, removed) = counted.unwrap_or_default();
    Ok(Some(FileChange {
        path: path.to_string(),
        status: ChangeStatus::Added,
        added,
        removed,
        patch,
        truncated,
    }))
}

/// A file's patch from `git`, cut at a line boundary when over [`MAX_PATCH_BYTES`].
fn patch_of(workspace: &Path, args: &[&str]) -> Result<(Option<String>, bool)> {
    let (mut bytes, cut) = run_limited(workspace, args, MAX_PATCH_BYTES, &[0, 1])?;
    if cut && let Some(last_newline) = bytes.iter().rposition(|&b| b == b'\n') {
        bytes.truncate(last_newline + 1);
    }
    let patch = String::from_utf8_lossy(&bytes).into_owned();
    Ok(((!patch.is_empty()).then_some(patch), cut))
}
