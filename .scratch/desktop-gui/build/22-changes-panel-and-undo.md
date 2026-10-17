# 22: Changes panel, summary actions and undo

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** L (one agent session)

**What to build:** The Changes panel with both scopes, its refresh rules, and the undo flow with the confirmation dialog and composer hint (ticket 07 sections 6 and 7). Split point if over budget: panel (22a) then undo (22b).

**Blocked by:** 18, 11

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/renderer/state/changes.ts`, `components/changes/{ChangesPanel.tsx,DiffView.tsx}`, `components/dialogs/UndoDialog.tsx`, `components/session/TurnSummary.tsx`
- `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts`

**Acceptance criteria:**
- [ ] Last turn and Workspace scopes use `changes.turn` and `changes.workspace`; refresh is single-flight with a 400 ms trailing debounce, runs after `edit`, `write_file` and `bash` responses while the panel is open or the badge shows, immediately on `turn.done`, after undo, on focus and on scope or session change; the previous list stays visible while refreshing.
- [ ] States: loading with a stale list, error with Retry, not a git repository, no checkpoint, empty, truncated patch, binary file, deleted and renamed; the selected file survives refresh and falls back when it disappears; "View changes" and a card path open the right scope and file; panel open state, width and scope persist (default closed).
- [ ] The lightweight diff view only (no Monaco), windowed with `content-visibility` for long diffs; header totals match the list.
- [ ] Undo is offered only for the session's last finished turn that has a checkpoint, is not undone and has no running turn in the workspace; clicking it refreshes `changes.turn` and opens the confirmation (files listed, **Keep changes** focused); on confirm `session.undo` runs once, `restored` marks the turn undone from the returned `turn.undone` event, refreshes both scopes and shows the composer hint; `nothing_to_undo` hides the button; an error keeps it with the message.
- [ ] After a reload the undone turn (from the replayed event) offers no undo; the agent's context is not changed.
- [ ] e2e `edit-and-undo`: both scopes list the `write_file` and a `bash` edit, undo restores the file system, and the turn shows Undone after a reload.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 07 sections 6 and 7.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
