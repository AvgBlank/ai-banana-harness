# 12: Safety net: limits, doom loop, checkpoint and undo

**What to build:**

**Turn limits:** per-turn limits of 100 LLM calls and 30 minutes of wall clock, both configurable. Hitting a limit ends the turn with a clear error.

**Doom loop:** three identical consecutive tool calls inject an internal warning to the model; a fifth ends the turn.

**Checkpoint:**
- Before a turn's first mutating call in a git workspace, record a stash-style snapshot ref in the database.
- `/undo` restores the workspace to it and removes the files the turn created.
- The TUI confirms the undo.

**Blocked by:** 04

**Status:** done

- [x] Seam-1 test: a model that keeps calling tools stops at the configured iteration limit, with an error.
- [x] Seam-1 test: repeated identical calls produce the warning after three and end the turn after five.
- [x] Seam-1 test: after a turn edits one file and creates another, undo restores the edited file and removes the created one.
- [x] Seam-1 test: in a non-git workspace, no checkpoint is taken and undo reports that nothing is available.

## Notes

For whoever merges ticket 11 next to this branch, and for 13/14 (`/help`, live smoke).

**Turn limits (`harness-core/src/safety.rs`):** `AgentConfig` gained `max_llm_calls`/`max_turn_minutes`
(`config.rs`; `default.toml` sets 100/30). `TurnShared` (`harness.rs`) gained a `limits: TurnLimits`
field, constructed once per turn in `execute` and shared by the root thread and every subagent
(they all count against the same budget — a deliberate reading of "per-turn": a runaway fan-out of
subagents shouldn't get its own separate 100-call allowance). `TurnLimits::check()` runs at the top
of every `run_thread` loop iteration, before the LLM call; wall-clock is `Instant::now()` at turn
start, calls is an `AtomicU32`. Either breach `bail!`s a clear error, which — like every other
`run_thread` error — becomes `turn.done{state: error}` via `TurnRunner::run`'s existing catch-all.
No new event type was needed.

**Doom loop (`safety.rs::DoomLoop`):** per-thread (not shared like `TurnLimits`), a
run-length count of consecutive identical `(name, arguments)` tool calls, folded in inside the
existing `for call in &completion.tool_calls { emit(ToolCall...) }` loop in `run_thread`
(`harness.rs`). The 3rd identical call queues a warning; the 5th `bail!`s immediately (before that
round's tools ever run — `run_tool_calls` is only reached if `DoomLoop::note` doesn't return `End`).
The queued warning is pushed as a `<bh-internal>`-wrapped `Role::User` message right after that
round's tool-response messages (so the model sees its own tool result before the warning), reusing
compaction's `INTERNAL_START`/`INTERNAL_END` constants (now `pub(crate)` in `compaction.rs` instead
of private) and its `<bh-internal>`-message convention (already skipped by the compaction
transcript serializer). No `doom_loop` permission-config key was added: ticket 04's note speculated
one might be needed, but doom-loop enforcement is a loop-level safety net, not a per-tool
allow/ask/deny decision, so it isn't configurable and isn't part of `PermissionConfig`.

**Checkpoint (`harness-core/src/checkpoint.rs`, new module; `harness-store` migration `0005.sql`,
new `checkpoint` table):** captured in `TurnRunner::run_tool_calls` (`harness.rs`), which now opens
with `checkpoint_if_first_mutation(shared, calls)` — checks each round's calls against
`MUTATING_TOOLS = ["write_file", "edit", "bash"]` (mirrors `harness-tui`'s
`ToolCallItem::expandable()` set) and, on the turn's first match, flips `TurnShared`'s
`checkpoint_taken: AtomicBool` (an atomic `swap`, so two subagents racing on their first mutating
call in the same instant still checkpoint exactly once) and runs `checkpoint::capture` in
`spawn_blocking` (it shells out to `git`, like `harness-tui::workspace_changes` already does).
`capture` records `git rev-parse HEAD`, a `git stash create` (tracked-changes-vs-HEAD as a commit,
without touching the index/worktree/stash list — `None` when there was nothing to stash, i.e. the
worktree matched `HEAD`) anchored under `refs/banana-harness/checkpoint/<turn_id>` so it survives a
`git gc` between the checkpoint and a later `/undo`, plus the workspace's untracked files at that
moment. `Harness::undo(session_id)` (`harness.rs`) looks up the session's **last** turn specifically
(not "the most recent checkpoint row", which would wrongly reach further back if that last turn
never mutated anything) and returns `UndoOutcome::NothingToUndo` if it has no checkpoint row —
covering both "non-git workspace" (`capture` returns `None` there, nothing is ever inserted) and "a
turn that only read/searched." `checkpoint::restore` does `git checkout <stash_ref or head_sha> --
.` (restores every tracked path to its checkpoint content) then deletes any currently-untracked
file that wasn't in the checkpoint's untracked list (i.e. the turn created it).
- `ponytail:` `restore` doesn't handle a tracked file the turn deleted or renamed — only the
  edited/created shapes the acceptance tests (and the ticket) ask for. Add `checkout`'s companion
  deletion pass (diff `HEAD`'s tree against the checkpoint tree for tracked removals) if a later
  ticket needs it.
- **TUI (`harness-tui`):** `/undo` is a new `Action::Undo` (`lib.rs`, alongside `/new`/`/provider`),
  calling `Harness::undo` synchronously (git shelling on the UI thread, same trade-off
  `workspace_changes` already makes) and refreshing `modified_files` before
  `MainState::confirm_undo` (`app.rs`) pushes a `TranscriptItem::System` confirming which outcome
  happened. Added to `HELP_COMMANDS` (`ui.rs`) per ticket 13's note.

**Migration-number collision:** this branch's `checkpoint` table is `harness-store/migrations/
0005.sql` (the highest existing was `0004.sql`, ticket 08's thread-title column). Ticket 11 is
running in parallel from the same base and will very likely also add a migration (an
`mcp_oauth_token` table) — if it also picks `0005.sql`, merging both branches will hit an add/add
conflict on that filename and (mechanically) on `MIGRATIONS` in `harness-store/src/lib.rs`; resolve
by renumbering one of them to `0006.sql` (whichever merges second) and re-ordering `MIGRATIONS`
to match. The two tables are otherwise independent (no shared columns/foreign keys), so this is a
pure rename, not a real conflict.

**Tests:** `harness-core/tests/safety.rs` (iteration limit: `max_llm_calls = 3` via
`TestHarness::open_configured`, four queued identical-shape-but-different-path calls, asserts the
error names "3" and that only 3 requests were sent; doom loop: five identical `read_file` calls,
asserts the error names "doom loop", that exactly 5 requests were sent — the 5th is only known
once its response arrives, so it's the call whose result triggers the end, never itself run — and
that request 4's body carries the `<bh-internal>...3 times...` warning queued after request 3) and
`harness-core/tests/checkpoint.rs` (edit + create, then `undo` restores/removes; a workspace with
`.git` removed after `TestHarness::open` gets `NothingToUndo` and leaves the created file alone).
