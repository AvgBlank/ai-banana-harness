# Sidecar crate placement and wire protocol

Type: grilling
Status: resolved
Blocked by:

## Question

Where does the sidecar live (a `serve --stdio` subcommand in `banana-harness`, or a new crate depending on core), given the crate-boundary rules and the single-owner rule? Where do the serde wire types live and who converts them from core types? Define the NDJSON envelope (request/reply/event/error), method list (workspace open, session list/create/resume/rename, start turn, cancel, reply to required action, undo, providers/models, changes), id correlation, event routing by workspace and session, backpressure, shutdown and crash semantics, and protocol versioning.

## Answer

Resolved 2026-10-17 against the code. Builds on [Core API additions](10-core-api-additions.md) without contradicting it (two small amendments are listed under "Core changes this design needs"). Nothing is implemented.

### Facts that shaped the design

- `Harness` is fixed to one workspace; `TurnHandle` gives an ordered `mpsc` stream plus `cancel()` and `reply()`; the stream always ends with `turn.done`. The first event, `turn.created`, carries the turn id; a turn can fail before it is emitted.
- Events are already tagged serde (`{"id", "type", ...}`) and already redacted of the API key before they are forwarded or stored.
- `banana-harness run --json` already prints one `Event` per line; the sidecar reuses that shape inside its own frames.
- An unfinished turn stays `state = 'running'` in SQLite (crash or kill); `turn.done` is the only thing that finishes it.
- `headless: true` auto-answers required actions; the sidecar always opens harnesses with `headless: false`.
- Core already writes warnings with `eprintln!`; MCP stdio servers get piped stdin/stdout from rmcp and `bash` runs with stdin null and captured output, so nothing in the tree writes to the process's stdout today.
- Layered config (embedded, user, workspace) is built inline in `crates/banana-harness/src/main.rs`; the TUI uses `Config::embedded_default()` only. `base_url`, MCP servers and permission rules can all come from the workspace layer.
- `etcetera` uses XDG-style paths on macOS too, so `XDG_CONFIG_HOME` and `XDG_DATA_HOME` redirect the user config and the database without any code change.

### 1. Placement

- **New library crate `harness-bridge`**, depending on `harness-core` only (the same position as `harness-tui`). It owns the protocol types, the dispatcher and the server loop. No other new dependencies beyond workspace-pinned `tokio`, `serde`, `serde_json`, `anyhow`, `thiserror`, `ulid`, `libc`.
- **Entry point is a subcommand of the existing binary: `banana-harness serve --stdio`.** Electron ships and spawns one binary, the one `make setup` already builds. `--stdio` is required, so other transports can't be added by accident.
- `banana-harness` depends on `harness-core`, `harness-tui` and `harness-bridge`. `AGENTS.md`'s crate-boundary line is updated in the same change (`harness-bridge` depends on core; the binary on core, tui and bridge). `harness-bridge` MUST NOT depend on the TUI or on any leaf crate directly.
- The crate name carries no product name; the app name in the `ready` frame comes from `APP_NAME`.
- `harness-bridge` denies `clippy::print_stdout` and `print_stderr` outside one `log` helper that writes to stderr, so a stray `println!` fails `make lint`.
- Defence in depth (unix): `serve` moves the real stdout to a private fd and points fd 1 at stderr before anything else runs, so even a dependency that writes to stdout cannot corrupt the stream. ponytail: unix only; Windows is out of scope.
- Because `bridge::serve` takes its reader, writer and options as parameters (below), the same code runs in-process in tests and as the subcommand.

```rust
pub struct BridgeOptions {
    pub db_path: PathBuf,             // production: default_db_path()
    pub user_config: Option<PathBuf>, // production: user_config_path()
    pub api_key: String,              // production: api_key_from_env()
}
pub async fn serve<R: AsyncRead + Unpin, W: AsyncWrite + Send + Unpin + 'static>(
    reader: R, writer: W, options: BridgeOptions,
) -> anyhow::Result<()>;
```

### 2. Wire format (protocol version 1)

NDJSON over stdio: UTF-8, one JSON object per line, `\n`-terminated, no embedded newlines. Client writes the sidecar's stdin; the sidecar writes only frames to stdout and logs to stderr. Maximum accepted line: 8 MiB. Every frame has a `kind`.

| kind | direction | shape |
|---|---|---|
| `ready` | sidecar to client, first line | `{"kind":"ready","protocol":1,"app":"Banana Harness","version":"0.1.0","api_key_present":true}` |
| `fatal` | sidecar to client, then exit 1 | `{"kind":"fatal","error":{"code":"...","message":"..."}}` (e.g. database cannot be opened) |
| `request` | client to sidecar | `{"kind":"request","id":"r17","method":"turn.start","params":{...}}` |
| `response` | sidecar to client | `{"kind":"response","id":"r17","ok":true,"result":{...}}` or `{"kind":"response","id":"r17","ok":false,"error":{"code":"turn_in_progress","message":"...","data":{...}}}` |
| `event` | sidecar to client, unsolicited | `{"kind":"event","workspace":"/abs/path","session_id":"01k...","turn_id":"01k..."|null,"event":{"id":"01k...","type":"model.message.delta","thread_id":"...","message_id":"...","content":"Hel"}}` |

- `id` is a client-chosen string, unique among in-flight requests. Responses may arrive in any order; the client correlates by `id`.
- `event.event` is core's `Event`, unchanged, so replayed (`transcript`) and live events are the same type. `turn_id` is `null` only for a turn that failed before `turn.created`.
- **Versioning:** `protocol` is one integer. Adding methods, optional params, result fields or event kinds does not bump it; a breaking change does. The client refuses to continue when `protocol` differs from the one it was built for. Unknown params fields are ignored by the sidecar; unknown result fields and event types are ignored by the client.
- **Error codes** (a `thiserror` enum in the bridge, because the dispatcher branches on it to build the frame): `invalid_request`, `unknown_method`, `invalid_params`, `workspace_not_found`, `workspace_not_open`, `workspace_mismatch`, `session_not_found`, `turn_in_progress`, `no_active_turn`, `unknown_action`, `api_key_missing`, `internal`. `message` is human-readable and passes through the key redactor; `data` is optional structured detail.

### 3. Method surface

Workspaces are identified by their canonical absolute path (the same string stored in `session.workspace`). Sessions by `session_id`.

| method | params | result |
|---|---|---|
| `session.list` | `{}` | `{"sessions":[Session]}` all workspaces, newest first; served from a `Store`, works before any workspace is open |
| `workspace.open` | `{"path", "trust_workspace_config"?: false, "provider"?}` | `{"workspace", "already_open", "provider", "workspace_config":{"present","loaded"}}` |
| `workspace.close` | `{"workspace"}` | `{}`; cancels a running turn, drops the `Harness`; sessions stay in the database |
| `workspace.list` | `{}` | `{"workspaces":[{"workspace","provider","active_turn": null or {"session_id","turn_id","pending_action_ids":[...]}}]}` |
| `provider.list` | `{"workspace"}` | `{"providers":[ProviderOption],"default":"deepseek"}` (v1: one model per provider) |
| `session.create` | `{"workspace","provider"?}` | `{"session":Session}` |
| `session.resume` | `{"workspace","session_id"}` | `{"session":Session,"turns":[TurnRecord]}` (`Harness::transcript`) |
| `session.rename` | `{"workspace","session_id","title"}` | `{"session":Session}` |
| `session.switch_provider` | `{"workspace","session_id","provider"}` | `{"session":Session}` |
| `session.undo` | `{"workspace","session_id"}` | `{"outcome":"restored"|"nothing_to_undo"}`; refused with `turn_in_progress` while a turn runs |
| `turn.start` | `{"workspace","session_id","input"}` | `{}` accepted; the turn's events follow as `event` frames |
| `turn.cancel` | `{"workspace","session_id"}` | `{"cancelled":bool}` |
| `action.reply` | `{"workspace","session_id","action_id","answer"}` | `{}`; `answer` is core's `Answer` JSON: `"allow"`, `"allow_for_session"`, `"deny"` or `{"text":"..."}` |
| `changes.workspace` | `{"workspace"}` | `{"files":[FileChange]}` |
| `changes.turn` | `{"workspace","session_id"}` | `{"files":[FileChange]}` last turn vs its checkpoint |
| `shutdown` | `{}` | `{}` then the process exits |

- `Session`, `TurnRecord`, `ProviderOption`, `FileChange` and `Event` are the core and store types from ticket 10, serialized as-is. The bridge defines request/response envelopes and per-method param structs only; it defines no mirrors of those types.
- There is no file-read, file-write, shell or directory-listing method. The renderer sees file content only as `FileChange.patch` and tool-call events.
- `workspace.open` is idempotent: an already-open workspace is reused with `already_open: true`. A different `trust_workspace_config` than the open one is ignored and reported through `workspace_config.loaded`; change it with `close` then `open`.
- `session.create` with a `provider` creates the session, then `switch_provider`s it.
- `session.resume` checks that the session's stored workspace equals the open one, else `workspace_mismatch`.
- Not in v1: MCP status and the OAuth prompt as methods (MCP presentation stays in the map's fog; `mcp.auth_required` still arrives as an ordinary event), session delete, file viewing.

### 4. Event routing and turn lifecycle

- State: `workspaces: Map<canonical path, Arc<WorkspaceState>>`, each holding its `Harness` (with its own `Store::open` handle) and at most one active turn `{session_id, turn_id, TurnHandle, pending_action_ids}`.
- `turn.start` validates the workspace and session, rejects with `turn_in_progress` if the **workspace** already has a running turn (v1 rule: one running turn per workspace, because checkpoints and undo assume one agent changing a tree at a time; it can be relaxed later without a protocol change), registers the turn, replies `{}`, and spawns one forwarder task that moves `TurnHandle.events` into the output queue wrapped as `event` frames in order.
- The forwarder learns `turn_id` from `turn.created` and tracks `tool.approval_required` / `tool.response_required` ids into `pending_action_ids`; they leave on `action.reply`, `turn.cancel` and `turn.done`. The active turn is cleared after the `turn.done` frame has been queued, so a client that sees `turn.done` can immediately start the next turn.
- `action.reply` is checked against `pending_action_ids`; an id not pending is `unknown_action` (stricter than core, which silently ignores it). The bridge never auto-answers anything.
- Failure of a turn is just its `turn.done{state:error, error}` event; a request-level failure is an error `response`. Streamed deltas, tool calls, required actions, compaction, thread events and MCP auth events all use the same `event` frame.
- A renderer reload does not lose events (the Electron main process owns the sidecar connection); `workspace.list` plus `session.resume` rebuild the view, and a live delta stream resumes from the next frame. Who buffers deltas for a reloaded renderer is for ticket 06/07 (see "Hand-offs").

### 5. Core integration (follows ticket 10)

`session.list` uses `Store::sessions` (one extra `Store` handle owned by the bridge); every other session method goes through the workspace's `Harness`: `create_session`, `transcript`, `rename_session`, `switch_provider`, `undo`, `start_turn`, `providers`, `workspace_changes`, `turn_changes`. The last-provider fallback is exactly ticket 10's: explicit `provider` param, else `last_provider(store)`, else DeepSeek. Blocking calls (`undo`, `workspace_changes`, `turn_changes`, store reads) run on `spawn_blocking`.

**Core changes this design needs, in addition to ticket 10** (small, additive):
1. **Amend `TurnRecord.state` to `Option<TurnState>`.** `None` means the turn never finished (the process died while it ran; the database still says `running`). Ticket 10 typed it as `TurnState`, which cannot represent that and would make `transcript` fail on any crashed turn. Ticket 10 is amended below.
2. **Shared config paths.** Core gains `user_config_path() -> Option<PathBuf>` and `workspace_config_path(&Path) -> PathBuf`, extracted from `main.rs`; the headless run and the bridge both use them and the inline copy in `main.rs` is deleted. (The TUI's embedded-default-only behaviour is a separate, pre-existing divergence from the README; see "Uncertainties".)
3. **Export the redactor.** `harness_core::Redactor` (currently private) becomes public so the bridge redacts error messages with the same single implementation; it is never reimplemented.

### 6. Workspace ownership

- `workspace.open` canonicalizes the path, requires an existing directory (`workspace_not_found`), loads `Config::load` with the user layer and, only when `trust_workspace_config` is true and the file exists, the workspace layer, then `Harness::open` with a fresh `Store::open(db_path)` and `headless: false`. Concurrent opens of the same path share one in-flight open (no double `Harness`).
- A `Harness` lives until `workspace.close` or shutdown. Closing never deletes sessions.
- Why the trust flag: a workspace's `.banana-harness/config.toml` can define MCP servers (spawning arbitrary commands), loosen permission rules and point a provider's `base_url` at another host, which would receive the API key. Opening an unfamiliar folder from a GUI must not do that silently. The flag defaults to `false`; the GUI sets it only after an explicit per-workspace "trust" confirmation (ticket 06 designs that UI and where the choice is remembered). The existing headless run keeps loading the workspace layer as today.

### 7. Reliability

- **Malformed input never crashes the sidecar.** Not-JSON, invalid UTF-8, an oversized line (discarded up to the next newline) or a non-object get `{"kind":"response","id":null,"ok":false,"error":{"code":"invalid_request",...}}` and the loop continues. A parseable request with an unknown `method` or bad `params` gets an error response carrying its `id`. A reused in-flight `id` is `invalid_request`.
- **Concurrency:** each request runs in its own task; one writer task owns stdout.
- **Backpressure:** responses and events share one bounded queue (1024 frames) into the writer task. Frames are never dropped or coalesced. If the client stops reading, the pipe fills, the queue fills, the forwarder blocks, and core's bounded turn channel blocks the turn: the agent pauses instead of memory growing. If stdout fails (broken pipe) the sidecar treats it as the client being gone and shuts down. ponytail: no slow-consumer timeout; add one if a stalled renderer ever wedges turns.
- **Shutdown:** stdin EOF, `SIGTERM`/`SIGINT` or `shutdown` cancel every active turn (pending actions resolve as denied per ADR 0002), wait up to 3 s for each `turn.done`, drop the harnesses and exit 0. A clean `shutdown` replies before exiting.
- **Sidecar crash or kill:** the running turn is lost (ADR 0002 already accepts this); the database stays consistent (SQLite WAL). On the next `session.resume` that turn appears with `state: null`, and the session continues from its last persisted context. A crash while a required action waits cannot be resumed.
- **Restart:** the sidecar never restarts itself. Electron's main process respawns it (policy in ticket 06), then re-issues `workspace.open` for the workspaces that were open and `session.resume` for the visible session. Every in-flight request at crash time is failed locally by the client (`sidecar_exited`). No sidecar state needs recovering beyond that.

### 8. Security

- **The API key reaches the sidecar only through its environment** (`AI_API_KEY`, read once with `api_key_from_env` in the subcommand) and is never present in any request, response, event or log line. `ready.api_key_present` tells the UI whether to show onboarding; `workspace.open` fails with `api_key_missing` otherwise. How a Finder-launched app obtains the variable is open (see Uncertainties).
- Events and transcripts are already redacted by core before they leave it; error messages in `response` frames are redacted with the exported `Redactor`.
- Stderr logs carry method names, request ids, error codes and workspace paths only: never params, results, event payloads, tool arguments or file content.
- No filesystem, shell or config-write surface in the protocol (section 3), and `action.reply` only resolves ids the bridge saw as pending.
- The keys-never-in-config, workspace-confinement and guardrail rules are core behaviour and are unchanged: the bridge adds no tools and no permission bypass, and never sets `headless`.
- Electron-side validation (zod schemas on every frame, preload surface, IPC naming) is ticket 06.

### 9. Testing and the ticket 05 seam

Tests live in `crates/harness-bridge/tests/` and drive the protocol from outside; they never reach into bridge internals or core privates.

- **In-process protocol tests** call `serve` over `tokio::io::duplex`, one `BridgeOptions` per test with a temp `db_path`, a temp `user_config`, and a fake `api_key`.
  - handshake: first frame is `ready` with `protocol: 1`;
  - request/response correlation, out-of-order responses, duplicate id;
  - malformed lines: not JSON, invalid UTF-8, oversized, non-object, unknown method, bad params, each answered without ending the connection;
  - `session.list` works with no workspace open; `workspace.open` is idempotent, canonicalizes, rejects a missing directory, reports `api_key_missing`, and honours the trust flag (a workspace-layer `base_url` is ignored when untrusted and used when trusted);
  - full turn: `turn.start` then ordered `event` frames to `turn.done`, correct `workspace`, `session_id`, `turn_id`;
  - approvals: `tool.approval_required` then `action.reply` (allow, allow_for_session, deny, text) and `unknown_action`;
  - `turn.cancel` denies pending actions and ends `cancelled`; `turn_in_progress` per workspace; undo refused mid-turn;
  - resume: `session.resume` returns the transcript, a reopened bridge on the same database returns the same turns, an interrupted turn appears with `state: null`, `workspace_mismatch` is raised;
  - rename, switch_provider, `provider.list`, `changes.workspace`, `changes.turn`;
  - two workspaces open at once route events to the right `workspace`; closing one cancels only its turn;
  - shutdown cancels turns and returns; backpressure (a reader that stalls does not lose or reorder frames once it resumes);
  - the key never appears in any frame or captured stderr, even when a tool echoes it.
- **One process-level test** spawns the real `banana-harness serve --stdio` (`CARGO_BIN_EXE_banana-harness`, owned by the binary crate's `tests/`) with `AI_API_KEY`, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` pointed at temp dirs, and asserts that stdout is only valid NDJSON frames (including after the core emits a `warning:` line to stderr), that stdin EOF exits 0, and that a deliberately printed stray byte through a test-only child does not reach the pipe.
- **Golden frames:** the in-process tests also write a small set of representative frames per method and event kind to `crates/harness-bridge/tests/fixtures/*.ndjson`, checked in. The renderer's schema tests (ticket 06) validate the same files, which keeps the Rust and TypeScript sides honest without a code generator.
- **The fake-provider seam (for ticket 05).** Production code gets no test hook. The bridge reaches a model only through `base_url` in layered config, and the database and user-config paths come from `BridgeOptions` (in-process) or `XDG_*` variables (process level), both ordinary mechanisms. So any HTTP stub that speaks the provider's SSE dialect at a loopback URL works for the bridge tests and for Playwright. The stub is currently `FakeProvider` in `crates/harness-core/tests/common`, which no other crate can import. Ticket 05 decides how one canonical stub is shared (and whether it also needs a scripted-tool-call mode for the approval flow); this ticket only fixes that the seam is config plus options, nothing else.

### Wire examples

Open a workspace, then a turn that needs an approval:

```
-> {"kind":"request","id":"1","method":"workspace.open","params":{"path":"/Users/me/app"}}
<- {"kind":"response","id":"1","ok":true,"result":{"workspace":"/Users/me/app","already_open":false,"provider":"deepseek","workspace_config":{"present":true,"loaded":false}}}
-> {"kind":"request","id":"2","method":"session.create","params":{"workspace":"/Users/me/app"}}
<- {"kind":"response","id":"2","ok":true,"result":{"session":{"id":"01k...","title":null,"workspace":"/Users/me/app","provider":"deepseek","model":"deepseek-flash","created_at":"...","updated_at":"..."}}}
-> {"kind":"request","id":"3","method":"turn.start","params":{"workspace":"/Users/me/app","session_id":"01k...","input":"add a README"}}
<- {"kind":"response","id":"3","ok":true,"result":{}}
<- {"kind":"event","workspace":"/Users/me/app","session_id":"01k...","turn_id":"01t...","event":{"id":"01e...","type":"turn.created","session_id":"01k...","turn_id":"01t..."}}
<- {"kind":"event","workspace":"/Users/me/app","session_id":"01k...","turn_id":"01t...","event":{"id":"01e...","type":"tool.approval_required","thread_id":"01h...","action_id":"01a...","name":"bash","arguments":"{\"command\":\"make test\"}","rule":"bash:make *"}}
-> {"kind":"request","id":"4","method":"action.reply","params":{"workspace":"/Users/me/app","session_id":"01k...","action_id":"01a...","answer":"allow_for_session"}}
<- {"kind":"response","id":"4","ok":true,"result":{}}
```

A second turn while one runs, and a malformed line:

```
-> {"kind":"request","id":"5","method":"turn.start","params":{"workspace":"/Users/me/app","session_id":"01k...","input":"again"}}
<- {"kind":"response","id":"5","ok":false,"error":{"code":"turn_in_progress","message":"a turn is already running in this workspace","data":{"session_id":"01k..."}}}
-> not json
<- {"kind":"response","id":null,"ok":false,"error":{"code":"invalid_request","message":"line is not valid JSON"}}
```

### Acceptance criteria for the build run

- `crates/harness-bridge` exists with `serve` as above; `banana-harness serve --stdio` runs it; `AGENTS.md` crate-boundary line, `README.md` and `CONTEXT.md` (sidecar vocabulary, see ticket 09) are updated in the same change.
- Every method in section 3 behaves as specified, with the error codes in section 2, and every test in section 9 exists and passes under `make test`; `make lint` passes with `print_stdout`/`print_stderr` denied in the bridge.
- stdout of `serve --stdio` is provably NDJSON-only; the key appears nowhere in frames or logs.
- Core changes listed here and in ticket 10 are done with the duplicates deleted (`main.rs` inline config-path code included).
- Golden frame fixtures are checked in for each method and event kind.

### Uncertainties and hand-offs

- **How the key reaches a Finder-launched app (ticket 06).** A macOS app started from the Dock does not inherit shell exports, and the repo's rule is that the key comes only from `AI_API_KEY` and never from config. Ticket 06 must decide how Electron's main process obtains the variable for the sidecar's environment without writing the key into any config or log (for example reading it from the user's login shell or an OS-protected store) and must confirm that is consistent with `AGENTS.md`.
- **Workspace trust UX (ticket 06).** Where the per-workspace trust decision is stored and how it is shown. The protocol only carries `trust_workspace_config`.
- **Live-event buffering across a renderer reload (tickets 06/07).** The protocol keeps sending frames to the main process; whether main buffers the active turn's frames for a reloaded renderer, or the renderer calls `session.resume` and accepts a missed partial delta, is undecided.
- **Interrupted turns left as `running`.** v1 shows them as interrupted (`state: null`). Marking them finished on the next open would be a core change, not made here.
- **TUI config divergence.** The TUI opens with `Config::embedded_default()` and ignores the user and workspace layers the README describes, while the headless run and the bridge honour them. Pre-existing and out of scope here; worth a separate look.
- **Concurrent stores.** The bridge assumes several `Store` handles on one WAL database with a 5 s busy timeout are fine under turn load; ticket 10 requires a core test for it.
- **MCP presentation.** `mcp.status` and OAuth prompting are deliberately not methods yet; if v1 needs them, `McpServerStatus` needs serde first.

### Amendment (from ticket 06, 2026-10-17)

- `workspace.open`'s `workspace_config` result is `{ "present", "loaded", "path" }`, where `path` is the absolute config file path from core's `workspace_config_path`, so the GUI never hard-codes the layout (it needs the path to hash and summarise the file for the trust dialog).
- `provider.list`'s `ProviderOption` carries `host` (see ticket 10). The GUI uses it to show where the key goes and to block a trusted workspace whose config redirects the provider.
- Both are additive; protocol version stays 1. See [Electron main/preload design](06-electron-process-and-security.md#3-workspace-trust).

### Amendment (from ticket 07, 2026-10-17)

`changes.workspace` returns `{ "git": bool, "files": [FileChange] }`, `changes.turn` returns `{ "checkpoint": bool, "files": [FileChange] }`, and `session.undo` returns `{ "outcome": "restored" | "nothing_to_undo", "event"?: Event }` (`event` is `turn.undone` when restored). Additive; protocol version stays 1.

### Amendment (from ticket 05, 2026-10-17)

The fake-provider seam is settled: a dev-dependency-only `harness-testkit` crate owns the stub, fixtures, `TestEnv` and scenario files; the product is reached only through `base_url` in layered config and `XDG_*` paths (verified with the real binary). The same scenario files drive the in-process bridge tests, the real `serve --stdio` process test and the Playwright suite. See [ticket 05](05-fake-provider-for-smoke-tests.md).

### Amendment (from ticket 08, 2026-10-17)

`harness-bridge` and `banana-harness serve --stdio` are built by `make setup` (default members) and shipped as the single binary inside the macOS bundle at `Contents/Resources/bin/banana-harness`; `harness-testkit` is excluded from default members. See [ticket 08](08-repo-layout-make-ci-packaging.md).
