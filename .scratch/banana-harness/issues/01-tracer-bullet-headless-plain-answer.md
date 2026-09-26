# 01: Tracer bullet: a headless run answers a plain question end to end

**What to build:** create the Cargo workspace in the `rust/` subfolder, with its seven packages, pinned toolchain and pinned dependencies. Add a root Makefile, a `.env.example`, the product-name constants, and the committed default config for both providers.

Wire the thinnest complete path:

`banana-harness run --provider deepseek "<question>"`
→ core opens a session
→ the turn runner streams the model's plain-text reply from the real OpenAI-compatible client (thinking off)
→ it persists the session, turn and durable events to SQLite (migrations included)
→ it prints the reply.

No tools yet. Also build the **fake OpenAI-compatible HTTP server** test helper that all later core tests use (seam 1): it serves queued SSE fixtures, and the provider base URL points at it.

**Blocked by:** None (can start immediately)

**Status:** done, except the real-key check (unverified: no `AI_API_KEY` was available)

- [x] `make setup` succeeds on a fresh Linux container with no Rust installed (it installs rustup) and on macOS. It exits with an actionable message when no C compiler is present.
- [x] `make test` passes offline with no `AI_API_KEY`.
- [x] A seam-1 test gets a plain answer through the fake server, both for a DeepSeek-format stream and for a Qwen-format stream (usage on the last chunk). It asserts the event sequence (turn created, model message start, deltas, final message, turn done) and that the rows can be read back from SQLite.
- [x] A `length` finish reason ends the turn with the error "max_tokens breached". An HTTP error from the provider ends the turn with an error. Neither panics.
- [ ] With a real key, `banana-harness run --provider deepseek "say hi"` and `--provider qwen` both print a reply.
- [x] The key comes only from `AI_API_KEY`. It is redacted in the persisted rows and never appears in any committed file.
- [x] `cargo fmt --check` and clippy with warnings denied are clean.

## Notes

For tickets 02, 03, 08, 09 and later.

**Layout** (`rust/`, edition 2024, resolver 3, `rust-toolchain.toml` = 1.95 + rustfmt + clippy):
- `crates/harness-llm`: `ChatClient::new(ClientConfig{dialect, base_url, api_key, model, temperature, seed})`, then `client.stream(&[Message]) -> ChatStream`. `ChatStream::next()` yields `ChatEvent::Delta(String)`, then one `ChatEvent::Done(Completion{content, finish_reason, usage})`, then `None`. `Dialect::{DeepSeek, Qwen}` sets thinking off (`thinking:{type:disabled}` / `enable_thinking:false`) and sends seed only to Qwen. It always sends `stream_options.include_usage`. Usage comes from whichever chunk carries it. A stream that ends without a `finish_reason` is an error. The client installs the rustls `ring` provider itself. Tool-call deltas are ticket 03's job: extend `Delta`/`Completion` and assemble by `index`.
- `crates/harness-store`: `Store::open(path)` (WAL, foreign keys, migrations). Insert types: `NewSession`/`NewTurn`/`FinishTurn`/`NewEvent`. Read types: `Session`/`Turn`/`Event`, via `session(id)`, `turns(session_id)` and `events(turn_id)` (in rowid order). Timestamps are filled in by SQL as ISO-8601 UTC text.
- `crates/harness-core`: `Config`, `Provider`, `Event`/`EventKind`, `Harness`, plus `APP_NAME`, `APP_SLUG`, `API_KEY_ENV`, `api_key_from_env()` and `default_db_path()`. It re-exports `Store` and `Usage`.
- `crates/harness-tools`, `harness-mcp` and `harness-tui` are empty libs, already wired into the dependency graph from the spec (core depends on the four leaves; tui depends on core; the bin depends on core and tui).
- `crates/banana-harness`: the bin. There is only a `run` subcommand so far; the subcommand is required until 02 makes the bare command open the TUI.
- Every third-party version is pinned once in `[workspace.dependencies]` (the whole spec table, including `rmcp = "=3.4.1"`). Members inherit with `x.workspace = true`. Workspace lints deny `unwrap_used`/`expect_used`, so test crates start with `#![allow(clippy::unwrap_used, clippy::expect_used)]`.

**Harness API:**
- `Harness::open(HarnessOptions{config, store, provider, api_key, workspace})`
- `harness.create_session() -> session_id`
- `harness.start_turn(&session_id, input) -> TurnHandle{events: mpsc::Receiver<Event>}`. It must be called inside a tokio runtime, and the stream always ends with `turn.done`.
- `reply`/`cancel` don't exist yet. 02 adds `cancel`, the `CancellationToken` and `TurnState::Cancelled`; 05 adds `reply`.
- The key is passed in explicitly. Surfaces read it with `api_key_from_env()`; tests pass `TEST_API_KEY`.

**Events** (`#[serde(tag="type")]`, flattened next to `id`, a monotonic lowercase ULID):
- `turn.created{session_id, turn_id}`
- `model.message{thread_id, message_id}`: the start marker, with no `message` field
- `model.message.delta{thread_id, message_id, content}`
- `model.message{thread_id, message_id, message:{content, finish_reason, usage}}`
- `turn.done{state: done|error, error?}`

The start marker and deltas are forwarded only. The other events are redacted, persisted, and then forwarded. The start marker is sent only after the provider accepts the request, so an HTTP error gives just `turn.created` then `turn.done`. `length` gives the final message, then `turn.done` with the error `max_tokens breached`. For now each turn mints a fresh root `thread_id`; 08 should make it the session's persisted root thread.

**Runner:** `TurnRunner` in `harness.rs` is the only writer to the store. It inserts the turn (seq is computed in SQL), then emits. `finish_turn` writes the state, error and usage before `turn.done` is forwarded, so a consumer that sees `turn.done` can read back final rows. The prompt is currently `[system: agent.instructions, user: input]`; ticket 09 replaces this with the instruction builder. There is no cross-turn context yet (08).

**Redaction:** `Redactor` replaces the key, in its JSON-escaped form, with `[REDACTED]` in the turn input, the error and every persisted event payload. Deltas are not redacted.

**DB schema and migrations:**
- `crates/harness-store/migrations/0001.sql` has only `session`, `turn` and `event`.
- To add tables (08: `thread`, `setting`; later: `mcp_oauth_token`, `checkpoint`), add `NNNN.sql` and append it to `MIGRATIONS` in `lib.rs`. `PRAGMA user_version` counts the migrations applied. Never edit a shipped migration.
- The database lives at `$XDG_DATA_HOME` or `~/.local/share` + `/banana-harness/harness.db`. Setting `XDG_DATA_HOME` redirects it, which is handy for manual runs.

**Fake provider (seam 1):**
- `crates/harness-core/tests/common/mod.rs`. Use it with `mod common;` in any core test file.
- `TestHarness::open(Provider).await` gives a harness whose base URLs point at a `FakeProvider`, with a temp workspace dir (not yet a git repo; 03 should `git init` it) and a temp DB.
- `t.provider.push_sse(fixture("x.sse"))` or `push_error(status, body)` queues replies, which are served FIFO. An empty queue answers 500.
- `t.provider.requests()` returns the recorded `{path, headers (lowercased), body: Value}`.
- `t.run_turn(input).await -> (session_id, events)`. `collect(handle)` has a 10 s timeout and asserts that the stream ends with `turn.done`. `t.reopen_store()` reads rows back.
- Fixtures live in `tests/fixtures/*.sse`, in each provider's real format: DeepSeek sends usage on the finish chunk plus a `: keep-alive` comment; Qwen sends usage in a final `choices: []` chunk plus null `function_call`/`refusal`/`audio` fields.
- The server writes one SSE event per write.

**Makefile** (repo root):
- `setup`: runs `check-cc` and `install-rust` (rustup `--profile minimal`, only when no cargo is found), then `cargo build --release --locked`.
- `run`: runs `rust/target/release/banana-harness`. `AI_API_KEY` and `WORKSPACE` are inherited from the environment or the make command line.
- `test`: `cargo test --workspace --locked`.
- `lint`: `fmt --check` plus clippy on all targets with `-D warnings`.
- `clean`.
- Every target runs `cd rust && $(CARGO) …` rather than `--manifest-path`, so that `rust-toolchain.toml` applies. rustup chooses the toolchain from the cwd.
- Ticket 14 adds the live-smoke hook to `test`.

**Verification:**
- `make setup && make test` passed in a fresh `ubuntu:24.04` container (aarch64) with no Rust: rustup was installed, the pinned 1.95 toolchain was auto-installed from `rust-toolchain.toml`, and all 4 seam-1 tests passed. With no compiler, `make setup` exits with the `apt install build-essential` fix; the macOS branch (`xcode-select --install`) was checked with a PATH that has no `cc`. On macOS, `make setup` passed on a machine that already had rustup. A fresh macOS user is still ticket 16.
- A bogus key against the real DeepSeek and Qwen (Beijing) hosts returns a clean `HTTP 401` turn error. This proves that TLS through ring and the platform verifier works.
- The real-key checks (`run --provider deepseek|qwen "say hi"`) are **unverified**: no `AI_API_KEY` was available. The live streaming chunk shapes are still unconfirmed, as the spec warns.
