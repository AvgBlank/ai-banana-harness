# 19: CI: the gui job on Linux and macOS

**Lane / milestone:** C · M5 First session   **Size:** S (one agent session)

**What to build:** Protect everything that follows: add the `gui` job (ticket 08 section 4) as soon as the first end-to-end test exists, and close the Linux assumptions.

**Blocked by:** 18

**Status:** open (not started)

**Files and components likely to change:**
- `.github/workflows/ci.yml` (the `gui` job; `workflow_dispatch`; a `concurrency` group)

**Acceptance criteria:**
- [ ] The `gui` job runs on `ubuntu-latest` and `macos-latest` with `REQUIRE_GUI=1`: setup-node from `gui/.node-version`, pnpm from `packageManager`, caches for pnpm and the Electron binary, `make gui-setup`, `make gui-lint`, `make gui-test`, `make gui-e2e` (`xvfb-run -a` on Linux).
- [ ] The Linux package names for Electron, the need for `--no-sandbox` under CI and the actions' versions are confirmed by a green run (closes those assumptions); any adjustment is recorded in `gui/AGENTS.md`.
- [ ] A step greps `gui/test-results` for the test key and fails if found; failure artifacts are uploaded only when it is absent; no secret is used.
- [ ] The existing `check` job is unchanged and still passes.

**Verification** (commands to run; none has been run by the planning):

```
gh workflow run ci.yml   # or push a branch
make lint && make test
```

**Documentation:** None.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
