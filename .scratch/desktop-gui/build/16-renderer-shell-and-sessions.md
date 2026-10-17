# 16: Renderer shell, theme and the session list

**Lane / milestone:** G · M4 GUI foundation   **Size:** M (one agent session)

**What to build:** Port the approved shell from the prototype with the accessibility fixes: title bar, icon rail, sidebar with sessions grouped by workspace, the home layout, primitives and theme switching. Connected to the real `session.list` through the preload API; everything else renders from the typed client with fakes in tests. No visual changes from the prototype.

**Blocked by:** 15

**Status:** done

**Files and components likely to change:**
- `gui/src/renderer/{main.tsx,App.tsx}`, `components/{ui,shell,home}/`, `state/store.ts` (UI slice only), `lib/cn.ts`
- `gui/tests/unit/renderer/*` (React Testing Library)

**Acceptance criteria:**
- [x] Components are carried over from `.scratch/desktop-gui/prototype/src/components` with the 40 Biome findings fixed (every `<button>` typed, semantic elements, labels with controls) and no change to layout, spacing, type or colours.
- [x] Theme: System, Dark and Light work for every ported component; the preference is applied before first paint through the preload API and persisted by main.
- [x] The sidebar lists sessions from the real `session.list`, grouped by workspace, with relative times, the running dot and the interrupted marker driven by state; the layout rule from ticket 07 holds at 1100 by 720 (sidebar hidden below 1280 px while Changes is open).
- [x] The interface never says "chat" or "thread" (checked on rendered text in component tests).
- [x] `make gui-lint` has zero Biome findings; `check-tokens` passes; Vitest covers the shell states.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e   # the hardening spec still passes with the real shell
```

**Documentation:** None.

**Notes:** Visual reference: `.scratch/desktop-gui/prototype/` (screens 1 and 3).

**Completion record:**
- Ported from the prototype with every accessibility finding fixed (every button typed, the rename field focuses through an effect instead of `autoFocus`, headings and labels with controls) and zero Biome findings: `ui/{badge,banana-mark,button,popover,tooltip}`, `shell/{TitleBar,IconRail,Sidebar}`, `home/{HomeScreen,Composer,WorkspacePicker,ModelPicker}`, an Appearance-only `settings/SettingsView` (the theme control), `lib/cn.ts` and the rest of the prototype's stylesheet. `dialog`, `checkbox` and `kbd` stay out until a ticket uses them. A real screenshot of the built app in dark and light was compared by eye with the prototype's home screen.
- Data: `lib/client.ts` is the typed client every component reads from context (`state/client-context.tsx`); tests give it a fake. `state/sessions.ts` loads the real `session.list` and `workspace.list` through the preload API, retries while the sidecar starts, reloads when it reports ready, derives the running dot from `active_turn` and turns running sessions into the interrupted marker when the sidecar crashes. `state/store.tsx` holds the UI slice only.
- Theme: `prefs.get` and `prefs.set` are new preload API keys and IPC channels; main keeps `preferences.json` in `userData` (private, written by rename, corrupt file means defaults). `main.tsx` waits for the saved theme and sets it before anything is drawn. `AppInfo` now carries the product name from the package metadata, so the renderer never spells it. The window uses `titleBarStyle: hiddenInset` as in the prototype.
- Layout rule: below 1280 px the sidebar hides while Changes is open (unit-tested with a mocked media query). Changes is the frame and empty state only (`ChangesPlaceholder`).
- Checked: `make gui-lint` (Biome with no findings, both tsc projects, `check-tokens`, `check-versions`); `make gui-test` (104 tests); `make gui-e2e` (10 passed: the hardening spec with the real shell, plus the sidebar listing a real session after a reload and a theme kept across a relaunch). Mutations caught: the narrow-window rule, the interrupted marking and the theme-save bookkeeping (the unit test found and the code fixes a bug where switching back to the starting theme was not saved).
- Deviations: (1) the composer is disabled with "Starting a session is not available yet." and Search sessions is disabled: both belong to later tickets. (2) A session click selects it and fills the title bar; the transcript view is a later ticket, so the main area stays on Home. (3) The Restricted chip, the key onboarding card and the approval dialogs are not ported; they need trust and key flows. (4) The traffic-light stand-in is dropped: macOS draws the real ones and the title bar reserves their space. (5) The suggestion chips fill the composer text. (6) `index.html` starts in `dark`, the default preference, to avoid a flash before the saved theme is applied; `system` is applied from the saved preference.
- Not covered: Linux; 1100 by 720 was checked through the layout rule's unit test, not a rendered screenshot.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
