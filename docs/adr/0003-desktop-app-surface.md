# Desktop app as a third surface, over a Rust sidecar

Accepted. This partly supersedes [ADR 0001](0001-rust-local-first-fork.md): the TUI and the headless run are no longer the only surfaces. Everything else in ADR 0001 stands, and so does [ADR 0002](0002-in-turn-waiting.md).

ADR 0001 made the TUI and the headless run the only surfaces and dropped the hosted product's React UI packages with the rest of the multi-tenant stack. Banana Harness now also ships a **desktop app**, an Electron application for macOS, because a window with a session list, a live transcript, a diff view and approval dialogs is a better way to supervise an agent than a terminal. This does not bring the hosted product back. Nothing is served over a network, there is no tenant or shared state, and the app drives the same local engine as the other two surfaces.

## Decision

- **Three surfaces, one engine.** The TUI, the desktop app and the headless run all drive the in-process harness in `harness-core`. None of them reimplements agent logic, and the Rust core stays the source of truth for sessions, turns, events, required actions and checkpoints.
- **The desktop app is a thin client.** Electron's main process launches the shipped `banana-harness` binary as `banana-harness serve --stdio`, the **sidecar**, and talks to it over its standard input and output. The renderer never touches Node, the file system or a process.
- **The wire protocol is newline-delimited JSON** with `ready`, `request`, `response`, `event` and `fatal` frames, request ids, structured errors and an integer protocol version. stdout carries protocol frames only; logs go to stderr. It reuses the core's tagged `Event` type unchanged. There is no HTTP or WebSocket server and no native Node binding. The protocol lives in a new library crate, `harness-bridge`, which depends on `harness-core` only.
- **One harness per open workspace**, opened lazily and reused. A harness serves exactly one workspace; the session list is read straight from the shared database, so it works before any workspace is open. One turn runs per workspace at a time.
- **Durable undo marker.** Undo records a new durable `turn.undone` event, so an undone turn stays undone after a restart and cannot be undone twice. No schema migration is needed.
- **No Keychain in v1.** The API key is still read only from `AI_API_KEY`. The app finds it in its own environment, then in the user's login shell (which also yields the user's `PATH`), and otherwise accepts a key typed for the current session that is held in memory and never written anywhere.

## Consequences

**Security.**
- The renderer runs sandboxed with context isolation and no Node integration. It sees one frozen preload object with an allowlist of calls; folder selection, external links, workspace opening and process management stay in the main process. Every sidecar frame and every renderer call is validated against a schema before it is used. The renderer is served from a custom origin with a strict content security policy.
- **A workspace's own config is not trusted by default.** `.banana-harness/config.toml` can start programs (MCP servers), loosen guardrails and change where the API key is sent. The desktop app therefore opens every workspace in **restricted mode**, without that file, and loads it only after the user has trusted that exact file; trust is bound to the file's content hash and stored by the app, never in the workspace.
- **Provider destinations are checked.** After a trusted open, the app compares each provider's URL authority (host and port) and scheme with the baseline and blocks the workspace on any difference the user did not approve. A non-HTTPS endpoint is allowed only for loopback, which is what tests use.
- The key never appears in a request, response, event, log, preference file or test artifact; tests prove it.

**Testing.**
- The fake provider has one owner, a new dev-only crate, `harness-testkit`, which depends on no product crate and is never linked into the shipped binary. Tests reach a model only through `base_url` in layered config, with `XDG_*` paths and an empty environment for isolation. Production code carries no test hook.
- One set of scenario files drives the in-process protocol tests, a real `serve --stdio` process test and the Electron end-to-end tests, and all three must send the model the same conversation.

**Build and packaging.**
- `gui/` is a standalone TypeScript project (Node 24, a pinned pnpm, exact versions, a committed lockfile). Its toolchain is recorded in `gui/AGENTS.md`, not here, because it is easy to change.
- Version 1 ships an **unsigned macOS arm64** DMG and ZIP with the sidecar inside the app bundle. Windows, Linux packages, universal builds, signing, notarization and auto-update are out of scope.
- `make setup`, `make run`, `make test` and `make lint` keep working on a machine with only Rust; the desktop app's checks run in them only when its dependencies are installed, or when CI requires them.

**Exceptions to existing rules.**
- The product name may appear in the desktop app's package metadata (`gui/package.json` and `gui/electron-builder.yml`: product name, window title, app-data folder name, and the macOS bundle identifier `io.github.avgblank.banana-harness`) in addition to `harness-core`'s constants and the binary crate's package name. A packaging file has to state them before any code runs; the bundle identifier is hard to change once people have installed the app.

**Not decided here.** Signing and distribution outside the repository, other operating systems, several models per provider, whether undo should tell the agent that it happened (v1 shows a hint in the composer), and how MCP status and logins are presented.
