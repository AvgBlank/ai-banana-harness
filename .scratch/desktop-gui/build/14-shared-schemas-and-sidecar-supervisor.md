# 14: Shared protocol schemas and the sidecar supervisor

**Lane / milestone:** G · M4 GUI foundation   **Size:** L (one agent session)

**What to build:** The contract layer and the Electron main process's sidecar management (ticket 06 section 1), tested with a scripted fake child. Split point if over budget: schemas and contract tests (14a) then the supervisor (14b).

**Blocked by:** 13, 09

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/shared/{protocol.ts,ipc.ts}` (zod schemas for frames, methods, results, events and the amendments; IPC channel names and argument schemas)
- `gui/src/main/sidecar/{supervisor.ts,framing.ts,env.ts,client.ts}`
- `gui/tests/unit/shared/contract.test.ts`, `gui/tests/unit/main/supervisor.test.ts`

**Acceptance criteria:**
- [ ] Contract test: every golden frame under `crates/harness-bridge/tests/fixtures/*.ndjson` parses with the schemas, and every method and event kind the schemas define has a fixture.
- [ ] Supervisor with a fake child: `ready` within 10 s moves to `ready`; no `ready` gives `no_ready`; a different protocol gives `incompatible_protocol` (no restart); `fatal` and early exit give `fatal` and `exited_early` with redacted stderr; `api_key_present: false` gives `needs_key`.
- [ ] Framing: oversized, non-UTF-8 and non-JSON lines are dropped and counted; 20 consecutive invalid lines kill and restart; an unknown response id and a late response after a timeout are ignored; ids are main-generated; stdin writes honour backpressure; timeouts are 30 s (60 s for open, resume, undo and changes).
- [ ] Crash handling: pending requests reject with `sidecar_exited`; restarts at 0.5, 1 and 2 s, at most 3 in 60 s, then `failed` with a Restart; a 60 s stable run resets the budget; workspaces are reopened one at a time after a restart.
- [ ] Shutdown sends `shutdown` (4 s), closes stdin, waits 5 s, then SIGTERM, then SIGKILL after 2 s; the single-instance lock focuses the first window.
- [ ] The sidecar environment is an explicit allowlist (no `ELECTRON_*`, no `NODE_OPTIONS`); the key is only in that environment and never in argv or logs.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
```

**Documentation:** None.

**Notes:** Specification: ticket 06 section 1 and ticket 03 section 2.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
