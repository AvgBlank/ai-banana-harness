# 05: Workspace and turn changes with real patches

**Lane / milestone:** R · M1 Core additions   **Size:** M (one agent session)

**What to build:** Move `workspace_changes` out of `harness-tui` into core and give it real patches. Add `Harness::workspace_changes()` and `Harness::turn_changes(session_id)` with the `git` and `checkpoint` flags the UI needs to tell "no changes" from "not a git repository" and "no checkpoint". One owner for the git plumbing.

**Blocked by:** 01

**Status:** done 2026-10-17. All five acceptance criteria verified by tests that were run against real temporary git repositories; `make lint` and `make test` passed locally (169 tests, 0 failed) with the build output in a temporary directory outside the repository. Not run: CI on Linux (macOS only here).

**Files and components likely to change:**
- `crates/harness-core/src/changes.rs` (new) and `checkpoint.rs` (shared git helper; update its doc comment)
- `crates/harness-core/src/{harness.rs,lib.rs}`
- `crates/harness-tui/src/{lib.rs,app.rs}` (use core's `FileChange`; delete the TUI copy, its `git` helper and its `FileChange`)
- `crates/harness-core/tests/{changes.rs (new),checkpoint.rs}`, `crates/harness-tui/tests/polish.rs` (the moved test)

**Acceptance criteria:**
- [x] `FileChange { path, status, added, removed, patch, truncated }` and `ChangeStatus` (added, modified, deleted, renamed) live only in core.
- [x] `WorkspaceChanges { git: bool, files }`: against `HEAD` plus untracked files; modified, added, deleted and binary files give the right status, counts and patch; a non-git workspace gives `git: false` and no files; a patch over 256 KiB is cut with `truncated: true`; binary files have counts 0 and no patch.
- [x] `TurnChanges { checkpoint: bool, files }`: the session's last turn against its checkpoint, including files changed by `bash`, excluding edits made before the turn, files created during the turn listed as added; `checkpoint: false` and no files after a read-only turn.
- [x] The TUI sidebar still shows per-file counts, now from core; `git grep -n "fn workspace_changes" crates/harness-tui` is empty.
- [x] Existing checkpoint and undo tests still pass.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-core --test changes --test checkpoint --locked
cargo test -p harness-tui --locked
make lint
make test
```

**Documentation:** None.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.

**Completion record (2026-10-17):**
- **Changed:** new `crates/harness-core/src/changes.rs`; `checkpoint.rs` (its `git` and `run` helpers are now `pub(crate)`, one size-limited runner `run_limited` was added, and its doc comment no longer points at the TUI); `harness.rs`, `lib.rs`; `crates/harness-tui/src/{lib.rs,app.rs}` (the copy, its `git` helper and its `FileChange` are deleted); tests: new `crates/harness-core/tests/changes.rs` (12 tests), `checkpoint.rs` and `common/mod.rs` (the two git test helpers now live once, in `common`, instead of privately in `checkpoint.rs`), and `crates/harness-tui/tests/polish.rs` (the two `workspace_changes` tests moved to core; the rendering tests build `FileChange` through core's type). No migration, no new dependency.
- **API:** `Harness::workspace_changes() -> Result<WorkspaceChanges>` and `Harness::turn_changes(session_id) -> Result<TurnChanges>`, with `FileChange { path, status, added, removed, patch, truncated }`, `ChangeStatus` (added, modified, deleted, renamed), `WorkspaceChanges { git, files }` and `TurnChanges { checkpoint, files }`, all defined only in core, all `Serialize`, exported from `harness-core`. Both run git and wait, so the bridge calls them on a blocking thread.
- **Patches and counts:** `patch` is the file's unified diff exactly as git prints it, headers included, so a renamed file's old path is in its `rename from` line; `None` for a binary file. Counts are git's own `--numstat`, never a scan of the patch, because a hunk line like `--- sql comment` (a removed line starting with `--`) cannot be told from a file header by its prefix; a test with exactly such lines pins it, and a mutation that counts by skipping `---`/`+++` lines fails it. Binary files have counts 0 and no patch. A patch over 256 KiB is cut at a line boundary with `truncated: true` and the counts stay exact; git is killed once past the limit, so memory is bounded by the limit, not the file.
- **Workspace changes:** against `HEAD` plus untracked files. `git: false` (and no files) outside a git repository or when git is missing. A repository with no commits is compared against the empty tree, so its staged and untracked files count as added (a fresh `git init` workspace is common). `--relative` keeps a workspace that is a subfolder of a repository to its own files, with paths relative to it. Untracked entries that are directories (an embedded repository is listed by git as `dir/`) are skipped. File names are read with `-z`, so spaces and non-ASCII names come back exact. Tracked files come first in git's order, then untracked ones.
- **Turn changes:** the session's last turn against its checkpoint: the stash snapshot's tree (or `HEAD` when the worktree matched it), so edits made before the turn are excluded, `bash` edits are included, and files created during the turn are listed as added (untracked now and not in the checkpoint's untracked list). `checkpoint: false` with no files when there is no last turn (this includes an unknown session, which is not an error), when the last turn has no checkpoint (a read-only turn, or a non-git workspace). Only the last turn counts: a read-only turn after an editing one gives `checkpoint: false`. After an undo (ticket 06) the files are back at the checkpoint, so it reports `checkpoint: true` with an empty list; a test pins that. The tests prove the baseline is the checkpoint and not `HEAD`: a file edited both before and during a turn shows only the turn's delta in the turn's patch and the combined delta in the workspace's.
- **Safety of running git in a workspace:** every diff passes `--no-ext-diff` and `--no-textconv`, since a repository's own config can name commands for both, and fixed `a/` `b/` prefixes so a user's diff config cannot change the patch format.
- **TUI:** the sidebar still shows per-file counts, now from core's list (it ignores the patches); a failure or no git shows an empty list as before. `git grep` finds no `fn workspace_changes` and no `fn git(` in `harness-tui`, and `FileChange` is defined once, in core.
- **Tests run:** `cargo fmt --check` (clean); `cargo test -p harness-core --test changes --locked` (12 passed); `make lint` (clean after fixing one clippy `collapsible_if` in my TUI edit); `make test` (169 passed, 0 failed; was 159, which is 12 new minus the 2 TUI tests that moved). Live smoke skipped on purpose (`AI_API_KEY` unset). Mutation checks, each restored byte-for-byte: the turn baseline ignoring the stash fails two tests; not excluding files untracked before the turn fails one; dropping `--relative` fails the subfolder test; removing the size limit, treating binaries as text, dropping the empty-tree fallback and counting by scanning the patch (the naive `---`/`+++` skip) each fail the test written for it.
- **Cost, measured on this machine (a throwaway benchmark, deleted):** about 26 ms for the git calls with nothing changed, about 170 ms with 30 modified files, and about 9 ms per untracked file (1.9 s with 30 modified plus 200 untracked). It is one git process per changed or untracked file, noted in the code with a `ponytail:` comment.
- **Not run:** CI on Linux; the isolated `cargo test -p harness-core --locked` (the known `mcp_fixture` issue from ticket 03). Not covered by a test: a file name that is not valid UTF-8 (paths are read lossily, so such a file would fail its per-file diff and fail the call), and a repository whose checkpoint object was pruned (the call fails with git's exit code).
- **Deviations from the ticket:** none to its acceptance criteria. Additions inside its intent: the methods return `Result` (an unreadable repository is an error, not an empty list, so a UI can show "couldn't load changes"); the empty-tree baseline, `--relative` and embedded-repository handling; `turn_changes` of an unknown session is `checkpoint: false`, matching `undo`.
- **Remaining risks:**
  - **TUI cost (decision for the user):** the TUI refreshes this list after every tool response, on its own thread, and now pays for patches it does not show: about 170 ms per refresh with 30 modified files, and about 2 s with 200 untracked files. The ticket asked for the TUI to use core's call, so it does; if it lags, add a counts-only call to core (the TUI helper's `ponytail:` comment names it).
  - Files that were already untracked when a turn started cannot be compared (the checkpoint does not store them), so edits to them are missing from the turn's changes, as undo cannot restore them either (noted in the code).
  - Rename detection applies to tracked files only; an untracked new file is just an added one.
  - Ticket 07 moves `common/mod.rs` into the testkit; its `git` and `commit_all` helpers go with it.

