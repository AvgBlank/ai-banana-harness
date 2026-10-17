# 25: Settings, theme, accessibility and the visual parity review

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** M (one agent session)

**What to build:** Finish the product surface and gate it on the user's visual review against the approved prototype. Shortcuts are limited to the keys that exist (Enter to send, Shift+Enter for a newline, Esc to close dialogs); the global shortcuts shown in the prototype were never specified and are not part of V1.

**Blocked by:** 20, 21, 22, 23, 24

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/renderer/components/settings/SettingsView.tsx`, any remaining screens
- `gui/tests/e2e/visual.spec.ts` (screenshots of the 19 prototype states at 1440 by 900 and 1100 by 720, saved as artifacts, not compared pixel for pixel)

**Acceptance criteria:**
- [ ] Settings has Appearance (System, Dark, Light as live preview cards), API key status (never the value, "Check again"), trusted workspaces with Forget, the shortcuts that exist, and About (app version, protocol 1).
- [ ] Both themes work across every screen; `make gui-lint` has zero Biome findings and `check-tokens` passes; focus rings, Esc handling and dialog default focus match the prototype.
- [ ] The production app's screenshots for the 19 states exist in both themes and both sizes next to the prototype's; the user reviews them and approves (a gate, recorded in this ticket); any difference is a concrete bug, not a redesign.
- [ ] No voice input, update or marketing badges, Monaco, global shortcuts or other excluded features exist (a checklist in the ticket is ticked).
- [ ] No visible "chat" or "thread" label in any state (rendered-text check across all states).

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** Applies from the drafts: the `AGENTS.md` Settings-shortcuts clause (line 15).

**Notes:** The visual review is the human gate before packaging.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
