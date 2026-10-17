# 27: Clean-checkout acceptance and documentation conformance

**Lane / milestone:** F · M7 Ship   **Size:** M (one agent session)

**What to build:** Prove the whole V1 from a clean checkout and confirm the repository documentation matches the approved drafts. No new behaviour.

**Blocked by:** 19, 25, 26

**Status:** open (not started)

**Files and components likely to change:**
- Documentation only, plus fixes for anything the run exposes
- `.scratch/desktop-gui/issues/` and `build/` statuses

**Acceptance criteria:**
- [ ] Ticket 08's acceptance sets A (Rust-only machine) and B (GUI machine) pass from a clean clone on macOS, exactly as written there, and CI is green on Linux and macOS.
- [ ] `CONTEXT.md`, `docs/adr/0003-desktop-app-surface.md`, `gui/AGENTS.md`, and the final `AGENTS.md`, `README.md` and `CONTRIBUTING.md` equal the drafts in `.scratch/desktop-gui/drafts/` once their staged hunks are all applied (a diff, with any deliberate difference explained in this ticket).
- [ ] Every item in the spec's assumption list is closed or recorded as a known limitation.
- [ ] No test hook exists in production code: `git grep` for `harness-testkit`, `BANANA_HARNESS_BIN` and `FAKE_PROVIDER_BIN` outside dev-dependencies, tests and the GUI's unpackaged paths finds nothing in product source.
- [ ] The key and the test key appear in no artifact; `make clean` leaves no build output.

**Verification** (commands to run; none has been run by the planning):

```
make setup && make test && make lint
make gui-setup && make gui-lint && make gui-test && make gui-e2e
make gui-package && make gui-package-smoke
diff CONTEXT.md .scratch/desktop-gui/drafts/CONTEXT.md
diff AGENTS.md .scratch/desktop-gui/drafts/AGENTS.md
diff gui/AGENTS.md .scratch/desktop-gui/drafts/gui-AGENTS.md
```

**Documentation:** Checks that every staged documentation hunk was applied by the ticket it was staged for.

**Notes:** The last ticket; when it is done the V1 plan is complete.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
