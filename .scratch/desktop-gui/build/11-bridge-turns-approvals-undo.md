# 11: harness-bridge: turns, events, approvals, undo

**Lane / milestone:** B · M3 Sidecar   **Size:** L (one agent session)

**What to build:** The turn half of the protocol: starting and cancelling turns, forwarding events with routing metadata, required actions, undo, and clean behaviour on shutdown. Split point if over budget: turns and events (11a) then approvals, undo and shutdown (11b).

**Blocked by:** 10, 06

**Status:** open (not started)

**Files and components likely to change:**
- `crates/harness-bridge/src/{turns.rs,methods/*.rs}`
- `crates/harness-bridge/tests/*.rs`, `tests/fixtures/*.ndjson`

**Acceptance criteria:**
- [ ] `turn.start` validates the workspace and session, returns `{}`, and the turn's events follow as `event` frames carrying `workspace`, `session_id` and `turn_id` (`null` only before `turn.created`), in order, ending with `turn.done`.
- [ ] One running turn per workspace: a second `turn.start` is `turn_in_progress` with the session id in `data`; `turn.cancel` returns `{cancelled}`.
- [ ] `action.reply` accepts `allow`, `allow_for_session`, `deny` and `{text}`; an id that is not pending is `unknown_action`; pending ids are tracked and appear in `workspace.list` `active_turn` together with the turn id.
- [ ] `session.undo` returns `{outcome, event?}` (the `turn.undone` event when restored) and is refused with `turn_in_progress` while a turn runs.
- [ ] Cancel resolves pending actions as denied and ends `cancelled`; closing a workspace cancels only its turn; shutdown cancels every turn and waits up to 3 s.
- [ ] The API key never appears in any frame or captured stderr, even when a tool echoes it.
- [ ] Golden frames cover every event kind and the new methods.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-bridge --locked
make lint
make test
```

**Documentation:** None.

**Notes:** Ticket 03 section 4; ticket 07 amendments for undo.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
