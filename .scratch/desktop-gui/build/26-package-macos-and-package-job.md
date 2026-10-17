# 26: macOS packaging and the package job

**Lane / milestone:** P · M7 Ship   **Size:** M (one agent session)

**What to build:** Unsigned arm64 DMG and ZIP with the sidecar inside the bundle (ticket 08 section 5), the packaged-app smoke test, and the CI `package` job.

**Blocked by:** 24

**Status:** open (not started)

**Files and components likely to change:**
- `gui/electron-builder.yml`, `gui/scripts/copy-sidecar.mjs`, `gui/build/icon.icns` (made from the approved banana mark)
- `Makefile` (`gui-package`, `gui-package-smoke`), `gui/tests/e2e/packaged.spec.ts`
- `.github/workflows/ci.yml` (the `package` job)

**Acceptance criteria:**
- [ ] `make gui-package` (macOS only) builds the release sidecar, stages it through `copy-sidecar.mjs` (refuses a missing or debug binary, mode 755) and produces `gui/dist/Banana Harness-<version>-arm64.dmg` and `...-arm64-mac.zip`; it exits non-zero with the macOS-only message elsewhere.
- [ ] The app contains `Contents/Resources/bin/banana-harness` with mode 755 that runs `--version`; `app.asar` lists no `node_modules`; `hdiutil verify` is valid; the version equals the crate version.
- [ ] `make gui-package-smoke` launches the packaged app through Playwright and expects the window and a sidecar `ready` frame.
- [ ] The `package` job runs on pushes to `main` and manually, needs `check` and `gui`, and uploads the unsigned DMG and ZIP.
- [ ] A downloaded copy (quarantine attribute set) is tried and the Gatekeeper behaviour and workaround are recorded in the README (closes that assumption); release sidecar size is recorded.
- [ ] Out of scope and absent: Windows, Linux packages, universal or Intel builds, signing, notarization, auto-update.

**Verification** (commands to run; none has been run by the planning):

```
make gui-package
make gui-package-smoke
make lint
make test
```

**Documentation:** Applies from the drafts: the `README.md` opening paragraph and the "Desktop app" section.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
