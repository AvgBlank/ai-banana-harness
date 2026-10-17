# 18: First turn: composer to streamed answer (tracer bullet)

**Lane / milestone:** G · M5 First session   **Size:** L (one agent session)

**What to build:** The thin end-to-end slice that proves the whole stack: type a prompt, run it through the real sidecar against the fake provider, and watch the streamed answer. Starts the reducer with message and turn lifecycle only, plus resume, session switching and rename. Split point if over budget: reducer core and replay (18a) then composer, streaming and the first e2e (18b).

**Blocked by:** 17, 11

**Status:** done

**Files and components likely to change:**
- `gui/src/renderer/state/{reduceTurn.ts,store.ts,selectors.ts}` (events: `turn.created`, `model.message` start, delta, final, `turn.done`; dedupe by `event.id`; the final message is authoritative; no clock)
- `gui/src/renderer/components/{home/Composer.tsx,session/SessionView.tsx}`
- `gui/src/main/ipc/*` (`rpc.call` for the turn and session methods), `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts` (the first spec)

**Acceptance criteria:**
- [x] Reducer tests: replay equivalence for `plain-answer` (live stream equals persisted events only, apart from `seen`); a final `model.message` with no start marker creates the message; deltas applied twice or overlapping a resumed prefix give the same state; an empty final is removed.
- [x] The composer sends optimistically (`turn_id` null until `turn.created`), is disabled while the workspace has a running turn ("A turn is running in this workspace"), restores its text and shows the error if `turn.start` fails, and Stop sends `turn.cancel` ("Stopping..." until `turn.done`).
- [x] Streaming renders one flush per animation frame; the transcript sticks to the bottom and shows "Jump to latest" when scrolled up.
- [x] Resume: opening a session runs `session.resume` and renders its turns; switching sessions and renaming (optimistic, with revert on error) work; the sidebar auto-title updates after `turn.created` and `turn.done`.
- [x] e2e `plain-answer` through the real sidecar and the stub: the answer is visible and the stub saw the expected request (model-facing assertions identical to the in-process seams); `provider-error-401` shows the error on the turn and the hint to switch provider.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Milestone check M5: this ticket is the tracer bullet. Specification: ticket 07 sections 2 to 4.

**Completion record:**
- `state/reduceTurn.ts` (pure, no clock): `turn.created`, `model.message` (start marker and final), deltas and `turn.done`, deduped by event id, the final message authoritative (a delta after it is ignored, an empty final is removed, a final or a delta with no marker creates the message), unknown events ignored but remembered; `turnFromRecord` rebuilds a stored turn and marks one with no stored state interrupted. `state/transcripts.ts` holds the views per session and the workspace's running turn: frames are queued and applied once per animation frame, the prompt shows at once (optimistic turn bound by `turn.created`), a failed start removes it, `Stop` reads "Stopping…" until `turn.done`. `useSessions` gained an optimistic rename that reverts on error, `add`, and `reload`, which runs when a turn is created and when it ends (the sidebar auto-title).
- UI: the composer works from home (creates the session, then starts the turn) and inside a session; it is disabled while the workspace has a running turn ("A turn is running in this workspace"), and on failure the text and the error come back. `SessionView` shows the prompt, the streamed text with a caret, errors (with the switch-provider hint for a 401), cancelled and interrupted turns, the bottom-sticking scroll and "Jump to latest" (sending reattaches). Choosing a session opens its workspace restricted and runs `session.resume`; `Client` gained `createSession`, `resume`, `startTurn`, `cancelTurn` and `onSidecarEvent`.
- Checked: `make gui-lint`; `make gui-test` (169 tests: reducer replay equivalence including the bridge's own plain-answer golden frames, idempotence, overlapping deltas, empty final; composer, stop, errors, resume, switching, rename revert, scrolling); `make gui-e2e` (20 passed; new `scenarios.spec.ts` runs plain-answer through the real sidecar and the stub and checks the stub saw one request, the key accepted and the prompt in it, a reload and resume, provider-error-401 with the hint, and a rename kept by the sidecar). Mutations caught: dedupe, the final being authoritative, per-frame batching, and a failed start leaving the optimistic turn (after adding a test for the open-session case).
- Deviations: (1) turns render only the user prompt, the assistant text and turn-level notices; tool cards, approvals, the summary card and Undo belong to later tickets. (2) A session of a workspace that cannot be opened (no API key) cannot be resumed, because `session.resume` needs an open workspace and opening needs a key; the view says so. (3) `TurnView` holds a flat list of assistant messages, not per-thread lists, until subagents arrive. (4) Switching a session's provider through the picker is not wired; the picker's choice applies to the next new session. (5) Only the last-started turn's running state comes from the sidecar after a reload (`workspace.list`); a turn started before the reload reads as running in the sidebar but its live frames are not replayed (live attach is a later ticket).
- Not covered: Linux; long-session performance.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
