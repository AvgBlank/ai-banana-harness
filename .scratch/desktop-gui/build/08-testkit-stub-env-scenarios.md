# 08: harness-testkit: control API, TestEnv, scenarios, fake-provider binary

**Lane / milestone:** K · M2 Test foundation   **Size:** L (one agent session)

**What to build:** Complete the test foundation from ticket 05: the stub's control API, gates, failure replies, watchdog and leftover check; `TestEnv`; the `Scenario` type with the scenario and workspace files; the `fake-provider serve` binary; and a permanent version of the isolation run that was verified by hand. Split point if over budget: stub features (8a) then `TestEnv`, scenarios and the binary (8b).

**Blocked by:** 07

**Status:** done

**Files and components likely to change:**
- `crates/harness-testkit/src/{stub.rs,control.rs,env.rs,scenario.rs}` and `src/bin/fake-provider.rs`
- `crates/harness-testkit/{scenarios/*.json,workspaces/hello-repo/,tests/*.rs}`
- `crates/banana-harness/tests/isolation.rs` (new, uses the testkit as a dev-dependency)

**Acceptance criteria:**
- [x] Stub: incremental SSE, body-substring matchers, queue order, an empty queue answers 500 and is recorded as unmatched, gates released through `POST /__fake/release/<gate>`, `abort_after_parts`, `hang` plus a 30 s watchdog, the control API (`replies`, `requests`, `status`, `reset`, `health`), the authorization header recorded as `[redacted]` with an `auth_ok` flag, the leftover check, and two stubs at once.
- [x] `TestEnv`: an empty environment, `HOME` and `XDG_*` in a temp root, `NO_PROXY=127.0.0.1,localhost` plus a dead outbound proxy, the fixed test key, a copied and templated workspace (`{{stub:name}}`, `{{workspace}}`, `{{root}}`, `{{marker}}`) with `git init`, a local identity and one commit, returned as the canonical path; `without_key()` and `shell(path)` variants.
- [x] `Scenario` (serde) and the scenario files: `plain-answer`, `edit-and-approve`, `bash-and-test`, `question-and-answer`, `subagents`, `slow-stream-gated`, `provider-error-401`, `provider-abort-midstream`, `key-never-leaks`, `edit-and-undo`; a testkit test parses every file.
- [x] `fake-provider serve --scenario <file> [--stub name ...]` prints one JSON line (`root`, `workspace`, `env`, `stubs`), serves until stdin closes or SIGTERM, and removes its root unless `BH_TEST_KEEP=1`.
- [x] Isolation tests: a run under the poisoned-proxy environment reaches the stub; a non-loopback request fails fast; the database lands under the temp `XDG_DATA_HOME` and the real data and config directories are unchanged (before/after listing); the host's `AI_API_KEY` is absent from the child environment; the real `banana-harness run --json` against the stub reproduces the event sequence verified in ticket 05 (`turn.created`, start marker, two deltas, final message, `turn.done`).
- [x] Cleanup removes the root; stdin EOF exits the binary; no fixed ports are used.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-testkit --locked
cargo test -p banana-harness --test isolation --locked
make lint
make test
```

**Documentation:** None.

**Notes:** Ticket 05 sections C to F are the specification.

**Completion record:**
- `harness-testkit` now has `stub.rs` (`FakeProvider`, `ReplySpec`, gates, `abort_after_parts`, `hang`, 30 s watchdog configurable through `StubOptions`, `Status`, `Leftover`/`assert_drained`, authorization recorded as `[redacted]` with `auth_ok`), `control.rs` (`/__fake/{health,status,requests,replies,reset,release/<gate>}`), `env.rs` (`TestEnv`, `TestEnvOptions`, `KeySetup`, `without_key()`, `shell()`), `scenario.rs` (`Scenario`, `Step`, `Expect`; `script()` queues a scenario's replies on a stub), `src/bin/fake-provider.rs`, the 10 `scenarios/*.json` and `workspaces/hello-repo/`.
- Isolation tests are `crates/banana-harness/tests/isolation.rs` (the real `banana-harness run --json` against an in-process stub; the testkit's `fake-provider` binary is covered by `crates/harness-testkit/tests/process.rs`). The plain-answer scenario's `expect.events` is asserted against the real binary's event sequence.
- Deviations: (1) `Step` gains an `undo` verb, which `edit-and-undo` needs and the ticket-05 verb list lacked. (2) `Request` gains `auth_ok` and `unmatched`; the core `plain_answer` test now asserts `[redacted]` plus `auth_ok` instead of the raw header. (3) A gate sits at a part index (`gate_after_parts`, default 0) rather than on a named part. (4) `expect.events` is filled only for `plain-answer`, the one scenario whose sequence was run against the real binary; the others carry `requests`, `request_contains`, `files` and `ui`, which are not yet run by any driver (no bridge or Electron driver exists). (5) `fake-provider` exposes a single `serve` command; `TestEnv::with_key(KeySetup)` is an extra convenience over `without_key()`/`shell()`.
- Not verified: Linux, `SIGTERM` handling beyond macOS, the other nine scenarios running end to end, the real `~/.local/share` change check under a deliberately leaking mutation (it would write to the real directory).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
