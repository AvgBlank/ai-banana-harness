# 06: Durable undo marker

**Lane / milestone:** R · M1 Core additions   **Size:** S (one agent session)

**What to build:** Undo records a durable `turn.undone` event so an undone turn stays undone after a restart and cannot be undone twice (ticket 07). The TUI keeps working.

**Blocked by:** 04

**Status:** done 2026-10-17. All five acceptance criteria verified by tests that were run; `make lint` and `make test` passed locally (159 tests, 0 failed) with the build output in a temporary directory outside the repository. Not run: CI on Linux (macOS only here).

**Files and components likely to change:**
- `crates/harness-core/src/{event.rs,harness.rs}` (`EventKind::TurnUndone { turn_id }`, `name()` and `thread_id()`; `Harness::undo` writes the event to the undone turn's log with the same id generator and returns it with the outcome; a last turn that already has it is `NothingToUndo`)
- `crates/harness-tui/src/app.rs` (no-op arms for the new variant)
- `crates/harness-core/tests/checkpoint.rs`, `transcript.rs`

**Acceptance criteria:**
- [x] `Harness::undo` returns the outcome and, when restored, the `turn.undone` event; the event is persisted.
- [x] Undoing twice restores once; the second call is `NothingToUndo`; `transcript` shows the event exactly once.
- [x] A turn with no checkpoint is `NothingToUndo`.
- [x] The TUI's `/undo` still reports correctly and its event matches are exhaustive.
- [x] No schema migration is added.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-core --test checkpoint --test transcript --locked
cargo test -p harness-tui --locked
make lint
make test
```

**Documentation:** None (`undone turn` is already in `CONTEXT.md`).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.

**Completion record (2026-10-17):**
- **Changed:** `crates/harness-core/src/{event.rs,harness.rs,lib.rs}`, `crates/harness-tui/src/{app.rs,lib.rs}`, and tests in `crates/harness-core/tests/{checkpoint.rs,transcript.rs}` and `crates/harness-tui/tests/transcript.rs`. No store change, no migration (`MIGRATIONS` still has its six entries and the migrations directory is untouched).
- **Event:** `EventKind::TurnUndone { turn_id }`, wire name `turn.undone`, a turn-level event (`thread_id()` is `None`). It is written to the undone turn's own log after that turn's `turn.done`, so it is the last event of that turn in a replayed transcript. It never appears on a live turn stream.
- **`Harness::undo(session_id) -> Result<UndoReport>`:** `UndoReport { outcome: UndoOutcome, event: Option<Event> }` is new, so the `UndoOutcome` string shape pinned in ticket 03 is unchanged. It serializes as `{"outcome": "restored", "event": {...}}`, and `event` is omitted unless the workspace was restored. The TUI uses `.outcome`; `confirm_undo` is unchanged.
- **Semantics:**
  - `NothingToUndo` (no event written) when the session has no turn, when the last turn already has a `turn.undone` event, or when it has no checkpoint (it never mutated the workspace, or the workspace is not a git repository).
  - Otherwise the checkpoint is restored, then the event is written with an id from the same monotonic generator (a test checks it sorts after everything the turn wrote), and returned.
  - Already-undone is detected from the stored events' `kind` column, so a restart changes nothing: a new harness on the same database still refuses a second undo and does not touch work done since.
  - Undo still targets only the session's last turn, and the marker is per turn: after an undo, a later turn that edits files gets its own checkpoint and can itself be undone.
  - Restore comes before the write, so a failure never marks a turn undone that was not. If the write fails after a successful restore, the error says so ("the workspace was restored but the undo could not be recorded"), and a retry would restore the same checkpoint again.
- **Replay:** `transcript` needed no change; `turn.undone` is an ordinary stored event, replayed as a typed `EventKind::TurnUndone` in write order, exactly once. A hand-crafted-row test pins its wire shape (`{"id", "type": "turn.undone", "turn_id"}`).
- **TUI:** `apply_event` and `apply_thread_event` list the variant as an explicit no-op, so both matches stay exhaustive (the compiler enforces it); the TUI shows its own undo message from the outcome, and a test confirms a stray `turn.undone` event changes nothing on screen.
- **Tests run:** `cargo fmt --check` (clean); `cargo test -p harness-core --test checkpoint --test transcript --locked` (7 and 12 passed); `cargo test -p harness-tui --locked` (all passed); `make lint` (clean); `make test` (159 passed, 0 failed; was 152, so +7 new). Live smoke skipped on purpose (`AI_API_KEY` unset).
- **Mutation checks** (each restored byte-for-byte): removing the already-undone check fails the double-undo and restart tests; not persisting the marker fails four tests (persistence, double-undo, per-turn and restart).
- **Not run:** CI on Linux; the isolated `cargo test -p harness-core --locked` (known fixture issue from ticket 03, not caused by this change). The TUI's `/undo` command path inside the terminal run loop has no test (the loop needs a terminal); that change is a one-field access, `.outcome`, verified by compilation, and the rendering of the result is the unchanged `confirm_undo`.
- **Deviations from the ticket:** none. Adding `UndoReport` follows the ticket's "returns it with the outcome".
- **Remaining risks:** the check-then-write is not atomic (two overlapping undos of one session could both restore), marked with a `ponytail:` comment; the bridge must refuse undo while a turn runs in the workspace (ticket 11 already requires it), because `undo` itself would restore the checkpoint of a turn still in progress; the failure window between restore and recording is described above.

