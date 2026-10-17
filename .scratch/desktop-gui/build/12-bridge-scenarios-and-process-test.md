# 12: Shared scenarios at the bridge and process seams

**Lane / milestone:** B · M3 Sidecar   **Size:** M (one agent session)

**What to build:** Run the shared scenario files through the in-process bridge (seam 3) and through the real `serve --stdio` process, so the sidecar is proven end to end before any GUI code depends on it. This is the Rust half of the milestone check.

**Blocked by:** 11, 08

**Status:** open (not started)

**Files and components likely to change:**
- `crates/harness-bridge/tests/scenarios.rs` (the runner and its verbs: send, approve, answer, cancel, release, wait_for)
- `crates/banana-harness/tests/serve_stdio.rs` (the same scenarios against the process, plus `kill_sidecar`)
- `crates/harness-testkit/scenarios/*.json` (refinements only)

**Acceptance criteria:**
- [ ] Every scenario runs in-process and against the real process; normalized frame sequences equal `expect.events`; `expect.requests`, `request_contains` and `files` hold.
- [ ] After `kill_sidecar` mid-turn a new process resumes the session and the interrupted turn has `state: null`.
- [ ] The process test starts with `ready` within 10 s, shuts down with `shutdown` within 5 s, and stdout is only valid frames.
- [ ] The test key is searched for in all captured frames and stderr and is not found.
- [ ] The set of golden frames in `tests/fixtures` covers every method and event kind (checked by a test).

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-bridge --test scenarios --locked
cargo test -p banana-harness --test serve_stdio --locked
make lint
make test
```

**Documentation:** None.

**Notes:** Milestone check M3: `printf` a `workspace.open`, `session.create` and `turn.start` against `fake-provider` into `target/debug/banana-harness serve --stdio` and read the streamed answer.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
