# 11: harness-bridge: turns, events, approvals, undo

**Lane / milestone:** B · M3 Sidecar   **Size:** L (one agent session)

**What to build:** The turn half of the protocol: starting and cancelling turns, forwarding events with routing metadata, required actions, undo, and clean behaviour on shutdown. Split point if over budget: turns and events (11a) then approvals, undo and shutdown (11b).

**Blocked by:** 10, 06

**Status:** done

**Files and components likely to change:**
- `crates/harness-bridge/src/{turns.rs,methods/*.rs}`
- `crates/harness-bridge/tests/*.rs`, `tests/fixtures/*.ndjson`

**Acceptance criteria:**
- [x] `turn.start` validates the workspace and session, returns `{}`, and the turn's events follow as `event` frames carrying `workspace`, `session_id` and `turn_id` (`null` only before `turn.created`), in order, ending with `turn.done`.
- [x] One running turn per workspace: a second `turn.start` is `turn_in_progress` with the session id in `data`; `turn.cancel` returns `{cancelled}`.
- [x] `action.reply` accepts `allow`, `allow_for_session`, `deny` and `{text}`; an id that is not pending is `unknown_action`; pending ids are tracked and appear in `workspace.list` `active_turn` together with the turn id.
- [x] `session.undo` returns `{outcome, event?}` (the `turn.undone` event when restored) and is refused with `turn_in_progress` while a turn runs.
- [x] Cancel resolves pending actions as denied and ends `cancelled`; closing a workspace cancels only its turn; shutdown cancels every turn and waits up to 3 s.
- [x] The API key never appears in any frame or captured stderr, even when a tool echoes it.
- [x] Golden frames cover every event kind and the new methods.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-bridge --locked
make lint
make test
```

**Documentation:** None.

**Notes:** Ticket 03 section 4; ticket 07 amendments for undo.

**Completion record:**
- New in `harness-bridge`: `turns.rs` (per-workspace `Busy` state: idle, undoing or one running turn with its id, pending action ids and shared `TurnHandle`; a forwarding task per turn) and the methods `turn.start`, `turn.cancel`, `action.reply` and `session.undo` in `methods.rs`. `workspace.list` now reports `active_turn {session_id, turn_id, pending_action_ids}`. New codes: `turn_in_progress` (with `data.session_id`), `no_active_turn`, `unknown_action`. Error responses can carry `data`.
- Behaviour: the forwarder records `turn_id` and pending ids before queueing each frame, and clears the workspace to idle while holding the state lock that queues `turn.done`, so a client that has seen `turn.done` can start the next turn at once (tested 10 times in a row) and a new turn cannot overtake it. Undo reserves the workspace like a turn does. `workspace.close` cancels only its turn and waits up to 3 s for it; shutdown, `SIGTERM` and `SIGINT` cancel every turn, wait up to 3 s, and flush the closing frames before the writer ends. A reader that stops reading cancels its turn instead of running it unseen. No turn is ever resent.
- Testkit: `driver.rs` (`Transport`, `run_scenario`) runs a scenario's steps and checks `expect` (events, requests, request contents, files, provider leftovers, routing) over any transport; the bridge tests and the real sidecar process use it. The testkit's `TestEnv` user config now makes edits and commands ask (deny rules stay), because the product default allows both and no scenario could otherwise reach an approval. The scenarios gained verified `expect.events`, and `edit-and-approve` now asks about `touch deployed.txt` because `git push*` is denied outright and never asks.
- Golden frames: `turns.ndjson` (plain answer, approval, question, single subagent, undo, compaction), `turn-errors.ndjson` (overlap, unknown action, undo refused, cancel). Kinds covered: every event kind except `mcp.auth_required`, which needs a remote OAuth server and is forwarded like any other event; `turn.undone` appears in the `session.undo` response.
- Security: an untrusted workspace's MCP command does not run during a turn (a trusted control run does run it); the key is absent from frames, the database and stderr, including when a tool echoes it and over the real process. Provider destinations are reported (scheme and authority) and are not blocked in the bridge: the post-open block for non-HTTPS non-loopback destinations belongs to Electron main (ticket 06), so that guarantee depends on the GUI tickets. The bridge cannot stop a user config from pointing at such a destination.
- Deviations: the shutdown future from ticket 09 now also stops turns; `session.undo` returns core's `UndoReport` unchanged. Not covered: `mcp.auth_required` golden, child processes started by a cancelled `bash` tool (core owns their cleanup), Linux.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
