# Running the sidecar against the fake provider

Type: grilling
Status: resolved
Blocked by:

## Question

The fake provider is in-test code, and `AGENTS.md` forbids test hooks leaking into production. How do the Playwright smoke test and the sidecar integration tests drive a real sidecar with scripted model output (a test-only binary or feature, an env-selected fixture provider, an HTTP stub of the provider API)? Which option keeps one canonical owner for the fake and no production shim?

## Answer

Resolved 2026-10-17. Builds on [the wire protocol](03-sidecar-placement-and-protocol.md) (stub through `base_url`), [the Electron design](06-electron-process-and-security.md), [the renderer state](07-renderer-state-and-changes-panel.md) and [the core additions](10-core-api-additions.md). **Nothing is implemented**: no Rust, no GUI, no test harness. Every statement below is labelled **Verified** (observed this session, by reading code or running it) or **Proposed** (a design not yet built or exercised).

### A. What was verified

**Verified by reading the code**
- `crates/harness-llm/src/lib.rs`: the client is a plain `reqwest::Client` (workspace `reqwest` has the `system-proxy` feature), `connect_timeout` 30 s, `read_timeout` 660 s, no TLS requirement. It posts to `format!("{}/chat/completions", base_url.trim_end_matches('/'))` with `bearer_auth(api_key)`, `"stream": true`, and a JSON body containing the model and messages. A plain `http://` base URL is accepted.
- `base_url` is an ordinary config key (`[providers.deepseek]` / `[providers.qwen]`) merged by `Config::load` from embedded defaults, then the user file, then the workspace file. Nothing production-side needs to change to point a provider elsewhere.
- The existing fake, `FakeProvider` in `crates/harness-core/tests/common/mod.rs`, is a hand-rolled HTTP/1.1 server on `127.0.0.1:0` (`TcpListener`), so no external dependency. Each request pops the next queued reply; an optional matcher serves a reply only to a request whose body contains a needle (so parallel subagents each get their own answer); `chunk_delay` pauses before each SSE part; an empty queue returns HTTP 500 `fake provider: no reply queued`; replies declare `content-length` and `connection: close`; every request is recorded with lowercased header names and the parsed JSON body; `max_in_flight` is tracked. Helpers `sse_text`, `sse_tool_calls`, and `fixture()` (10 `.sse` files in `tests/fixtures/`) build the replies. Limitation: it is test-only code in `harness-core/tests/common`, so no other crate and no external process can use it, and its server loop uses `unwrap()` (a client that disconnects panics that task).
- `TestHarness` builds a real `Harness` against it with a temp workspace (`git init`), a temp database, and the key `sk-test-0123456789abcdef`.

**Verified by running (this machine: macOS, Rust 1.95, a temporary `CARGO_TARGET_DIR`)**
1. `cargo test -p harness-core --test plain_answer --locked`: 4 passed.
2. The same test with a hostile proxy environment (`HTTP_PROXY`, `http_proxy`, `ALL_PROXY` set to `http://127.0.0.1:9`): **all 4 failed**. Adding `NO_PROXY=127.0.0.1,localhost`: **all 4 passed**. So a proxy variable in the environment breaks the existing offline suite, and `NO_PROXY` for loopback fixes it. `scutil --proxy` printed no OS-level proxy on this Mac, so macOS system-proxy settings were **not** exercised.
3. The real binary through a throwaway stub (a 20-line Python SSE server serving the core `deepseek_plain.sse` fixture), run as `env -i PATH=… HOME=<tmp>/home XDG_CONFIG_HOME=<tmp>/config XDG_DATA_HOME=<tmp>/data NO_PROXY=127.0.0.1,localhost AI_API_KEY=sk-e2e-not-real WORKSPACE=<tmp>/ws banana-harness run --provider deepseek --json "say hi"`, with `<tmp>/config/banana-harness/config.toml` setting both providers' `base_url` to `http://127.0.0.1:18765`:
   - the stub received `POST /chat/completions`, header `Authorization: Bearer sk-e2e-not-real`, model `deepseek-flash`;
   - the events were `turn.created`, `model.message` (the live **start marker**, no `message`), two `model.message.delta` ("Hello", " there!"), `model.message` (final, with content), `turn.done{state: done}`, which matches the sequence ticket 07's reducer assumes;
   - the database was created at `<tmp>/data/banana-harness/harness.db`, and `XDG_CONFIG_HOME` was honoured for the user config, both **on macOS**, so `XDG_*` isolation works without any code change;
   - the real `~/.local/share/banana-harness` was not touched.
4. **Not run:** `banana-harness serve --stdio` and `harness-bridge` do not exist yet; Playwright, Electron and the stub as a standalone binary were not run; Linux was not run (etcetera uses native XDG paths there, which is expected but unobserved).

**Pre-existing finding (affects the evaluation path, not only the GUI).** The offline suite that `make test` runs fails whenever a proxy variable is set in the environment without a loopback `NO_PROXY`, because the in-process client honours it and sends the loopback request to the proxy. An evaluator machine behind a proxy would see `make test` fail with no product fault. Fix proposed in section G.

### B. Where the fake lives (Proposed)

The fake must be usable by `harness-core` tests, `harness-bridge` tests, the binary's process-level test and Playwright, with one canonical owner (`AGENTS.md`: one owner per helper, no duplicates). Option comparison:

| Option | Verdict |
|---|---|
| Keep it in `harness-core/tests/common` and `#[path]`-include it elsewhere | duplicates compilation, brittle, and Playwright cannot run it. Rejected. |
| A second stub (Python or Node) for Playwright | two implementations drift apart; violates single owner. Rejected. |
| **A new workspace crate `harness-testkit`** (library plus one binary) | one implementation, usable in-process and as an external process. **Chosen.** |

- `harness-testkit` depends on no product crate (only `tokio`, `serde`, `serde_json`, `tempfile`, `clap`, all already pinned), so there is no dependency cycle with `harness-core`. It is a `[dev-dependencies]` entry in `harness-core`, `harness-bridge` and `banana-harness`, never a normal dependency, and nothing in the shipped binary links it.
- It owns: the `FakeProvider` server (moved from `harness-core/tests/common`, public API kept so existing tests change only their `use`), the SSE fixtures (moved from `harness-core/tests/fixtures/`), the SSE builders, the `TestEnv` recipe (section D), the scenario type (section C), and the `fake-provider` binary. `TestHarness` stays in core's `tests/common` because it needs `Harness`.
- `AGENTS.md` crate-boundary line gains `harness-testkit` as a test-support crate (dev-dependency only). Crate-level lint relaxation for unwrap is allowed because it is test-support code, but the server loop handles I/O errors without panicking (log to stderr, drop that connection).
- Workspace `Cargo.toml` gets `default-members` listing the product crates, so `make setup` (`cargo build --release --locked`) stays lean and never builds the testkit; `cargo test --workspace` and `cargo clippy --workspace` still cover it.
- **No production hook:** the product reads only `base_url` from layered config and the XDG paths from the environment, exactly as in the verified run. The only test-only environment variables live in the GUI test code (`BANANA_HARNESS_BIN`, `FAKE_PROVIDER_BIN`, read by Playwright setup, and the existing ticket 06 `BANANA_HARNESS_BIN` which is honoured only when the app is not packaged).

### C. Stub protocol and deterministic scenarios (Proposed; the request/response behaviour below is the verified existing behaviour plus additions)

**Provider surface (unchanged from the existing fake).** `POST <base>/chat/completions`, any path is accepted and recorded; reply selection is queue order, restricted by an optional body-substring matcher; SSE is written one event at a time with an optional delay per part; an unmatched or empty queue gives HTTP 500 `fake provider: no reply queued` and is recorded as `unmatched`. Both providers' `base_url` are set to the same stub (DeepSeek and Qwen dialects are both served; the model name in the request distinguishes them). The authorization header is recorded as `[redacted]` plus a boolean `auth_ok` computed against the expected key, so no log or artifact contains the key.

**Additions (all Proposed):**
- **Control API** on the same listener under `/__fake/`: `POST /__fake/replies` (queue replies), `GET /__fake/requests` (the recorded requests, key redacted), `GET /__fake/status` (`{queued, consumed, unmatched}`), `POST /__fake/release/<gate>` (open a gate), `POST /__fake/reset`, `GET /__fake/health`. The product never calls `/__fake/`; a request to `/chat/completions` is the only provider path. In-process Rust tests call the same state through typed methods.
- **Gates for determinism.** A reply part can name a gate; the stub pauses before that part until `release` is called. This makes "reload the renderer mid-stream", "cancel while streaming" and "kill the sidecar mid-turn" deterministic with no sleeps. `delay_ms` per part remains for slow streams.
- **Failure replies:** `status` with a JSON error body (401, 429, 500), `abort_after_parts: n` (close the connection mid-stream), `hang` (accept and never answer; only valid with a test that cancels), each exercising the client's real error paths.
- **Watchdog:** any reply still unfinished 30 s after it started is aborted by the stub. The product client's read timeout is 660 s, so without this a scripting mistake would hang a test for 11 minutes.
- **Leftover check:** at teardown a fixture asserts `queued == 0 && unmatched == 0` (opt out per test), which catches scripts that drift from product behaviour.
- **Multiple instances:** several stubs can run at once on separate ports, e.g. a baseline stub and an `evil` stub for the ticket 06 `base_url` tests; each records its own requests.

**Scenario file (canonical type in Rust, JSON on disk):** `crates/harness-testkit/scenarios/<name>.json`.
```json
{
  "name": "edit-and-approve",
  "workspace": "hello-repo",
  "replies": [
    { "sse": { "tool_calls": [ { "id": "call_1", "name": "write_file", "arguments": { "path": "NOTES.md", "content": "hi\n" } } ] } },
    { "sse": { "tool_calls": [ { "id": "call_2", "name": "bash", "arguments": { "command": "git push origin main" } } ] }, "gate": "after-edit" },
    { "sse": { "text": "Done. I wrote NOTES.md." } }
  ],
  "steps": [
    { "send": "write a note and push it" },
    { "approve": "deny" },
    { "wait_for": "turn.done" }
  ],
  "expect": {
    "events": ["turn.created", "model.message", "tool.call", "tool.response", "tool.approval_required", "tool.response", "model.message", "turn.done"],
    "requests": 3,
    "request_contains": [ { "index": 2, "text": "NOTES.md" } ],
    "files": { "NOTES.md": "hi\n" },
    "ui": [ "I wrote NOTES.md.", "Edited 1 file" ]
  }
}
```
- `sse` is either generated (`text`, `tool_calls`, built by the testkit's builders, covering both dialects) or `file` naming a raw fixture; `match`, `delay_ms`, `gate`, `status`, `abort_after_parts`, `hang` are optional per reply.
- `steps` is a small verb set both drivers implement: `send`, `approve` (`allow` | `allow_for_session` | `deny`), `answer`, `cancel`, `release` (a gate), `wait_for` (an event type), plus `kill_sidecar` (process-level and Electron only).
- `expect.requests` and `expect.request_contains` check what the **model** was sent; `expect.files` checks the workspace after the run. These are identical across drivers. `expect.events` is asserted by the Rust drivers (protocol frames, ids and timestamps normalised); `expect.ui` is asserted by Playwright (text that must be visible).
- A testkit test parses every scenario file, so a malformed scenario fails `cargo test`.

### D. Isolation recipe (`TestEnv`, Proposed; every ingredient except the last two is verified in A.3)

`TestEnv::new(scenario)` creates one temp root per test and returns the exact environment for the process under test:

| Ingredient | Value | Status |
|---|---|---|
| base environment | **empty** (`env -i` semantics); only `PATH` (and `DISPLAY` for Electron on Linux) are passed through, so the developer's real `AI_API_KEY`, proxy and git settings can never reach a test | verified (`env -i` run) |
| `HOME` | `<root>/home` | verified |
| `XDG_CONFIG_HOME` | `<root>/config`, containing `banana-harness/config.toml` that sets both providers' `base_url` to the stub (plus any `{{stub:name}}` placeholders in workspace files) | verified |
| `XDG_DATA_HOME` | `<root>/data` (database lands at `data/banana-harness/harness.db`) | verified |
| `NO_PROXY` / `no_proxy` | `127.0.0.1,localhost` | verified |
| `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` (+ lowercase) | `http://127.0.0.1:9` (a dead proxy) | the poisoned-proxy combination with `NO_PROXY` is verified; using it deliberately is Proposed |
| `AI_API_KEY` | `sk-test-0123456789abcdef` (the existing `TEST_API_KEY`, moved to the testkit); a `without_key()` variant and a `shell(path)` variant (a stub login-shell script) serve the ticket 06 credential tests | key reaches the stub as Bearer: verified |
| workspace | a copy of `workspaces/<name>/` into `<root>/workspace`, placeholders substituted (`{{stub:name}}`, `{{workspace}}`, `{{root}}`, `{{marker}}`), `git init`, a **local** `user.name`/`user.email`, one commit; returned as the **canonical** path | `git init` per test is existing practice; canonicalisation is Proposed |
| Electron userData | `<root>/userdata` via `--user-data-dir` (trust.json, preferences.json) | Proposed (ticket 06) |

- **Why a poisoned proxy:** with loopback excluded, any request that is *not* to the stub (an accidental call to a real provider, a telemetry call) is sent to a dead proxy and fails immediately, so a bug can never reach the internet or burn a real key. Verified: the hostile proxy plus `NO_PROXY` runs cleanly against the stub.
- **Canonical path:** the sidecar identifies a workspace by its canonical path. On macOS a temp directory under `/tmp` or `/var/folders` resolves through a symlink (`/private/...`), so tests must compare against the canonical path the testkit returns. This is expected macOS behaviour but was **not** observed in the verified run (which never compared paths).
- **Credential safety:** the test key is a fixed fake. The stub never records the real header value. A CI step greps the saved artifacts (frames, stub log, sidecar stderr, userData) for the test key and fails if found, which also proves redaction end to end.
- **MCP and trust fixtures** use `{{marker}}` (a file inside the temp root) so a malicious config that runs `sh -c "touch {{marker}}"` can be checked without touching the real file system.

### E. Process lifecycle, timeouts, diagnostics and cleanup (Proposed)

- **One implementation, two entry points.** The Rust tests call the testkit library directly. Playwright and shell use `fake-provider serve --scenario <file> [--stub baseline --stub evil]`: it creates the `TestEnv`, starts the stubs on port 0, prints **one JSON line** `{ "root", "workspace", "env": {…}, "stubs": { "baseline": { "provider_url", "control_url" } } }` and then serves until **stdin closes** (so a dying parent leaves no orphan, the same protection the sidecar has) or `SIGTERM`. On exit it removes `root`, unless `BH_TEST_KEEP=1` is set for debugging.
- **Start order:** stub first (its URL goes into the config), then the process under test. Ready detection is event driven: the stub's JSON line, then the sidecar's `ready` frame or Electron's first window. No fixed sleeps anywhere.
- **Rust bridge tests (in-process):** `serve` runs over `tokio::io::duplex` in the test; the stub is in the same process on port 0; both are dropped at the end of the test. Per-test timeout 60 s (`tokio::time::timeout`).
- **Process-level test (binary crate):** spawn `banana-harness serve --stdio` with `kill_on_drop(true)` and the `TestEnv` environment; wait up to 10 s for `ready`; at the end send `shutdown`, wait 5 s, then kill; a drop guard does the same on panic. Reads of stdout are bounded by timeouts; an invalid NDJSON line fails the test.
- **Playwright:** a worker-scoped fixture starts `fake-provider serve` and parses its line; a test-scoped fixture launches Electron with `_electron.launch({ args: [<built main>], env: prepared.env })` (the `env` option replaces `process.env`, as intended). Test timeout 90 s, expect timeout 15 s, `retries: 0` locally and 1 in CI with trace-on-first-retry so a retry is reported as a flake, not hidden. The fixture's teardown closes the app, sends `shutdown` through the app's normal quit path, closes the stub's stdin, and removes the root.
- **Failure diagnostics, saved under `test-results/<test>/` and uploaded by CI:** sidecar stderr (key-free by design), the stub's request log (authorization redacted), the protocol frames seen by the test (Rust) or the main-process log (Electron), the scenario's `expect` versus actual diff, and on failure the temp root is kept at `test-results/<test>/root` instead of deleted. A failing leftover check names the unconsumed reply or the unmatched request.
- **Timeouts that matter:** the product's 660 s provider read timeout is never reached because the stub's 30 s watchdog and the per-test timeouts fire first; `hang` replies are paired with a `cancel` step.
- **Port and ordering hygiene:** no fixed ports; no shared state between tests; the stub's request log and queue are per test.

### F. One scenario, three seams (Proposed)

The same `scenarios/<name>.json` drives three runners; each implements only the verbs and the assertions it can observe, and all share the **model-facing** and **file-system** assertions:

| Seam | Runner | Drives | Asserts |
|---|---|---|---|
| 1. Bridge, in-process | `harness-bridge/tests/scenarios.rs` | JSON requests over `duplex` (`workspace.open`, `session.create`, `turn.start`, `action.reply`, `turn.cancel`) | normalised frame sequence equals `expect.events`; `expect.requests`, `expect.request_contains`, `expect.files` |
| 2. Real sidecar process | `banana-harness/tests/serve_stdio.rs` | the same requests over the spawned process's stdio | the same, plus stdout is only valid NDJSON and the process exits 0 on `shutdown` |
| 3. Electron end to end | `gui/tests/e2e/scenarios.spec.ts` | the visible UI through Playwright (type in the composer, click Allow once or Deny, click Stop) | `expect.ui` text is visible; `expect.requests`, `expect.request_contains`, `expect.files` read from the stub's control API and the workspace |

- The stub's request log is the common ground: seams 1 and 3 must send the model the **same conversation** for the same scenario, which proves the UI path adds nothing and drops nothing.
- Shared scenarios for v1: `plain-answer`, `edit-and-approve` (approval allowed, once, for session, denied), `bash-and-test`, `question-and-answer` (`ask_question`), `subagents` (two parallel children with `match`ers), `slow-stream-gated` (gate mid-stream: reload or cancel), `provider-error-401`, `provider-abort-midstream`, `key-never-leaks` (a tool echoes the key). Ticket 06's trust and credential tests reuse the same `TestEnv` with the `evil` stub, `{{marker}}` and `without_key()`/`shell()` variants; ticket 07's reload and undo e2e tests are `slow-stream-gated` and `edit-and-undo`.
- The TS runner validates only the `steps` and `expect.ui` it uses (a small zod schema); the Rust `Scenario` type is canonical and a Rust test parses every file, so a schema change fails the Rust build first. This is the one place a type is described twice, kept honest by the shared files.

### G. Commands and CI (Proposed; Makefile wiring is ticket 08's)

```
# Rust: in-process scenarios, testkit self-tests, and the real-process serve test
cargo test --workspace --locked                      # already what `make test` runs
cargo test -p harness-testkit --locked               # stub, TestEnv, scenario parsing
cargo test -p harness-bridge --locked                # seam 1
cargo test -p banana-harness --test serve_stdio --locked   # seam 2

# Electron end to end (seam 3), after building the three binaries
cargo build --locked -p banana-harness -p harness-testkit --bin banana-harness --bin fake-provider
BANANA_HARNESS_BIN=target/debug/banana-harness FAKE_PROVIDER_BIN=target/debug/fake-provider \
  pnpm --dir gui test:e2e            # Linux CI: xvfb-run -a pnpm --dir gui test:e2e
```

- **Makefile fix for the pre-existing finding (Proposed, in the build run):** the `test` target exports `NO_PROXY=127.0.0.1,localhost` and `no_proxy` for the `cargo test` invocation, so the offline suite passes behind a proxy. This is independent of the GUI and protects the evaluation path.
- **CI:** the existing Linux and macOS matrix keeps `make lint` and `make test`; the e2e job runs on both with `xvfb-run` on Linux, uploads `test-results/` on failure, and runs the key-leak grep over those artifacts. Playwright's `retries: 1` flake report is surfaced in the job summary.

### H. Acceptance criteria for the build run

1. `harness-testkit` exists as a dev-dependency-only crate; the `FakeProvider` and fixtures are moved from `harness-core/tests`, and **every existing core test passes unchanged** apart from imports (the verified 4-test `plain_answer` result is the baseline).
2. The testkit's own tests cover: incremental SSE, matchers, queue order, empty queue gives 500 and is recorded as unmatched, gates and release, `abort_after_parts`, `hang` plus the 30 s watchdog, the control API, authorization redaction and `auth_ok`, the leftover check, two stubs at once, scenario parsing for every file, `prepare` output shape, cleanup removes the root, and exit on stdin EOF.
3. Isolation tests prove: a run under the poisoned-proxy environment reaches the stub; a request to a non-loopback address fails fast; the database lands under the temp `XDG_DATA_HOME` and the real data and config directories are untouched (checked by a before/after listing); the host's real `AI_API_KEY` never appears in the child environment.
4. The seams run the same scenario files and agree on `expect.requests`, `expect.request_contains` and `expect.files`; seam 1 and 2 agree on `expect.events`; seam 3 asserts `expect.ui`.
5. No product source gains a test hook: `git grep` for the testkit crate name outside `[dev-dependencies]` and tests finds nothing, and the release `banana-harness` binary does not link it (`default-members` excludes it).
6. `make setup` does not build the testkit; `make test` passes on a machine with `HTTP_PROXY` set and no `NO_PROXY` (the Makefile fix); `make lint` is clean.
7. Playwright smoke passes on macOS and, under `xvfb`, on Linux, with `retries: 0` locally; failure artifacts contain no occurrence of the test key.
8. Documentation: `AGENTS.md` (crate boundaries), `README.md` (how to run the e2e suite), and `CONTEXT.md` (term for the test-support crate if one is chosen) updated in the same change; ADR 0003 mentions the test seam.

### I. Limitations and what is still unverified

- Not verified: the bridge, `serve --stdio`, Playwright with Electron, Electron's environment allowlist passing `XDG_*` and the proxy variables to the sidecar as ticket 06 describes, Linux, macOS OS-level proxy settings (none configured here), and canonical-path behaviour for temp directories.
- The stub speaks the DeepSeek and Qwen **dialects as the existing fixtures encode them**; it is not a conformance test of the real providers (the live smoke test, run by `make test` when a key is set, remains the only real-provider check).
- Reply selection is body-substring matching; a scenario with parallel subagents needs needles that are unique per child, which is a scripting discipline, not enforced.
- The 30 s watchdog and 90 s Playwright timeout are starting values; slow CI runners may need tuning.
- HTTP only: nothing here exercises the product's TLS path.

### Amendments to earlier tickets

- **Ticket 10 / 06 (provider `host`):** while designing the `base_url` tests it became clear that `host` as "host only" cannot tell two loopback stubs apart (both are `127.0.0.1`) and, more importantly, would let a workspace redirect the key to a different **port** or downgrade to `http://` on the same host name. `ProviderOption.host` is therefore the URL **authority** (host plus an explicit non-default port, no userinfo), and `ProviderOption` gains `scheme`. Ticket 06's post-open check blocks any provider whose scheme is not `https`, unless the host is loopback, regardless of what the user approved. The tests use `127.0.0.1:<port>` against `localhost:<port>` and a differing port for the `evil` stub.
- **Ticket 03 (testing section):** the "fake-provider seam" paragraph is now concrete: the stub is `harness-testkit`, reached only through `base_url` and `XDG_*`; `BridgeOptions` carries the paths for in-process tests.
- **Ticket 06 (tests):** the credential, trust and malicious-config tests use this `TestEnv` (the `evil` stub, `{{marker}}`, `without_key()` and `shell()`).
- **Ticket 07 (tests):** the reload-mid-turn, interrupted-turn and undo end-to-end tests are the `slow-stream-gated` and `edit-and-undo` scenarios; undo's file-system assertion is `expect.files`.
- **Ticket 08 (to do, unblocked):** add `harness-testkit` and `default-members` to the workspace layout, the `make` targets for the e2e suite, the CI matrix (including `xvfb-run` on Linux and artifact upload), and the Makefile `NO_PROXY` fix.

### Questions that need your decision

None block the build; these have defaults.
1. **A new test-support crate and an `AGENTS.md` crate-boundary change.** Required to give the fake one owner. Default: yes, as a dev-dependency-only crate.
2. **The Makefile `NO_PROXY` fix.** It repairs a pre-existing failure of `make test` behind a proxy and is outside the GUI scope, but the stub tests depend on the same behaviour. Default: include it in the build run.

### Amendment (from ticket 08, 2026-10-17)

`harness-testkit` is approved by the user. The e2e entry point is `make gui-e2e`, which builds the debug sidecar and `fake-provider` and sets `BANANA_HARNESS_BIN` and `FAKE_PROVIDER_BIN` (test-side only). CI uploads `gui/test-results/` on failure after a grep proves the test key is absent. See [ticket 08](08-repo-layout-make-ci-packaging.md).
