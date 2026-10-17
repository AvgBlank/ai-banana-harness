# 22: Changes panel, summary actions and undo

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** L (one agent session)

**What to build:** The Changes panel with both scopes, its refresh rules, and the undo flow with the confirmation dialog and composer hint (ticket 07 sections 6 and 7). Split point if over budget: panel (22a) then undo (22b).

**Blocked by:** 18, 11

**Status:** done

**Files and components likely to change:**
- `gui/src/renderer/state/changes.ts`, `components/changes/{ChangesPanel.tsx,DiffView.tsx}`, `components/dialogs/UndoDialog.tsx`, `components/session/TurnSummary.tsx`
- `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts`

**Acceptance criteria:**
- [x] Last turn and Workspace scopes use `changes.turn` and `changes.workspace`; refresh is single-flight with a 400 ms trailing debounce, runs after `edit`, `write_file` and `bash` responses while the panel is open or the badge shows, immediately on `turn.done`, after undo, on focus and on scope or session change; the previous list stays visible while refreshing.
- [x] States: loading with a stale list, error with Retry, not a git repository, no checkpoint, empty, truncated patch, binary file, deleted and renamed; the selected file survives refresh and falls back when it disappears; "View changes" and a card path open the right scope and file; panel open state, width and scope persist (default closed).
- [x] The lightweight diff view only (no Monaco), windowed with `content-visibility` for long diffs; header totals match the list.
- [x] Undo is offered only for the session's last finished turn that has a checkpoint, is not undone and has no running turn in the workspace; clicking it refreshes `changes.turn` and opens the confirmation (files listed, **Keep changes** focused); on confirm `session.undo` runs once, `restored` marks the turn undone from the returned `turn.undone` event, refreshes both scopes and shows the composer hint; `nothing_to_undo` hides the button; an error keeps it with the message.
- [x] After a reload the undone turn (from the replayed event) offers no undo; the agent's context is not changed.
- [x] e2e `edit-and-undo`: both scopes list the `write_file` and a `bash` edit, undo restores the file system, and the turn shows Undone after a reload.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 07 sections 6 and 7.

**Completion record:**
- State: `useChanges` (`state/changes.ts`) keeps both scopes for the open session with one request in flight per scope and one follow-up for any asks that arrive meanwhile, a 400 ms trailing debounce, and the previous list kept while loading. `useTranscripts` reports every frame's type and whether it answers `edit`, `write_file` or `bash`; the shell refreshes after those responses (debounced), at once on `turn.done` and after undo, on window focus, and when the session, scope or panel changes. `TurnView.undone` is set by `turn.undone`, live or replayed. Prefs gained `changesScope` and `changesWidth` (360 to 760), validated in main and in the IPC schema; `Client` gained `changesTurn`, `changesWorkspace` and `undo`.
- UI: `ChangesPanel` (scope select, totals, file list, resize handle, states for loading, error with Retry, not git, no checkpoint, empty, truncated, binary, deleted and renamed), `DiffView` windowed with `content-visibility`, the title-bar button shows the scope's totals, `TurnSummary` shows Undone or Undo and View changes, a tool card with a diff has "View in changes", `UndoDialog` lists the files with **Keep changes** focused, and a composer hint says the agent still remembers the turn. `ChangesPlaceholder` was deleted.
- Undo offered when: last turn ended, has a turn id, is not undone, `changes.turn.checkpoint` is true, and no turn runs in the workspace; `nothing_to_undo` hides it, an error keeps it with the message.
- Checked: `make gui-lint`; `make gui-test` (247 tests); `make gui-build`; `make gui-e2e` (27 passed; `edit-and-undo` approves a `write_file` and a `bash` edit, lists both in the Last turn and Workspace scopes, undoes, checks the file system, and checks Undone again after a reload); `make lint`; `make test` (258 Rust tests). One mutation (checkpoint check removed) was caught.
- Deviations: (1) the Changes panel on the home screen shows the empty state, since there is no session to ask about. (2) The hint is per session and lasts until the next message; it is not stored.
- Not covered: Linux; sessions not on screen are not refreshed; the flaky core test `a_command_past_its_timeout_is_killed_with_its_process_group` remains an open, unexplained risk.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
