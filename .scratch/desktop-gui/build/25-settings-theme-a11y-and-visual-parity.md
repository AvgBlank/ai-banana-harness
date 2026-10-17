# 25: Settings, theme, accessibility and the visual parity review

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** M (one agent session)

**What to build:** Finish the product surface and gate it on the user's visual review against the approved prototype. Shortcuts are limited to the keys that exist (Enter to send, Shift+Enter for a newline, Esc to close dialogs); the global shortcuts shown in the prototype were never specified and are not part of V1.

**Blocked by:** 20, 21, 22, 23, 24

**Status:** built; waiting for the user's visual review (the human gate)

**Files and components likely to change:**
- `gui/src/renderer/components/settings/SettingsView.tsx`, any remaining screens
- `gui/tests/e2e/visual.spec.ts` (screenshots of the 19 prototype states at 1440 by 900 and 1100 by 720, saved as artifacts, not compared pixel for pixel)

**Acceptance criteria:**
- [x] Settings has Appearance (System, Dark, Light as live preview cards), API key status (never the value, "Check again"), trusted workspaces with Forget, the shortcuts that exist, and About (app version, protocol 1).
- [x] Both themes work across every screen; `make gui-lint` has zero Biome findings and `check-tokens` passes; focus rings, Esc handling and dialog default focus match the prototype.
- [ ] The production app's screenshots for the 19 states exist in both themes and both sizes next to the prototype's; the user reviews them and approves (a gate, recorded in this ticket); any difference is a concrete bug, not a redesign.
- [x] No voice input, update or marketing badges, Monaco, global shortcuts or other excluded features exist (a checklist in the ticket is ticked).
- [x] No visible "chat" or "thread" label in any state (rendered-text check across all states).

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** Applies from the drafts: the `AGENTS.md` Settings-shortcuts clause (line 15).

**Notes:** The visual review is the human gate before packaging.

**Completion record:**
- Settings: Appearance (live preview cards), API key (presence and source only, never the value; **Check again** goes through the same recheck as the home screen), trusted workspaces with Forget, the keyboard shortcuts that exist (Enter, Shift+Enter, Esc), and About (app name and version from main, protocol 1). The sections share one `Block`; `ui/kbd.tsx` is new. Esc now closes the undo dialog (Keep changes) and the trust dialog (Open in restricted mode); approvals and questions still ignore it. The sidebar marks a workspace whose config is not trusted with a Restricted badge, as the prototype does. `AGENTS.md` carries the Settings-shortcuts clause and the README lists the desktop keys.
- Visual review: `tests/e2e/visual.spec.ts` drives the real app through the prototype's states and writes `<theme>-<state>.png` under `<size>/` (1440 by 900 and 1100 by 720, dark and light) to `VISUAL_OUT` (default `gui/test-results/visual`) with a manifest per theme. 17 of the 19 states are captured: 68 screenshots were written to `.scratch/desktop-gui/production-screenshots/` next to the prototype's `prototype/screenshots/`, and left uncommitted because they are generated. Quit while running and External link are native OS dialogs in the real app, so there is no page to capture. The same test reads the rendered text of every state at both sizes and fails on "chat" or "thread".
- Findings from comparing a sample (settings, workspace trust, last-turn changes) with the prototype: the sidebar had no Restricted badge (fixed); the Appearance hint says the choice is remembered rather than that the app follows the system, because the default theme is dark; the shortcut list is shorter by design (only keys that exist); the screenshots have no macOS traffic lights because Playwright captures the page, not the window; the "View in changes" button on a tool card is a plain text button where the prototype has none. Only three states were compared by eye; the other pairs have not been looked at.
- Checklist of excluded features (a unit test, `tests/unit/shared/exclusions.test.ts`, keeps it true): [x] no voice input, [x] no update check or marketing badge, [x] no Monaco, [x] no global shortcut.
- Not done: the user's review and approval of the screenshots (the third acceptance criterion stays open). A real Rust panic in the sidecar's stderr tail contains the word "thread" (`thread 'main' panicked`); the banner shows that text verbatim, and the screenshot uses a message without it.
- Checked: `make gui-lint`, `make gui-test` (358), `make gui-build`, `make gui-e2e` (44, including the visual test for both themes), `make gui-leak-scan`, `make lint`, `make test` (258 Rust).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
