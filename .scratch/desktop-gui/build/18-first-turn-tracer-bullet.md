# 18: First turn: composer to streamed answer (tracer bullet)

**Lane / milestone:** G · M5 First session   **Size:** L (one agent session)

**What to build:** The thin end-to-end slice that proves the whole stack: type a prompt, run it through the real sidecar against the fake provider, and watch the streamed answer. Starts the reducer with message and turn lifecycle only, plus resume, session switching and rename. Split point if over budget: reducer core and replay (18a) then composer, streaming and the first e2e (18b).

**Blocked by:** 17, 11

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/renderer/state/{reduceTurn.ts,store.ts,selectors.ts}` (events: `turn.created`, `model.message` start, delta, final, `turn.done`; dedupe by `event.id`; the final message is authoritative; no clock)
- `gui/src/renderer/components/{home/Composer.tsx,session/SessionView.tsx}`
- `gui/src/main/ipc/*` (`rpc.call` for the turn and session methods), `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts` (the first spec)

**Acceptance criteria:**
- [ ] Reducer tests: replay equivalence for `plain-answer` (live stream equals persisted events only, apart from `seen`); a final `model.message` with no start marker creates the message; deltas applied twice or overlapping a resumed prefix give the same state; an empty final is removed.
- [ ] The composer sends optimistically (`turn_id` null until `turn.created`), is disabled while the workspace has a running turn ("A turn is running in this workspace"), restores its text and shows the error if `turn.start` fails, and Stop sends `turn.cancel` ("Stopping..." until `turn.done`).
- [ ] Streaming renders one flush per animation frame; the transcript sticks to the bottom and shows "Jump to latest" when scrolled up.
- [ ] Resume: opening a session runs `session.resume` and renders its turns; switching sessions and renaming (optimistic, with revert on error) work; the sidebar auto-title updates after `turn.created` and `turn.done`.
- [ ] e2e `plain-answer` through the real sidecar and the stub: the answer is visible and the stub saw the expected request (model-facing assertions identical to the in-process seams); `provider-error-401` shows the error on the turn and the hint to switch provider.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Milestone check M5: this ticket is the tracer bullet. Specification: ticket 07 sections 2 to 4.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
