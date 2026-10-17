# 03: Core serde, provider options, rename, config paths, redactor

**Lane / milestone:** R · M1 Core additions   **Size:** M (one agent session)

**What to build:** The small core additions from tickets 10, 03, 05 and 06: serde on `Answer`, `UndoOutcome` and `Provider`; `ProviderOption` and `Harness::providers()` (with URL authority and scheme); `last_provider`; `Harness::rename_session`; shared config-path helpers; a public `Redactor`; and a test that two harnesses can share one database file.

**Blocked by:** 02

**Status:** done 2026-10-17. All seven acceptance criteria verified by tests that were run; `make lint` and `make test` passed locally (141 tests, 0 failed) with the build output in a temporary directory outside the repository. Not run: CI on Linux (macOS only here).

**Files and components likely to change:**
- `crates/harness-core/src/{harness.rs,config.rs,lib.rs}`
- `crates/harness-core/tests/{sessions.rs,approvals.rs}`
- `crates/harness-tui/src/lib.rs` (startup preselection uses `last_provider`; inline read deleted)
- `crates/banana-harness/src/main.rs` (headless run uses the shared config paths; inline code deleted)

**Acceptance criteria:**
- [x] JSON shapes are tested: `Answer` is `"allow"`, `"allow_for_session"`, `"deny"`, `{"text":"..."}`; `UndoOutcome` is `"restored"` / `"nothing_to_undo"`; `Provider` is `"deepseek"` / `"qwen"`.
- [x] `Harness::providers()` returns DeepSeek then Qwen as `{provider, model, host, scheme}` where `host` is the URL authority (host plus an explicit non-default port, never userinfo); a test with `https://user:pw@h:8443/v1` gives host `h:8443`, scheme `https`, and neither `user` nor `pw` appears anywhere in the output; a workspace-layer `base_url` override is reflected. (If URL parsing needs a crate, pin it once in `[workspace.dependencies]`; it is already in `Cargo.lock` as a transitive dependency.)
- [x] `last_provider(&Store)` returns what `switch_provider` last stored, and the TUI's startup preselection uses it.
- [x] `Harness::rename_session(id, title)` trims, rejects empty, caps at 50 characters, redacts the key, and a later turn does not overwrite the new title.
- [x] `user_config_path()` and `workspace_config_path(&Path)` are exported from `harness-core`; `banana-harness run` uses them; its behaviour is unchanged (existing tests pass).
- [x] `Redactor` is public (`new(api_key)`, `apply(&str)`), the private copy is deleted, and persisted-data redaction tests still pass.
- [x] Two `Harness` instances on one database file, each with its own `Store::open`, run turns concurrently without a locking error (seam-1 test).

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-core --locked
cargo test -p harness-tui --locked
cargo test -p banana-harness --locked
make lint
make test
```

**Documentation:** None.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.

**Completion record (2026-10-17):**
- **Changed:** `crates/harness-core/src/{config.rs,harness.rs,lib.rs,Cargo.toml}`, `crates/harness-tui/src/lib.rs`, `crates/banana-harness/{src/main.rs,Cargo.toml}`, root `Cargo.toml` and `Cargo.lock`, and tests in `crates/harness-core/tests/{sessions.rs,approvals.rs,safety.rs}` plus a new `config_paths.rs` (the ticket named the first two; redaction fits `safety.rs`, and the path helpers had no existing home).
- **Serde:** `Answer`, `UndoOutcome` and `Provider` derive `Serialize` and `Deserialize` on the existing types (no mirrors). Shapes: `"allow"`, `"allow_for_session"`, `"deny"`, `{"text": ...}`; `"restored"`, `"nothing_to_undo"`; `"deepseek"`, `"qwen"`. The wire form is strict (`"DeepSeek"` is rejected), unlike the command line's case-insensitive `FromStr`; a test pins both and that `as_str()` agrees with the JSON.
- **Provider options:** `ProviderOption { provider, model, host, scheme }` (in `config.rs`, derived from `ProviderConfig`) and `Harness::providers() -> Result<Vec<ProviderOption>>`, DeepSeek then Qwen. `host` is the URL authority: host plus an explicit non-default port (the scheme's default port is dropped), lowercased, IPv6 kept in brackets, never userinfo, path, query or fragment. Parsing uses the `url` crate, pinned once in `[workspace.dependencies]` (2.5.8, already in `Cargo.lock`). It returns `Result` because a `base_url` that is not a usable URL (or has no host) cannot be shown or checked; the error names the provider and never repeats the URL, which may carry credentials.
- **`last_provider(&Store) -> Option<Provider>`:** the TUI's startup preselection uses it and its inline read is deleted; a missing, unreadable or unknown stored value is `None`.
- **`Harness::rename_session(id, title)`:** uses ticket 02's `set_session_title`; errors with "not found" for an unknown session and "can't be empty" for an empty result. The title is redacted first, then trimmed and cut to 50 characters (characters, not bytes) by one shared `cut_title` that the automatic title now also uses. Redacting before cutting is deliberate: cutting first would leave the front of a key showing, and the test is sized so that order would fail it (verified by mutation).
- **Config paths:** `user_config_path()` and `workspace_config_path(&Path)` are exported; `banana-harness run` uses them; its inline copy is deleted, and so are `main.rs`'s now-unused `etcetera` and `APP_SLUG` imports and the binary crate's `etcetera` dependency (and its `Cargo.lock` entry).
- **`Redactor`:** the single existing definition is now `pub` with `new` and `apply` documented and exported; no second copy exists.
- **Concurrency:** two harnesses on one database file, each with its own `Store::open`, run three turns each concurrently on a multi-threaded runtime; every turn ends `done` and both sessions persist all turns.
- **Tests run:** `cargo fmt --check` (clean); the five affected core test files (`sessions` 19, `approvals` 7, `safety` 4, `config_paths` 2 passed); `cargo test -p harness-tui` (52 passed) and `-p banana-harness` (compiles; it has no tests); `make lint` (clean); `make test` (141 passed, 0 failed; was 122, so +19 new). The live smoke was skipped on purpose (`AI_API_KEY` unset). Mutation checks: userinfo in the destination fails its test; cut-before-redact fails its test after the test was strengthened (the first version was too weak and did not catch it).
- **Verification note:** the ticket's command `cargo test -p harness-core --locked` fails by itself in a fresh build directory, with five `mcp.rs` tests reporting that `harness-mcp`'s `mcp_fixture` example is missing. That is a pre-existing requirement ("run `cargo test --workspace` so harness-mcp's example is built"), not caused by this change; the same tests pass under `make test` / `cargo test --workspace`. Later tickets should verify core through the workspace command.
- **Not covered by a test:** `banana-harness run`'s use of the shared config paths. The expressions moved unchanged and the helpers are tested, but nothing exercises the headless run's config layering end to end; ticket 08's isolation test will.
- **Remaining risks:** rename does not normalise a title containing a newline (the interface is single-line, so the bridge or renderer should validate); `rename_session` and `providers` return plain `anyhow` errors, so the bridge, which must map "not found", "empty" and "bad URL" to protocol error codes, may need typed errors (the repository allows a `thiserror` enum where a caller branches); the check-then-write race in the first-turn title from ticket 02 is unchanged and still documented by its `ponytail:` comment.
- **Deviations from the ticket:** none.

