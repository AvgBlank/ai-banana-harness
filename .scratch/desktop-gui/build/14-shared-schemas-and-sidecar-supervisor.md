# 14: Shared protocol schemas and the sidecar supervisor

**Lane / milestone:** G · M4 GUI foundation   **Size:** L (one agent session)

**What to build:** The contract layer and the Electron main process's sidecar management (ticket 06 section 1), tested with a scripted fake child. Split point if over budget: schemas and contract tests (14a) then the supervisor (14b).

**Blocked by:** 13, 09

**Status:** done

**Files and components likely to change:**
- `gui/src/shared/{protocol.ts,ipc.ts}` (zod schemas for frames, methods, results, events and the amendments; IPC channel names and argument schemas)
- `gui/src/main/sidecar/{supervisor.ts,framing.ts,env.ts,client.ts}`
- `gui/tests/unit/shared/contract.test.ts`, `gui/tests/unit/main/supervisor.test.ts`

**Acceptance criteria:**
- [x] Contract test: every golden frame under `crates/harness-bridge/tests/fixtures/*.ndjson` parses with the schemas, and every method and event kind the schemas define has a fixture.
- [x] Supervisor with a fake child: `ready` within 10 s moves to `ready`; no `ready` gives `no_ready`; a different protocol gives `incompatible_protocol` (no restart); `fatal` and early exit give `fatal` and `exited_early` with redacted stderr; `api_key_present: false` gives `needs_key`.
- [x] Framing: oversized, non-UTF-8 and non-JSON lines are dropped and counted; 20 consecutive invalid lines kill and restart; an unknown response id and a late response after a timeout are ignored; ids are main-generated; stdin writes honour backpressure; timeouts are 30 s (60 s for open, resume, undo and changes).
- [x] Crash handling: pending requests reject with `sidecar_exited`; restarts at 0.5, 1 and 2 s, at most 3 in 60 s, then `failed` with a Restart; a 60 s stable run resets the budget; workspaces are reopened one at a time after a restart.
- [x] Shutdown sends `shutdown` (4 s), closes stdin, waits 5 s, then SIGTERM, then SIGKILL after 2 s; the single-instance lock focuses the first window.
- [x] The sidecar environment is an explicit allowlist (no `ELECTRON_*`, no `NODE_OPTIONS`); the key is only in that environment and never in argv or logs.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
```

**Documentation:** None.

**Notes:** Specification: ticket 06 section 1 and ticket 03 section 2.

**Completion record:**
- `gui/src/shared/protocol.ts` holds zod schemas for every frame, all 16 methods (params and result) and all 13 event kinds, plus the amendments (`workspace_config.path`, provider `host` and `scheme`, undo `event`, `changes.*` shapes, `active_turn`). `gui/src/shared/ipc.ts` holds the channel names, the strict argument schemas and `parseRpcCall` (allowlist, per-method params, 1 MiB input cap). `zod` 4.6.7 was added as an exact devDependency.
- `gui/src/main/sidecar/`: `framing.ts` (line splitter with the 8 MiB cap and UTF-8 check, frame parser), `env.ts` (allowlisted environment, `redact`), `client.ts` (main-generated ids, 30 s and 60 s timeouts, stdin writer that waits for `drain`, `RpcError`), `supervisor.ts` (the state machine). `main/index.ts` now takes the single-instance lock (a second launch focuses the first window), starts the supervisor with the real `child_process.spawn`, and stops it on `before-quit` (a second quit forces the kill path).
- Tests (40 in `gui/tests/unit`, 35 of them new): the contract test checks every golden frame (all five frame kinds, request params and results against their method's schema, every method and event kind has a fixture); the supervisor test drives a scripted fake child through the handshake outcomes, framing, crash and restart policy, workspace reopening, the shutdown escalation on fake timers and the secrets rules. Mutations that were caught: the invalid-line limit, the restart delays, the fatal redaction, the SIGKILL wait, the undo timeout and the 8 MiB cap (the last two only after the tests were tightened).
- Checked for real: the built app with the debug sidecar reached `sidecar: ready`, a second launch exited without a second sidecar, quitting through the DevTools `Browser.close` logged `stopping` then `stopped` and left no process, and killing the main process with SIGKILL made the sidecar exit by itself; the key appeared in no log.
- Deviations: (1) the supervisor takes injected `spawn`, `environment` and `reopenWorkspace` functions; reopening calls the hook one workspace at a time, and the restricted-first trust flow that supplies it is a later ticket. (2) Clearing live-turn buffers on a crash and the quit confirmation for a running turn depend on state and dialogs that do not exist yet. (3) The key comes from this process's environment only; the login-shell probe and in-memory entry are the key flow's. (4) `restart_budget` is the reason for the failed state after three restarts in 60 s. (5) The IPC schemas are defined and tested but no handler or preload API uses them yet. (6) `tsconfig.node.json` now type-checks the main and shared unit tests.
- Not covered: Linux; a hung sidecar (the protocol has no ping); the renderer showing any of this state.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
