# 17: Open a workspace, find the key, remember preferences

**Lane / milestone:** G · M5 First session   **Size:** M (one agent session)

**What to build:** The first vertical slice with real data: pick a folder, open it restricted-first, resolve the API key (environment, login shell, memory-only entry), show the key onboarding, and persist preferences. Workspaces always open restricted here; trust arrives in ticket 23.

**Blocked by:** 16, 10

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/main/{key/{resolver.ts,probe.ts},prefs/store.ts}`, `main/ipc/*` (`workspace.pick`, `workspace.open`, `workspace.close`, `key.*`, `prefs.*`)
- `gui/src/renderer/components/{home/WorkspacePicker.tsx,dialogs/KeyOnboarding.tsx}`
- `gui/tests/unit/main/*`, `gui/tests/e2e/credentials.spec.ts`

**Acceptance criteria:**
- [ ] Key resolution order: the app's environment, then the login-shell probe (5 s timeout, sentinel-delimited, also yields `PATH` and the proxy variables), then a memory-only entry that restarts the sidecar with the key; the value is never written to disk, preferences, trust data or logs, and the renderer receives only `{present, source}`.
- [ ] Probe tests use a stub `$SHELL`: rc-file noise around the sentinels parses; a hanging shell is killed at 5 s and falls through; an empty result is not an error.
- [ ] With no key anywhere the app opens in `needs_key`: past sessions and transcripts still work, opening a workspace and starting a turn are disabled, and the onboarding panel offers "Check again" and the session-only entry.
- [ ] `workspace.pick` is the only folder dialog; a recent workspace opens through the same flow; the workspace always opens restricted with no config loaded; preferences hold theme, last workspace, up to 10 recents and panel state in `userData/preferences.json` (atomic writes, unknown keys dropped).
- [ ] The key never appears in `userData`, any IPC payload, `window.banana` return values, the sidecar's argv or any log (e2e greps all of them for the test key).

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** Applies from the drafts: the `AGENTS.md` key sentence and the workspace-trust rule (until ticket 23 every workspace is restricted, so the rule already holds).

**Notes:** Specification: ticket 06 sections 2 and 6.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
