# 12: Shared scenarios at the bridge and process seams

**Lane / milestone:** B · M3 Sidecar   **Size:** M (one agent session)

**What to build:** Run the shared scenario files through the in-process bridge (seam 3) and through the real `serve --stdio` process, so the sidecar is proven end to end before any GUI code depends on it. This is the Rust half of the milestone check.

**Blocked by:** 11, 08

**Status:** done

**Files and components likely to change:**
- `crates/harness-bridge/tests/scenarios.rs` (the runner and its verbs: send, approve, answer, cancel, release, wait_for)
- `crates/banana-harness/tests/serve_stdio.rs` (the same scenarios against the process, plus `kill_sidecar`)
- `crates/harness-testkit/scenarios/*.json` (refinements only)

**Acceptance criteria:**
- [x] Every scenario runs in-process and against the real process; normalized frame sequences equal `expect.events`; `expect.requests`, `request_contains` and `files` hold.
- [x] After `kill_sidecar` mid-turn a new process resumes the session and the interrupted turn has `state: null`.
- [x] The process test starts with `ready` within 10 s, shuts down with `shutdown` within 5 s, and stdout is only valid frames.
- [x] The test key is searched for in all captured frames and stderr and is not found.
- [x] The set of golden frames in `tests/fixtures` covers every method and event kind (checked by a test).

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-bridge --test scenarios --locked
cargo test -p banana-harness --test serve_stdio --locked
make lint
make test
```

**Documentation:** None.

**Notes:** Milestone check M3: `printf` a `workspace.open`, `session.create` and `turn.start` against `fake-provider` into `target/debug/banana-harness serve --stdio` and read the streamed answer.

**Completion record:**
- `crates/harness-bridge/tests/scenarios.rs` runs every bundled scenario in-process through the testkit driver (a loop over the scenario files, so a new file is picked up without edits), checks the key is in no received frame, and closes each bridge cleanly; only the process-only scenario is skipped, and the test names it. The shared `Run` fixture moved to `tests/common`, and the ten per-scenario tests in `turns.rs` became this loop.
- `crates/banana-harness/tests/serve_stdio.rs` runs all 11 scenarios against the real `serve --stdio`: `ready` within 10 s, a `shutdown` request answered and the process exited within 5 s, every stdout line a valid frame, the test key absent from all stdout and stderr. The new `kill-mid-turn` scenario kills the process with SIGKILL during a gated stream (`Transport::kill_and_restart`), starts a new one on the same data, reopens the workspace and resumes the session; the driver requires the interrupted turn's `state` to be null.
- `crates/harness-bridge/tests/coverage.rs` fails unless the checked-in golden frames contain every one of the 16 methods and 13 event kinds, and checks that every listed method exists on the server. `event-mcp-auth.ndjson` is built from core's own event type in the bridge's envelope, because an offline test cannot reach a remote OAuth server.
- Driver errors no longer repeat the scenario name; the callers name the scenario.
- Milestone check M3, run by hand with `fake-provider` and `banana-harness serve --stdio`: `ready`, `workspace.open`, `session.create`, `turn.start` streamed "Hello there!" and `turn.done` `done`; `shutdown` exited 0, the fake provider exited 0 on stdin close and removed its root.
- Not covered: the in-process transport cannot be killed, so `kill-mid-turn` runs only against the process; Linux.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
