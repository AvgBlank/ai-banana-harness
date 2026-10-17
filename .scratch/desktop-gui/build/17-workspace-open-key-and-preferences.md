# 17: Open a workspace, find the key, remember preferences

**Lane / milestone:** G · M5 First session   **Size:** M (one agent session)

**What to build:** The first vertical slice with real data: pick a folder, open it restricted-first, resolve the API key (environment, login shell, memory-only entry), show the key onboarding, and persist preferences. Workspaces always open restricted here; trust arrives in ticket 23.

**Blocked by:** 16, 10

**Status:** done

**Files and components likely to change:**
- `gui/src/main/{key/{resolver.ts,probe.ts},prefs/store.ts}`, `main/ipc/*` (`workspace.pick`, `workspace.open`, `workspace.close`, `key.*`, `prefs.*`)
- `gui/src/renderer/components/{home/WorkspacePicker.tsx,dialogs/KeyOnboarding.tsx}`
- `gui/tests/unit/main/*`, `gui/tests/e2e/credentials.spec.ts`

**Acceptance criteria:**
- [x] Key resolution order: the app's environment, then the login-shell probe (5 s timeout, sentinel-delimited, also yields `PATH` and the proxy variables), then a memory-only entry that restarts the sidecar with the key; the value is never written to disk, preferences, trust data or logs, and the renderer receives only `{present, source}`.
- [x] Probe tests use a stub `$SHELL`: rc-file noise around the sentinels parses; a hanging shell is killed at 5 s and falls through; an empty result is not an error.
- [x] With no key anywhere the app opens in `needs_key`: past sessions and transcripts still work, opening a workspace and starting a turn are disabled, and the onboarding panel offers "Check again" and the session-only entry.
- [x] `workspace.pick` is the only folder dialog; a recent workspace opens through the same flow; the workspace always opens restricted with no config loaded; preferences hold theme, last workspace, up to 10 recents and panel state in `userData/preferences.json` (atomic writes, unknown keys dropped).
- [x] The key never appears in `userData`, any IPC payload, `window.banana` return values, the sidecar's argv or any log (e2e greps all of them for the test key).

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** Applies from the drafts: the `AGENTS.md` key sentence and the workspace-trust rule (until ticket 23 every workspace is restricted, so the rule already holds).

**Notes:** Specification: ticket 06 sections 2 and 6.

**Completion record:**
- Key (`gui/src/main/key/{probe,resolver}.ts`): the order is the app's environment, then `$SHELL -ilc` (own process group, no stdin, 5 s timeout and kill, 256 KiB cap, `printenv` between sentinels, which also yields `PATH` and the proxy variables and is also run when the app's `PATH` is launchd's), then a memory-only entry. `KeyResolver` keeps the value and builds the sidecar's environment; the renderer gets `{present, source}` only. `key.status`, `key.recheck` ("Check again", restarts the sidecar if it was waiting for a key) and `key.setForSession` (stores in memory, restarts the sidecar) are new preload keys and IPC channels, with `workspace.close`. Main resolves the key before starting the sidecar.
- Preferences (`main/prefs.ts`): theme, panel state (sidebar and changes), last workspace and up to ten recents in `userData/preferences.json` (private, atomic, unknown keys dropped on load and never stored). The renderer may change only theme and the panels; main alone records the last workspace and recents when it opens one. Known workspaces are seeded from them, so a recent workspace opens through the same restricted flow as a picked one; the last workspace is opened again at start when there is a key.
- UI: `KeyOnboarding` (ported with real actions and the app name from main) shows when there is no key; Open folder, the recents and the composer are off until there is one; past sessions still list.
- Checked: `make gui-lint`; `make gui-test` (142 tests: probe with stub shells including a hanging one and its background child, resolver order and environment, preferences, IPC, onboarding, recents, last workspace and panels); `make gui-e2e` (16 passed: the key from the environment, from a noisy stub login shell, a hanging shell skipped after about 5 s, no key then a typed one that restarts the sidecar and reaches the provider as a valid key, "Check again" finding a key later, preferences surviving a relaunch). In each key path the test searches `userData`, the main log, what the renderer received, the page and the sidecar's argv for the test key. Mutations caught: the shell's kill, the order of sources (after adding a test that exercises it), a reply that echoes the key, and a log line with the key.
- Deviations: (1) preferences stay in `main/prefs.ts`, not `prefs/store.ts`. (2) `workspace.close` exists in main and preload but the interface has no control for it yet. (3) Without a key the sidecar opens a workspace with `api_key_missing`; the interface prevents the attempt and the e2e test checks the refusal. (4) A typed key is also kept out of the page: the field is emptied before the call and the e2e test searches the page text.
- Not covered: Linux; fish or other shells (the stub shells are `sh`); the Dock launch case with a real launchd environment.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
