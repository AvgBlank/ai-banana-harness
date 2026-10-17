# Electron main/preload/renderer design

Type: grilling
Status: resolved
Blocked by:

## Question

Specify the main-process sidecar supervisor (spawn, handshake, restart policy, crash UI, shutdown), the preload API surface and its schema validation, IPC channel naming, folder-picker and external-link handling, CSP, window and menu behaviour, where the theme preference and last workspace are persisted, and how the API key reaches the sidecar (only via `AI_API_KEY`, never persisted or shown).

## Answer

Resolved 2026-10-17 against [the wire protocol](03-sidecar-placement-and-protocol.md), [the core additions](10-core-api-additions.md), `AGENTS.md`, `README.md`, `config/default.toml` and ADR 0001/0002. Nothing is implemented. Two choices are marked as questions for the user; each has a default that is used unless you say otherwise.

### Facts that shaped the design

- `AGENTS.md`: the key is read only from `AI_API_KEY`; keys MUST NOT appear in code, config, fixtures, docs or commit messages and MUST stay redacted from anything persisted, displayed or returned by a tool. Nothing in the repo touches Keychain.
- A Dock- or Finder-launched macOS app inherits launchd's environment, not the user's shell: no `AI_API_KEY`, and a minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`). The second half matters as much as the first: the agent's `bash` tool and the git checkpoint/diff code need the user's `PATH` (git from Homebrew or Xcode, `node`, `cargo`, MCP launchers like `npx`).
- The embedded default already puts `.banana-harness/skills` on the skills path and always appends the workspace's `AGENTS.md`, so a workspace influences the model's prompt even without its config layer. Those are prompt content, bounded by the permission engine; they are not gated by trust (see "Residual risks").
- A workspace config can: define `[mcp.*]` servers (a command run outside the `bash` guardrails; `env` values expand `{env:VAR}` from the process environment, so `{env:AI_API_KEY}` would hand the key to that command), rewrite `[permission]` rules (the bash denylist), set `[providers.*].base_url` (the next model call sends the key there), and change `agent.instructions`, `tools` and `skills.paths`.
- Config is read only when a `Harness` is opened, so an edit made later (including by the agent itself) takes effect at the next open, not mid-session.
- The protocol (ticket 03) has `ready.api_key_present`, one running turn per workspace, `workspace.open` with `trust_workspace_config` (default false), and no file/shell methods.

### 1. Sidecar lifecycle (Electron main)

**Module:** a `SidecarSupervisor` in main with an explicit state machine: `starting` then `ready`, or `needs_key`, `crashed` (restarting), `failed` (needs a user action), `stopping`. The renderer only ever sees this state and the typed RPC; it never touches the process.

**Launch.**
- Binary: `<resources>/bin/banana-harness` when packaged; `target/{debug,release}/banana-harness` in development. A path override (`BANANA_HARNESS_BIN`) is honoured only when `!app.isPackaged`, so tests can point at a build; it is an environment variable of the GUI, not a Rust hook.
- `spawn(bin, ["serve", "--stdio"], { stdio: ["pipe","pipe","pipe"] })` with an explicit environment, not a copy of `process.env`: `HOME`, `USER`, `LOGNAME`, `SHELL`, `TMPDIR`, `LANG`/`LC_*`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, the proxy variables (`HTTP(S)_PROXY`, `ALL_PROXY`, `NO_PROXY`, either case), `SSL_CERT_FILE`/`SSL_CERT_DIR`, plus the resolved `PATH` and `AI_API_KEY` (section 2). `ELECTRON_*`, `NODE_OPTIONS` and everything else are dropped. The key is only ever in this environment, never in argv.

**Handshake.** Within 10 s the first stdout line must be a valid `ready` with `protocol === 1`. Otherwise the child is killed and the state is `failed` with one of: `no_ready` (timeout), `incompatible_protocol` (different integer), `fatal` (the sidecar's own `fatal` frame, message shown), `exited_early` (with the last 4 KiB of stderr, key-redacted). These are deterministic, so they are not auto-restarted. `ready.api_key_present === false` moves to `needs_key` (section 2). A `fatal` or early exit is never retried automatically.

**Framing and malformed frames (sidecar to main).**
- stdout is split on `\n`; a line over 8 MiB is discarded to the next newline.
- Every line is `JSON.parse`d and validated with a zod schema keyed by `kind` (the schemas are checked against ticket 03's golden frame fixtures). An invalid line is dropped and counted; it is logged by *count and kind only*, never content.
- 20 consecutive invalid lines, or any invalid line before `ready`, is treated as a corrupted stream: kill, state `crashed`, counts toward the restart budget.
- A `response` with an unknown id, or a duplicate, is dropped. An `event` that fails validation is dropped and shown only in development as a non-fatal warning.
- Requests from main use main-generated ids (`m<n>`); the renderer cannot choose ids. A stdin write waits for `drain` (queued, in order).
- Timeouts: 30 s default; 60 s for `workspace.open`, `session.resume`, `session.undo`, `changes.*`. A timeout rejects the caller's promise with `timeout`; a late response for that id is dropped. `turn.start` returns at once, so it has no long timeout. ponytail: no hung-sidecar detection (the protocol has no ping); the user can press Restart.

**Crash detection and restart.**
- Crash = `exit`/`close`, stdin EPIPE, stdout `end`, or the corrupted-stream rule. Main then: rejects every pending request with `sidecar_exited`, clears all live-turn buffers (section 5), marks which workspaces had a running turn, and pushes state `crashed`.
- Automatic restart with 0.5 s, 1 s and 2 s delays, at most 3 restarts in a 60 s window. Beyond that: `failed` with a Restart button (`banana:sidecar:restart`). A successful `ready` that stays up for 60 s resets the budget.
- After a restart main re-opens the workspaces that were open, one at a time, using the normal restricted-first trust flow (section 3, so a trust decision is re-evaluated, not assumed), then pushes `ready` with the list of reopened workspaces. It never re-sends or retries anything. The interrupted turn appears on the next `session.resume` with `state: null` and the renderer labels it interrupted (ticket 03, ADR 0002).
- If the sidecar dies while a required action waited, that action is gone with the turn; the UI says so.

**Clean shutdown.**
- Cmd+Q / `before-quit`: if a turn is running, confirm ("A turn is running. Quit and cancel it? Files it already changed stay; you can undo from the session.") Then: send `shutdown` (4 s), close stdin, wait up to 5 s for exit, then `SIGTERM`, then `SIGKILL` after 2 s, then quit. A second Cmd+Q forces the kill path.
- Closing the window on macOS keeps the app and any running turn alive (standard behaviour); only quit stops the sidecar.
- If main itself dies, the sidecar sees stdin EOF and exits (ticket 03), so no orphan survives.
- Single instance (`requestSingleInstanceLock`) and single window in v1: a second launch focuses the first, so there are never two sidecars, trust stores or live buffers for one user.
- A renderer crash (`render-process-gone`) reloads the window; the sidecar is untouched (section 5 covers catching up).

**One running turn per workspace** stays the sidecar's rule (`turn_in_progress`). Main does not queue turns; it tracks the active turn only for the quit confirmation, the crash message and the live buffer.

### 2. API key delivery on macOS

**Decision (default; see Question 1):** the key keeps a single source of truth, `AI_API_KEY` in the user's own environment, exactly as for `make run` and `banana-harness run`. The GUI never stores it. It finds it in this order, stops at the first hit, and caches the result in main-process memory for the app session only:

1. `process.env.AI_API_KEY`: present when launched from a terminal (`open -a` from a shell, `make gui`, or dev) or by a launch agent that sets it.
2. A **login-shell probe** (only if step 1 is empty): run `$SHELL -ilc '<print PATH and AI_API_KEY between sentinels>'` with stdin from `/dev/null`, a 5 s hard timeout and output cap, using `printenv` and `printf` (shell-agnostic, works in zsh, bash and fish). The sentinels let the parser ignore rc-file noise. The probe also returns the user's `PATH` and the proxy variables, which fixes the minimal-`PATH` problem in the same run, and it is run even when the key came from step 1 if `PATH` looks like launchd's. Interactive (`-i`) is needed because most people export the key in `~/.zshrc`. A probe that times out, fails or prints nothing is treated as "no key from the shell", never as an error that blocks the app.
3. **Session-only entry**: if still missing, the UI shows an onboarding panel; the user can type the key into a masked field. It goes to main over one IPC call, is kept in memory, and is passed to the sidecar's environment on its next spawn (the key is read once at start, so entering it restarts the sidecar). It is never written to disk, `localStorage`, preferences, trust data or logs, and the field is cleared on submit. Next launch it must be entered again or found via steps 1–2.

The Rust side is unchanged and still reads only `AI_API_KEY` from its environment, so the `AGENTS.md` rule holds for the harness. Because the sidecar never sees the key any other way, there is no file or protocol field to leak it from.

**Never shown.** The renderer receives only `{ present: boolean, source: "env" | "login_shell" | "session" | "none" }`. Main keeps a small `redact(text)` that removes the exact key string from anything it logs or forwards (stderr, error messages); the sidecar already redacts events, so this is belt and braces.

**Failure behaviour.**
- No key anywhere: state `needs_key`; the app opens normally (sessions listing and transcript replay of past sessions work, since they need no key), but `workspace.open` and turn start are disabled with the onboarding panel explaining `export AI_API_KEY=...` in the shell profile, a "Check again" button (re-runs steps 1–2, ignoring the cache) and the session-only entry.
- Probe hangs (an rc file that prompts): 5 s timeout, kill the process group, fall through to step 3 with a hint that the shell profile is slow or interactive.
- Wrong key (the provider answers 401): arrives as `turn.done{error}`; the UI offers a hint (a key belongs to one provider) and a switch of provider. The key is never echoed.
- `workspace.open` returns `api_key_missing` (the key vanished): same panel.

**Trade-offs.**
- Login-shell probe: runs the user's rc files (a small start-up cost and a theoretical side effect), but uses the one place the user already keeps the key and stores nothing new; failure degrades to the session-only entry.
- Session-only entry: annoying on every launch for people who don't export the key, but nothing is persisted and the rule is untouched.
- Passing the key by environment means a same-user process can read it (`ps eww`); `make run` has the same property, so this is no new exposure.
- Rejected: Keychain/`safeStorage` (a new at-rest copy of the secret and an `AGENTS.md` conflict, see Question 1), a settings field (persists), the workspace `.env` (the engine denies `.env*` reads, and it is workspace-controlled), asking the sidecar to read a file (breaks the single-source rule).

### 3. Workspace trust

**What trust gates.** Trust decides one thing: whether `.banana-harness/config.toml` is loaded into the `Harness` (`trust_workspace_config`). The user-level config is the user's own and always loads. Without trust a workspace runs in **restricted mode**: embedded defaults plus the user layer. That removes, in one switch: workspace MCP servers (commands and `{env:...}` leaks), rewritten permission rules, `base_url` overrides, and instruction/tool/skill-path overrides. The GUI writes none of the workspace's config.

**Flow (always restricted first, then upgrade).**
1. The user picks or selects a workspace. Main sends `workspace.open { path, trust_workspace_config: false }` (a restricted `Harness`; cheap, no model call).
2. If `workspace_config.present` is false: done, nothing to trust.
3. Otherwise main reads that one file (the `path` the sidecar returned from core's `workspace_config_path`, so the GUI never hard-codes the layout; refuse symlinks and files over 256 KiB), computes its SHA-256, and looks in the trust store for the workspace's canonical path.
   - Entry exists and the hash matches: main closes and re-opens with `trust_workspace_config: true`, then runs the host check below.
   - No entry, or the hash changed: the UI shows the trust dialog. The default button is **Open in restricted mode**; the other is **Trust and open**. The workspace header shows a "Restricted" chip with "Review and trust…" until trust is granted.
4. Upgrading to trusted is refused while a turn is running in that workspace.

**Trust dialog.** Main parses the TOML (a pinned parser, e.g. `smol-toml`) to build an advisory summary; the raw file is always available in a read-only view, and a parse failure shows only the raw text and a warning that the config will fail to load. The summary lists, with warning styling: every MCP server with its full command and env variable *names* (a red flag if any value contains `{env:`, and a specific red flag for `{env:AI_API_KEY}`), every permission rule it sets (red for any `allow` on `bash`/`external_directory`), every `[providers.*]` override with the destination host (red unless the host equals the baseline host, below), and other overrides (`agent.instructions`, `tools`, `skills.paths`). A footnote states that the folder's `AGENTS.md` and skills still guide the agent in restricted mode. If a `base_url` host differs from the baseline, **Trust and open** requires an extra checkbox naming that host ("My API key will be sent to <host>").

**Host check (independent of main's TOML parser).** `provider.list` returns each provider's `host` (the host only, no path, query or credentials; a ticket 10/03 amendment below). The baseline is read from the restricted open (embedded defaults plus user layer). After every trusted open, main compares each provider's host with baseline plus the hosts the user explicitly approved for that trust entry. A mismatch closes the workspace immediately and shows "Blocked: this folder's config changes where your API key is sent." The key is only sent on a model call, never at open, so blocking at open prevents the exfiltration even if the summary mis-parsed. Hosts are also shown in the model picker permanently, so the user always sees where the key goes.

**Storage.** A separate file `trust.json` in Electron's `userData` (mode 0600, atomic write): `{ "version": 1, "workspaces": { "<canonical path>": { "config_sha256": "...", "approved_hosts": [], "trusted_at": "..." } } }`. It is never read from or written into a workspace, and the sidecar and database never see it. A corrupt file is moved aside and treated as empty (nothing trusted). Settings lists entries with "Forget". Trust is bound to the file's content, so a changed config (including one the agent edited) prompts again at the next open and cannot silently escalate across sessions. ponytail: the hash is computed by main just before the open; a local attacker swapping the file in that window is outside the threat model.

**Unchanged elsewhere.** The TUI and the headless run keep their own behaviour; this gate is a GUI policy.

### 4. Electron security

**BrowserWindow.** `contextIsolation: true`, `sandbox: true`, `nodeIntegration: false`, `nodeIntegrationInWorker/SubFrames: false`, `webSecurity: true`, `allowRunningInsecureContent: false`, `webviewTag: false`, `enableBlinkFeatures` unset, `devTools: !app.isPackaged`. `titleBarStyle: 'hiddenInset'` (the Codex look, final pending ticket 04). Minimal native menu (app, edit, view, window); no remote content.

**Origin.** The renderer is served from a privileged custom scheme `app://banana/` (secure, `supportFetchAPI`, no CORS bypass) that maps only into the packaged `renderer/` folder with path normalisation (no `..`). In development only (`!app.isPackaged`) the Vite dev server on `http://localhost:<port>` is also allowed.

**Content security policy** (response header on the `app` scheme, plus a `<meta>` fallback): `default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; worker-src 'self' blob:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`. The renderer makes no network requests. `'unsafe-inline'` is for styles only (Tailwind and Monaco inject them); no inline or eval scripts. In development, `connect-src` and `script-src` add the dev server and its websocket. The Monaco worker setup (`blob:` workers) is unverified under this policy (ticket 02 noted it); a Playwright smoke test must render a Monaco diff under the final CSP.

**Navigation and windows.** `will-navigate` and `will-redirect` cancel anything not on `app://banana/` (or the dev origin). `setWindowOpenHandler` returns `{ action: 'deny' }` for everything; `https:` links are routed to the external-link path below. `session.setPermissionRequestHandler` and `setPermissionCheckHandler` deny every permission (camera, mic, geolocation, notifications). Dropped files do not navigate (`will-navigate` cancels the `file:` navigation). No `<webview>`, no `<iframe>` to remote origins.

**External links.** Model output is untrusted, and so are MCP-supplied OAuth URLs. The renderer calls `banana.external.open(url)`; main parses the URL and allows only `https:` (and `http:` for `localhost`/`127.0.0.1` OAuth redirects), rejects userinfo and over-long URLs, shows a confirm dialog with the full URL and host (with "don't ask again for this host this session"), and only then calls `shell.openExternal`. `file:`, `javascript:`, `data:`, custom schemes and anything else are rejected. Local files are never opened by this path.

**Folder selection.** Only main opens the system dialog: `banana:workspace:pick` calls `dialog.showOpenDialog({ properties: ['openDirectory'] })` and returns the chosen path. `banana:workspace:open` accepts only a path that main knows: one returned by the picker this run, in the recent list, or the `workspace` of a session from `session.list`. Any other path is `unknown_workspace`. The renderer never supplies `trust_workspace_config`; main sets it.

**Preload and IPC.** The preload (a single bundled script; the sandbox allows no `require` of local modules) exposes one frozen object via `contextBridge`, `window.banana`, and nothing else: no `ipcRenderer`, no Node globals, no event objects.

| API | IPC channel (invoke) | notes |
|---|---|---|
| `app.info()` | `banana:app:info` | version, platform, protocol |
| `prefs.get()` / `prefs.set(patch)` | `banana:prefs:get` / `banana:prefs:set` | zod-validated patch; unknown keys dropped |
| `workspace.pick()` | `banana:workspace:pick` | system folder dialog |
| `workspace.open(path)` | `banana:workspace:open` | runs the trust flow; returns `{ workspace, restricted, trust? }` |
| `workspace.trust(path, decision)` | `banana:workspace:trust` | stores the decision, then re-opens |
| `workspace.close(path)` | `banana:workspace:close` | |
| `rpc.call(method, params)` | `banana:rpc:call` | allowlisted methods only (below) |
| `live.attach(workspace, sessionId)` | `banana:live:attach` | returns buffered frames, then streams |
| `key.status()` / `key.setForSession(key)` / `key.recheck()` | `banana:key:*` | status only; the key is write-only |
| `external.open(url)` | `banana:external:open` | validated and confirmed |
| `sidecar.restart()` | `banana:sidecar:restart` | |

Events main to renderer (one channel each, subscribed through `window.banana.on(name, cb)` for a fixed set of names, returning an unsubscribe function): `banana:sidecar:state`, `banana:sidecar:event` (a validated `event` frame with its workspace, session and turn ids), `banana:menu:command`.

- **`rpc.call` allowlist:** `session.list`, `session.create`, `session.resume`, `session.rename`, `session.switch_provider`, `session.undo`, `provider.list`, `turn.start`, `turn.cancel`, `action.reply`, `changes.workspace`, `changes.turn`, `workspace.list`. Not callable from the renderer: `workspace.open` and `workspace.close` (the trust flow owns them), `shutdown`, anything unknown.
- **Validation:** every invoke handler (1) checks `event.senderFrame` is the main window's top frame on the allowed origin (any other sender is rejected and logged), (2) validates the arguments with a strict zod schema (unknown keys rejected, string lengths capped, `input` capped at 1 MiB), (3) for `rpc.call` re-validates the params against the per-method schema before forwarding, and (4) validates the sidecar's result against the response schema before returning it. Errors cross the boundary as `{ code, message }` with the key redacted, never a stack trace.
- The renderer has no filesystem, process, shell or network API of its own. File content reaches it only as `FileChange.patch` text and tool-call events; there is no method to read a file.

### 5. State ownership: who rebuilds the live view

There are three layers and exactly one owner for each, so nothing competes:

- **Durable truth:** Rust (SQLite), read through `session.resume`, `session.list` and `workspace.list`. Nothing in the GUI caches it as authoritative.
- **Ephemeral live tail:** Electron main holds, per workspace with a running turn, a bounded buffer (about 5 MiB, oldest dropped) of every `event` frame since that turn's `turn.created`, discarded on `turn.done`, on `workspace.close` and on any sidecar crash. It is a cache of frames already sent, not state.
- **UI state:** the renderer, derived purely by reducing events (ticket 07 designs the reducer). It owns nothing durable.

**Reload and reconnection protocol.** After a renderer (re)load the renderer calls `workspace.list` for open workspaces and active turns, then for the visible session `session.resume` (persisted turns), then `live.attach(workspace, sessionId)`. `attach` returns, in one step, the buffered frames and from that point forwards new frames to the window; because main is single-threaded, the snapshot and the subscription happen with no gap and no duplicate window. The renderer applies persisted events, then buffered frames, **deduplicating by `event.id`** (every `Event`, deltas included, carries a unique ULID, and replay and live use the same type). `workspace.list`'s `pending_action_ids` is used to show which required actions are still open. After a sidecar crash the buffer is empty, the transcript shows the turn as interrupted (`state: null`), and nothing is replayed. Reloading the renderer never touches the sidecar or the turn.

**Hand-offs to ticket 07:** the reducer must be idempotent by `event.id`, treat `state: null` as interrupted, and treat a required action as pending only when it appears in `pending_action_ids` or in a live frame after the last `turn.created`.

### 6. Persistence

| What | Where | Notes |
|---|---|---|
| Theme (`system` / `dark` / `light`), sidebar and Changes-panel state, last workspace, up to 10 recent workspaces | `userData/preferences.json`, written by main, atomic, validated, unknown keys dropped | ordinary preferences; paths only; no secrets |
| Trust decisions | `userData/trust.json` (0600) | section 3; separate file so a bad preference write can never touch trust |
| Last provider | the core database (`LAST_PROVIDER_SETTING`) | core already owns it (ticket 10); the GUI does not duplicate it |
| API key | nowhere on disk | memory only, section 2 |
| Workspace config | the workspace; only its hash is stored by the GUI | never copied or edited by the GUI |
| Sessions, transcripts, checkpoints | the core database | unchanged |

- Theme without a flash: main sets `nativeTheme.themeSource` from the preference at start and the window's `backgroundColor` for the resolved theme, so CSS `prefers-color-scheme` already matches when the renderer paints; an explicit choice also sets a `data-theme` attribute after `prefs.get`.
- `userData` is Electron's per-user app-data folder; tests redirect it with Chromium's `--user-data-dir`.
- The renderer keeps no durable state in `localStorage` or IndexedDB (nothing there is read back as truth).

### 7. Tests and acceptance criteria

Layers follow ticket 00: Vitest for main-process modules (dependency-injected: the supervisor takes a spawn function, the key resolver takes a shell path, the trust store takes a directory), and Playwright Electron tests against the real sidecar and a stub provider. Rust-side protocol tests are ticket 03's. Final folder layout is ticket 08's; `gui/tests/` is assumed here.

**Lifecycle (supervisor unit tests with a scripted fake child, plus Playwright with the real binary):**
- Handshake: `ready` within 10 s moves to `ready`; no `ready` gives `no_ready`; a different `protocol` gives `incompatible_protocol` (no auto-restart); a `fatal` frame gives `fatal` with its message; an early exit gives `exited_early` carrying redacted stderr.
- Framing: an oversized line, a non-UTF-8 line and non-JSON are dropped and counted without ending the connection; 20 consecutive invalid lines trigger kill and restart; an unknown response id is ignored; a response after a timeout is dropped.
- Crash: killing the real sidecar mid-turn rejects pending requests with `sidecar_exited`, pushes `crashed`, restarts after the backoff, re-opens the workspaces, and the interrupted turn appears with `state: null` after `session.resume`; the interrupted turn is never re-sent.
- Restart budget: a fake child that always exits fails after 3 restarts in 60 s and shows `failed` with a working Restart button; a child that stays up 60 s resets the budget.
- Shutdown: Cmd+Q with a running turn asks for confirmation; confirming sends `shutdown` and the process exits; a child that ignores `shutdown` and stdin EOF is terminated by SIGTERM then SIGKILL within the limits; killing main leaves no orphan sidecar.
- A second launch focuses the first window and starts no second sidecar. One running turn per workspace: a second `turn.start` surfaces `turn_in_progress` inline and sends nothing else.

**Credentials:**
- Order: env wins; with env empty the login-shell probe supplies key and `PATH` (the test points `$SHELL` at a stub script); with both empty the state is `needs_key`.
- A stub shell that prints rc noise around the sentinels parses correctly; one that hangs is killed at 5 s and falls through; one that prints nothing is not an error.
- In `needs_key`, sessions list and past transcripts still work, and `workspace.open` and turn start are blocked with the onboarding panel; "Check again" picks up a key that appeared; session-only entry restarts the sidecar with the key and a turn then succeeds against the stub provider.
- The key never appears in: any file under `userData` (scanned after the run), any renderer-visible IPC payload, `window.banana` return values, the sidecar's argv, or any log (the test uses a distinctive key string and greps all of them).

**Workspace trust and malicious configuration (fixtures under the test workspace's `.banana-harness/config.toml`):**
- MCP command: a config with `[mcp.x] preload = true`, `command = ["/bin/sh","-c","touch <marker>"]`. In restricted mode, after a full turn, the marker does not exist. After trusting, the dialog listed the command first.
- Key leak through env: an MCP `env` value `{env:AI_API_KEY}` is flagged red in the dialog; in restricted mode the server never starts.
- `base_url`: a config pointing `providers.deepseek.base_url` at a loopback stub that records requests. Restricted mode: the stub receives nothing and the model call goes to the baseline stub. Trusted without approving the host: the post-open host check closes the workspace and the stub still received no request. Trusted with the host approved: the host appears in the model picker and the stored entry lists it.
- Permissions: a config that removes the `sudo *` denylist entry is shown in the dialog; in restricted mode a `sudo` bash call is still denied.
- Hash binding: editing the trusted config (a byte change, and a change made by the agent during a turn) causes the dialog again at the next open and does not change the running session; `trust.json` corruption means nothing is trusted and the file is moved aside; a symlinked or oversized config is refused for hashing and the workspace opens restricted.
- Upgrading to trusted is refused while a turn runs. Restricted mode shows its chip and "Review and trust…" until granted. The dialog's raw view shows the file text and the summary never executes it.
- The TUI and headless behaviour are untouched (existing tests still pass).

**IPC and Electron hardening (Playwright against the packaged-style build, plus unit tests of the validators):**
- In the renderer, `typeof require`, `process`, `Buffer` and `__dirname` are `undefined`; `window.banana` is frozen and its keys equal the table above; no `ipcRenderer` is reachable.
- `rpc.call` rejects `shutdown`, `workspace.open`, `workspace.close` and an unknown method with `method_not_allowed`; rejects params with extra keys (including a smuggled `trust_workspace_config`) and over-long strings; `workspace.open` with a path main does not know is `unknown_workspace`.
- The sender check rejects an invoke from another frame or origin (unit-tested with synthetic `senderFrame` values) and logs it without payload.
- Sidecar responses and events that fail the zod schema never reach the renderer.
- CSP: injecting an inline `<script>` or `eval` in the page does nothing and a `securitypolicyviolation` fires; `fetch('https://example.com')` is blocked; a Monaco diff renders under the final CSP.
- Navigation: `location = 'https://example.com'`, `window.open(...)` and a dropped file leave the window on the app origin. Permission requests are denied.
- External links: `https://host/...` shows the confirm dialog with the full URL; `file:`, `javascript:`, `data:`, `http://` non-local, and URLs with userinfo are rejected without opening anything.
- Reload: after reloading the renderer mid-turn, the transcript, any streamed partial message and a pending approval are restored without duplicates (event ids unique in the rendered transcript) and the turn continues.

### Amendments to earlier tickets

- **Ticket 03 (wire protocol):** `workspace.open`'s `workspace_config` gains the file's absolute `path` (from core's `workspace_config_path`), and `provider.list`'s `ProviderOption` gains `host`.
- **Ticket 10 (core additions):** `ProviderOption` is `{ provider, model, host }`, where `host` is the host of the effective `base_url` (no path, query or credentials). Add a test that a workspace `base_url` override is reflected in `host`, and that credentials in a URL are not.
- Both are additive; ticket 03's protocol version stays 1 (the build run has not started).

### Residual risks (stated, not solved here)

- `AGENTS.md` and workspace skills still reach the model in restricted mode (the embedded default includes `.banana-harness/skills`), so a hostile folder can still prompt-inject. The permission engine and the bash denylist bound what the model can do; the default policy is autonomous apart from that denylist.
- `{env:VAR}` expansion in an MCP `env` hands any process-environment variable, including `AI_API_KEY`, to the server command. Restricted mode and the dialog contain this for workspace config, but the expansion is a core behaviour worth hardening (for example never expanding `AI_API_KEY`); it is not decided here.
- Same-user processes can read the sidecar's environment. This is the same as `make run`.

### Questions that need your decision

1. **Keychain "remember my key"?** I chose not to persist the key in v1 (login-shell probe, then a session-only entry), because `AGENTS.md` says the key is read only from `AI_API_KEY` and a stored copy would be a new at-rest secret. If you want a "Remember in macOS Keychain" option, ADR 0003 would have to amend that rule, and the key would still reach Rust only through the sidecar's environment. Default: no.
2. **Product name outside core.** The app's window title, `userData` folder name, `productName` and bundle id all need the name before the sidecar can supply it, and `AGENTS.md` allows the name only in `APP_NAME`/`APP_SLUG` and the binary's package name. Ticket 08/09 must either record the GUI package metadata as the one allowed extra copy in ADR 0003, or generate it from `APP_NAME` at build time. Default: allow the GUI package metadata as an ADR-recorded exception.

### Effects on other tickets

- 04: needs designs for the onboarding panel, trust dialog, restricted chip, crash/restart banner, quit confirmation and external-link confirmation, in both themes.
- 05: the Playwright and process tests need the stub to serve more than one base URL and to record requests (for the `base_url` and key-leak tests).
- 07: reducer rules in section 5.
- 08: bundle the sidecar at `resources/bin/banana-harness`, a `gui/tests/` layout, `--user-data-dir` for tests, and the product-name question above.
- 09: ADR 0003 records the trust gate, the key rule and the single instance, plus the name exception if approved.

### Decisions from the user (2026-10-17, during ticket 04)

1. **Keychain:** no. The API key stays environment-based or memory-only in v1; no "Remember in Keychain" option.
2. **Product name outside core:** allowed. The GUI package metadata may define the window title, app-data folder and bundle ID, and ADR 0003 documents that as the one exception to the `APP_NAME`/`APP_SLUG` rule (ticket 09 writes it).

### Amendment (from ticket 07, 2026-10-17)

`live.attach` returns `{ frames, active_turn_id }`. Main retains each workspace's most recent turn buffer until the next `turn.start`, `workspace.close` or a sidecar crash (not only until `turn.done`), so an attach that lands just after a turn ends still receives the tail including `turn.done`. The renderer attaches to every workspace's active turn at start. See [the renderer ticket](07-renderer-state-and-changes-panel.md#4-reconnection-and-reload).

### Amendment (from ticket 05, 2026-10-17)

The post-open provider check compares the URL authority (host and explicit port) and the scheme from `provider.list`, not the bare host, and blocks any provider whose scheme is not `https` unless its host is loopback, whatever the user approved. The credential, trust and malicious-config tests run on the `harness-testkit` `TestEnv` (the `evil` stub, `{{marker}}`, `without_key()`, `shell()`); see [ticket 05](05-fake-provider-for-smoke-tests.md#d-isolation-recipe-testenv-proposed-every-ingredient-except-the-last-two-is-verified-in-a3).

### Amendment (from ticket 08, 2026-10-17)

Packaged builds never carry Chromium's `--no-sandbox`; the e2e launch adds it only under Linux CI (**Assumed** to be needed on Actions runners). `make gui-package-smoke` launches the packaged `.app` through Playwright `_electron` (verified feasible) and checks the window and the sidecar `ready` frame. `BANANA_HARNESS_BIN` stays honoured only when the app is not packaged. The packaged sidecar path is `Contents/Resources/bin/banana-harness` (verified layout).
