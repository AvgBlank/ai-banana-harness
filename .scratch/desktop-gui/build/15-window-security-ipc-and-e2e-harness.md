# 15: Window hardening, preload, IPC validation and the e2e harness

**Lane / milestone:** G · M4 GUI foundation   **Size:** L (one agent session)

**What to build:** Ticket 06 section 4 made real, plus the Playwright harness every later behaviour is tested with and the `make gui-e2e` target. Split point if over budget: window, origin and preload (15a) then IPC validation and the e2e harness (15b).

**Blocked by:** 14, 08

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/main/{window.ts,origin.ts,ipc/{register.ts,validate.ts,sender.ts},external/links.ts}`, `gui/src/preload/index.ts`
- `gui/tests/unit/main/*`
- `gui/tests/e2e/{fixtures.ts,hardening.spec.ts}`, `gui/playwright.config.ts`
- `Makefile` (`gui-e2e`)

**Acceptance criteria:**
- [ ] The window has context isolation and the sandbox on, Node integration off, dev tools only when unpackaged, a custom `app://banana/` origin with path normalisation and the strict content security policy, navigation and `window.open` denied, every permission denied.
- [ ] `window.banana` is one frozen object whose keys equal the allowlist; `rpc.call` allows only the listed methods and rejects `shutdown`, `workspace.open`, `workspace.close` and unknown methods with `method_not_allowed`; params are strict (extra keys, over-long strings and a smuggled `trust_workspace_config` rejected); every handler checks the sender frame and origin.
- [ ] External links: only `https:` (and loopback `http:`), no userinfo, confirmed with the full URL; `file:`, `javascript:`, `data:` and other schemes are rejected.
- [ ] Folder selection happens only in main; a path main does not know is `unknown_workspace`.
- [ ] e2e hardening against the real sidecar: `typeof require`, `process` and `Buffer` are `undefined`; an injected inline script or `eval` does nothing and a policy violation fires; `fetch` to an outside origin is blocked; `location` assignment, `window.open` and a dropped file stay on the app origin; sidecar frames that fail the schema never reach the renderer.
- [ ] The e2e harness starts `fake-provider serve` per worker, builds the isolated environment, launches Electron with `_electron.launch({ args, env })` (the environment replaces the process environment), saves failure artifacts under `gui/test-results/`, and adds `--no-sandbox` only when `CI` is set on Linux.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** Applies from the drafts: the `AGENTS.md` `gui-e2e` clause and seam-4 wording; the `CONTRIBUTING.md` test sentence and pull-request bullet; the `README.md` `make gui-e2e` line.

**Notes:** Ticket 05 section E defines the lifecycle and artifacts.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
