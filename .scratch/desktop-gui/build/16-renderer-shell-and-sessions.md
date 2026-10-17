# 16: Renderer shell, theme and the session list

**Lane / milestone:** G · M4 GUI foundation   **Size:** M (one agent session)

**What to build:** Port the approved shell from the prototype with the accessibility fixes: title bar, icon rail, sidebar with sessions grouped by workspace, the home layout, primitives and theme switching. Connected to the real `session.list` through the preload API; everything else renders from the typed client with fakes in tests. No visual changes from the prototype.

**Blocked by:** 15

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/renderer/{main.tsx,App.tsx}`, `components/{ui,shell,home}/`, `state/store.ts` (UI slice only), `lib/cn.ts`
- `gui/tests/unit/renderer/*` (React Testing Library)

**Acceptance criteria:**
- [ ] Components are carried over from `.scratch/desktop-gui/prototype/src/components` with the 40 Biome findings fixed (every `<button>` typed, semantic elements, labels with controls) and no change to layout, spacing, type or colours.
- [ ] Theme: System, Dark and Light work for every ported component; the preference is applied before first paint through the preload API and persisted by main.
- [ ] The sidebar lists sessions from the real `session.list`, grouped by workspace, with relative times, the running dot and the interrupted marker driven by state; the layout rule from ticket 07 holds at 1100 by 720 (sidebar hidden below 1280 px while Changes is open).
- [ ] The interface never says "chat" or "thread" (checked on rendered text in component tests).
- [ ] `make gui-lint` has zero Biome findings; `check-tokens` passes; Vitest covers the shell states.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e   # the hardening spec still passes with the real shell
```

**Documentation:** None.

**Notes:** Visual reference: `.scratch/desktop-gui/prototype/` (screens 1 and 3).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
