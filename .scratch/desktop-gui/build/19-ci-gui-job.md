# 19: CI: the gui job on Linux and macOS

**Lane / milestone:** C · M5 First session   **Size:** S (one agent session)

**What to build:** Protect everything that follows: add the `gui` job (ticket 08 section 4) as soon as the first end-to-end test exists, and close the Linux assumptions.

**Blocked by:** 18

**Status:** implemented, not confirmed by a CI run

**Files and components likely to change:**
- `.github/workflows/ci.yml` (the `gui` job; `workflow_dispatch`; a `concurrency` group)

**Acceptance criteria:**
- [x] The `gui` job is written and runs on `ubuntu-latest` and `macos-latest` with `REQUIRE_GUI=1`: setup-node from `gui/.node-version`, pnpm from `packageManager`, caches for pnpm and the Electron binary, `make gui-setup`, `make gui-lint`, `make gui-test`, `make gui-e2e` (`xvfb-run -a` on Linux).
- [ ] (open: needs a green run) The Linux package names for Electron, the need for `--no-sandbox` under CI and the actions' versions are confirmed by a green run (closes those assumptions); any adjustment is recorded in `gui/AGENTS.md`.
- [x] A step greps `gui/test-results` for the test key and fails if found; failure artifacts are uploaded only when it is absent; no secret is used.
- [x] The existing `check` job is unchanged and still passes.

**Verification** (commands to run; none has been run by the planning):

```
gh workflow run ci.yml   # or push a branch
make lint && make test
```

**Documentation:** None.

**Completion record:**
- `.github/workflows/ci.yml`: `workflow_dispatch`, a `concurrency` group that cancels superseded runs on the same ref, and the `gui` job on `ubuntu-latest` and `macos-latest` (`REQUIRE_GUI=1`, `CI=1`, 30-minute limit): checkout, Rust toolchain and cache, pnpm from `gui/package.json`, Node from `gui/.node-version` with the pnpm cache, an Electron binary cache, the Linux libraries and `xvfb` (with a `libasound2` fallback for older images), then `make gui-setup`, `gui-lint`, `gui-test`, `gui-e2e`; a step that fails the job if the test key is anywhere under `gui/test-results`, and an upload of that folder only on failure and only when the key is absent. No secret is used. The `check` job is unchanged. The `package` job from the plan is not added: its targets (`gui-package`, `gui-package-smoke`) do not exist yet.
- Checked here: the YAML parses and has the expected jobs, steps, env and concurrency (Ruby's YAML loader; no `actionlint` is installed); the leak step's grep logic was run against a folder with and without the key; on macOS, `CI=1 REQUIRE_GUI=1 make gui-e2e` (20 passed), `make gui-lint` and `make gui-test` pass; `make lint` and `make test` pass.
- **Not confirmed:** nothing has run on GitHub Actions, so the Linux package names, whether `--no-sandbox` is enough under CI, the action versions and the Electron cache paths are still assumptions, and `gui/AGENTS.md` records no CI adjustment. The first push or `gh workflow run ci.yml` is the check, and any fix it needs belongs in this file's checklist.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
