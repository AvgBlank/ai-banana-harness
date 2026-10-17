# 09: harness-bridge: framing, lifecycle, `serve --stdio`, session list

**Lane / milestone:** B · M3 Sidecar   **Size:** L (one agent session)

**What to build:** Create the `harness-bridge` crate and the `banana-harness serve --stdio` subcommand with the whole transport: NDJSON framing, the `ready` and `fatal` frames, errors, request correlation, the bounded writer, shutdown and the stdout guard. The only data method is `session.list`. Golden frames for what exists are checked in. Split point if over budget: transport (9a) then the subcommand and process test (9b).

**Blocked by:** 03, 08

**Status:** done

**Files and components likely to change:**
- `crates/harness-bridge/` (new; depends on `harness-core` only): `BridgeOptions`, `serve`, frame and error types, `BridgeError` enum, the writer task, a `log` helper (the only stderr writer)
- `crates/banana-harness/src/main.rs` and `Cargo.toml` (the `serve` subcommand with required `--stdio`, unix stdout-to-stderr guard using `libc`)
- `Cargo.toml` (`harness-bridge` workspace dependency and `default-members`)
- `crates/harness-bridge/tests/*.rs`, `crates/harness-bridge/tests/fixtures/*.ndjson`, `crates/banana-harness/tests/serve_stdio.rs`

**Acceptance criteria:**
- [x] The first frame is `ready` with `protocol: 1`, `app` from `APP_NAME`, the crate version and `api_key_present`; a startup failure emits `fatal` and exits 1.
- [x] Requests and responses correlate by id and may complete out of order; a duplicate in-flight id is `invalid_request`.
- [x] Malformed input never ends the connection: not JSON, invalid UTF-8, a line over 8 MiB (discarded to the next newline), a non-object, an unknown method and bad params each get an error response, with `id: null` where there is none.
- [x] `session.list` works with no workspace open, newest first, from a `Store`; `shutdown` replies then exits; stdin EOF and SIGTERM exit 0.
- [x] Backpressure: a reader that stalls delays frames without dropping or reordering them once it resumes.
- [x] stdout carries only protocol frames: `print_stdout` and `print_stderr` are denied in the crate outside `log`; a process test confirms valid NDJSON (the core `warning:` case is not reachable yet, see the completion record); the descriptor guard is tested by re-executing the test binary as a child that prints a stray byte.
- [x] Golden frames for `ready`, `fatal`, `session.list`, errors and `shutdown` are written by tests and checked in.
- [x] `make setup` builds the subcommand; the shipped binary still does not link the testkit.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-bridge --locked
cargo test -p banana-harness --test serve_stdio --locked
make lint
make test
```

**Documentation:** Applies from the drafts: `AGENTS.md` crate boundaries (`harness-bridge`, the binary's dependencies), the stdout bullet and the seam-3 wording; `CONTRIBUTING.md` layout line for `harness-bridge` and the `serve` mention.

**Notes:** Specification: ticket 03 sections 1 to 3, 7, 8 and 9.

**Completion record:**
- `crates/harness-bridge` (depends on `harness-core` only): `serve(reader, writer, options, shutdown)`, `BridgeOptions {db_path, api_key}`, `claim_stdout()`, the frame and `BridgeError` types, a bounded writer task (1024 frames), an 8 MiB line cap that discards to the next newline, in-flight id tracking, and the `log` helper as the only stderr writer. Methods: `session.list` and `shutdown`. `banana-harness serve --stdio` (`--stdio` required) claims stdout, reads `AI_API_KEY` without failing when it is absent, and exits 0 on stdin EOF, `shutdown`, `SIGTERM` or `SIGINT`.
- Golden frames (written and checked by tests, `BH_UPDATE_GOLDEN=1` rewrites): `crates/harness-bridge/tests/fixtures/{ready,fatal,session-list,errors,shutdown}.ndjson`.
- Deviations: (1) `serve` takes a fourth parameter, a shutdown future, so the binary can pass signals without the bridge installing signal handlers inside test processes; tests pass `std::future::pending()`. (2) `BridgeOptions` has no `user_config` yet; ticket 10 adds it with `workspace.open`, the first user. (3) The stdout guard (`claim_stdout`) lives in `harness-bridge`, not the binary, so the re-exec test can reach it; the binary calls it first. (4) The fatal code is `startup_failed`. (5) `BridgeError` has only the codes that exist so far (`invalid_request`, `unknown_method`, `invalid_params`, `internal`); ticket 10 adds the rest.
- Found and fixed by the process test: the stderr log line echoed a client-supplied method name unredacted; it now passes the `Redactor`.
- Not covered: out-of-order completion (only fast methods exist), a core `warning:` line on stderr (core writes none until a workspace is opened, ticket 10), a missing home directory (`default_db_path` fails before any frame, so no `fatal` is sent), Linux, `SIGINT` (only `SIGTERM` is tested).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
