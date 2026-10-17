# 15: Window hardening, preload, IPC validation and the e2e harness

**Lane / milestone:** G · M4 GUI foundation   **Size:** L (one agent session)

**What to build:** Ticket 06 section 4 made real, plus the Playwright harness every later behaviour is tested with and the `make gui-e2e` target. Split point if over budget: window, origin and preload (15a) then IPC validation and the e2e harness (15b).

**Blocked by:** 14, 08

**Status:** done

**Files and components likely to change:**
- `gui/src/main/{window.ts,origin.ts,ipc/{register.ts,validate.ts,sender.ts},external/links.ts}`, `gui/src/preload/index.ts`
- `gui/tests/unit/main/*`
- `gui/tests/e2e/{fixtures.ts,hardening.spec.ts}`, `gui/playwright.config.ts`
- `Makefile` (`gui-e2e`)

**Acceptance criteria:**
- [x] The window has context isolation and the sandbox on, Node integration off, dev tools only when unpackaged, a custom `app://banana/` origin with path normalisation and the strict content security policy, navigation and `window.open` denied, every permission denied.
- [x] `window.banana` is one frozen object whose keys equal the allowlist; `rpc.call` allows only the listed methods and rejects `shutdown`, `workspace.open`, `workspace.close` and unknown methods with `method_not_allowed`; params are strict (extra keys, over-long strings and a smuggled `trust_workspace_config` rejected); every handler checks the sender frame and origin.
- [x] External links: only `https:` (and loopback `http:`), no userinfo, confirmed with the full URL; `file:`, `javascript:`, `data:` and other schemes are rejected.
- [x] Folder selection happens only in main; a path main does not know is `unknown_workspace`.
- [x] e2e hardening against the real sidecar: `typeof require`, `process` and `Buffer` are `undefined`; an injected inline script or `eval` does nothing and a policy violation fires; `fetch` to an outside origin is blocked; `location` assignment, `window.open` and a dropped file stay on the app origin; sidecar frames that fail the schema never reach the renderer.
- [x] The e2e harness starts `fake-provider serve` per worker, builds the isolated environment, launches Electron with `_electron.launch({ args, env })` (the environment replaces the process environment), saves failure artifacts under `gui/test-results/`, and adds `--no-sandbox` only when `CI` is set on Linux.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** Applies from the drafts: the `AGENTS.md` `gui-e2e` clause and seam-4 wording; the `CONTRIBUTING.md` test sentence and pull-request bullet; the `README.md` `make gui-e2e` line.

**Notes:** Ticket 05 section E defines the lifecycle and artifacts.

**Completion record:**
- Main: `origin.ts` (privileged `app://banana/` scheme, path normalisation that refuses `..`, encoded traversal, backslashes, NUL and symlinks out of the root, the strict content security policy on every response, a looser dev-server policy), `window.ts` (isolation, sandbox, Node integration off, `devTools: !app.isPackaged`, navigation, redirect and `window.open` denied, every permission denied), `ipc/{sender,validate,register,known}.ts` (every handler checks the top frame and origin, validates strict arguments and returns `{ code, message }`), `external/links.ts` (`https:` and loopback `http:`, no credentials, a confirm dialog with the full URL and a per-host session approval). `preload/index.ts` exposes one deep-frozen `window.banana`; its keys are `bananaKeys` in `shared/api.ts`.
- Implemented now: `app.info`, `workspace.pick` and `open` (a folder only main knows; always restricted), `rpc.call`, `external.open`, `sidecar.restart`, and the `sidecarState` and `sidecarEvent` pushes. Preferences, live attach, key entry, workspace trust and close, and the menu command channel are not here; their channels and API keys arrive with the tickets that implement them.
- E2E: `playwright.config.ts`, `tests/e2e/fixtures.ts` (a fake provider per worker, `_electron.launch` with the isolated environment replacing the process environment, `--no-sandbox` only for `CI` on Linux, main-process log and screenshot attached on failure under `gui/test-results/`), `support/noisy-sidecar.mjs` and `hardening.spec.ts` (8 tests, all against the real sidecar). `make gui-e2e` builds the debug sidecar and `fake-provider`, builds the app, and runs them; the Makefile now follows `CARGO_TARGET_DIR` for the binary paths.
- Checked: `make gui-lint`; `make gui-test` (63 unit tests); `make gui-e2e` (8 passed). Mutations caught by the e2e suite: no CSP header, no navigation guard, permissions granted, `shutdown` added to the allowlist. The frame-schema test runs the real sidecar through a wrapper that adds an invalid event after every real one and requires none to reach the renderer.
- Deviations: (1) no `<meta>` CSP fallback: it would also apply in development and block the dev server's inline preamble. (2) `page.evaluate` runs through the DevTools protocol, which is exempt from the eval check, so the test proves inline script and string timers are blocked and reads the policy header instead of calling `eval`. (3) A file drop is exercised by emitting `will-navigate` for a `file:` URL on the window's contents; the OS drop itself cannot be driven. (4) Dev tools are shown to open when unpackaged; the packaged case is not run because no packaged app exists yet. (5) `render-process-gone` reload and the menu are not part of this ticket.
- Not covered: Linux and xvfb; a subframe sender (covered by unit tests only).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
