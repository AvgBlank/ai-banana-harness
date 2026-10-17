# 04: Transcript replay

**Lane / milestone:** R · M1 Core additions   **Size:** M (one agent session)

**What to build:** `Harness::transcript(session_id)` returns a session's turns with their typed events, so a resumed session can be shown. Core owns converting stored rows to `Event`; callers never parse store rows.

**Blocked by:** 01

**Status:** done 2026-10-17. All six acceptance criteria verified by tests that were run; `make lint` and `make test` passed locally (152 tests, 0 failed) with the build output in a temporary directory outside the repository. Not run: CI on Linux (macOS only here).

**Files and components likely to change:**
- `crates/harness-core/src/{harness.rs,event.rs,lib.rs}` (`TurnRecord`; fix the doc on `EventKind::ModelMessage`: the start marker is streamed only, never persisted)
- `crates/harness-core/tests/transcript.rs` (new)

**Acceptance criteria:**
- [x] `TurnRecord { turn_id, seq, input, state: Option<TurnState>, error, usage, events }` derives `Serialize`; `state` is `None` for a turn left `running` in the database (an interrupted turn).
- [x] After two turns (one with a tool call, one cancelled) `transcript` returns both in order with their events, no `model.message.delta`, and the right state, usage and error.
- [x] A harness reopened on the same database returns the same transcript.
- [x] A corrupted event row makes the call fail with context naming the turn and the event; rows are never silently skipped.
- [x] A turn row left `running` appears with `state: None`.
- [x] The `ModelMessage` doc comment now says the start marker is live-only.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-core --test transcript --locked
make lint
make test
```

**Documentation:** None (the terms are already in `CONTEXT.md` from ticket 1).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.

**Completion record (2026-10-17):**
- **Changed:** `crates/harness-core/src/{harness.rs,event.rs,lib.rs}` and the new `crates/harness-core/tests/transcript.rs` (11 tests). No store change was needed: `Store::turns` and `Store::events` already return what is required, and they stay private to core's conversion.
- **API:** `Harness::transcript(session_id) -> Result<Vec<TurnRecord>>` and `TurnRecord { turn_id, seq, input, state: Option<TurnState>, error, usage: Option<Usage>, events: Vec<Event> }` (derives `Serialize`, plus `Clone`, `Debug`, `PartialEq`), exported from `harness-core`. It reuses core's `Event`, `TurnState` and `Usage` and the store's `i64` `seq`; no mirror types. `TurnState` gained `FromStr`, the inverse of `as_str`, so the text mapping has one owner.
- **Replay behaviour:**
  - Turns come in `seq` order; each turn's events come in the order they were written, which is the order the live stream delivered them (an event is stored, then sent). The test proves it: each turn's replayed events are exactly the live events minus the never-stored ones, with the same ids in the same order.
  - Streamed-only events are absent: every `model.message.delta` and each message's start marker. The final `model.message` carries the authoritative text. Note on wording: `model.message` is one event name with two forms; the start marker (`message: null`) is live-only and the final message (with `content`) is the persisted one.
  - A turn row still `running` (the process stopped) has `state: None`, with its error and usage `None` and whatever events it wrote.
  - `usage` is the turn's last model call's usage as stored, not a sum over calls (a test with a two-call turn pins it).
  - Corruption fails the whole call and is never skipped, with context naming the turn and, for events, the event kind and id: an event payload that does not parse; a payload whose `id` or kind disagrees with its own row (the row repeats both in columns); a state string that is not `done`, `error`, `cancelled` or `running`; an unreadable `usage_json`.
  - An unknown session is an error ("not found", like `rename_session`); an existing session with no turns returns an empty list. The ticket was silent on this; a typo should not look like empty history.
- **Doc fix:** the `EventKind::ModelMessage` comment now says the start marker is live-only and a replayed transcript has none.
- **Tests run:** `cargo fmt --check` (clean); `cargo test -p harness-core --test transcript --locked` (11 passed); `make lint` (clean); `make test` (152 passed, 0 failed; was 141, so +11 new; the five `mcp.rs` tests that need the workspace-built `mcp_fixture` example pass under it). Live smoke skipped on purpose (`AI_API_KEY` unset). Mutation checks (each restored byte-for-byte): skipping unreadable rows fails two tests; dropping the row-versus-payload check fails its test; treating an unreadable state as unfinished fails its test; reversing the event order fails four tests.
- **One real failure found and fixed on the way:** the first `make lint` failed on clippy's `redundant_closure` in the new test file; fixed, and lint and tests were re-run on the final code.
- **Not run:** the isolated `cargo test -p harness-core --locked` (it is known to fail in a fresh build directory because `harness-mcp`'s `mcp_fixture` example is built only through the workspace test path, as recorded in ticket 03); CI on Linux.
- **Deviations from the ticket:** none. The consistency check between a row and its payload and the "unknown session" error are small additions inside the ticket's "corrupt rows fail" and "typed events" intent.
- **Remaining risks:** an event type written by a newer version and read by an older one will fail the whole transcript (deliberate: it is never silently skipped, but it makes a downgrade visibly break replay); the transcript loads a whole session at once (the plan accepts this, with paging by turn as the upgrade path if resume gets slow); like `rename_session`, the errors are plain `anyhow`, so the bridge may need typed errors to map "not found" and "corrupt" to different protocol codes.

