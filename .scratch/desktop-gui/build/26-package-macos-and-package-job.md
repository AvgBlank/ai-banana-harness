# 26: macOS packaging and the package job

**Lane / milestone:** P · M7 Ship   **Size:** M (one agent session)

**What to build:** Unsigned arm64 DMG and ZIP with the sidecar inside the bundle (ticket 08 section 5), the packaged-app smoke test, and the CI `package` job.

**Blocked by:** 24

**Status:** built; the `package` job has not run on GitHub yet

**Files and components likely to change:**
- `gui/electron-builder.yml`, `gui/scripts/copy-sidecar.mjs`, `gui/build/icon.icns` (made from the approved banana mark)
- `Makefile` (`gui-package`, `gui-package-smoke`), `gui/tests/e2e/packaged.spec.ts`
- `.github/workflows/ci.yml` (the `package` job)

**Acceptance criteria:**
- [x] `make gui-package` (macOS only) builds the release sidecar, stages it through `copy-sidecar.mjs` (refuses a missing or debug binary, mode 755) and produces `gui/dist/Banana Harness-<version>-arm64.dmg` and `...-arm64-mac.zip`; it exits non-zero with the macOS-only message elsewhere.
- [x] The app contains `Contents/Resources/bin/banana-harness` with mode 755 that runs `--version`; `app.asar` lists no `node_modules`; `hdiutil verify` is valid; the version equals the crate version.
- [x] `make gui-package-smoke` launches the packaged app through Playwright and expects the window and a sidecar `ready` frame.
- [ ] The `package` job runs on pushes to `main` and manually, needs `check` and `gui`, and uploads the unsigned DMG and ZIP.
- [x] A downloaded copy (quarantine attribute set) is tried and the Gatekeeper behaviour and workaround are recorded in the README (closes that assumption); release sidecar size is recorded.
- [x] Out of scope and absent: Windows, Linux packages, universal or Intel builds, signing, notarization, auto-update.

**Verification** (commands to run; none has been run by the planning):

```
make gui-package
make gui-package-smoke
make lint
make test
```

**Documentation:** Applies from the drafts: the `README.md` opening paragraph and the "Desktop app" section.

**Completion record:**
- Build: `make gui-package` (macOS on arm64 only; other systems exit 1 with "only runs on macOS", Intel Macs with an arm64 message) builds the release sidecar with this machine's paths remapped out of it, stages it through `scripts/copy-sidecar.mjs` (refuses no path, a missing file, a folder or a path through `debug`; mode 755), runs the app build and electron-builder (`electron-builder` 26.15.3, `@electron/asar` 4.3.1, both exact devDependencies), then `scripts/verify-package.mjs`. Output: `gui/dist/Banana Harness-0.1.0-arm64.dmg` (137,365,048 bytes) and `Banana Harness-0.1.0-arm64-mac.zip` (137,378,021 bytes). `build/icon.icns` was made from the banana mark on the app's dark tile.
- Verified by `verify-package.mjs` on each build: `Contents/Resources/bin/banana-harness` exists with mode 755 and `--version` prints `banana-harness 0.1.0`; `app.asar` holds only `out/` and `package.json` (no `node_modules`); `hdiutil verify` is VALID; the version equals the crate's; the sidecar contains neither the home folder nor the repository path (a first build carried 846 `/Users/...` source paths, hence the remap). Release sidecar size: 22,271,792 bytes. Both the app and the sidecar are arm64 only.
- Smoke: `make gui-package-smoke` launches `Banana Harness.app` from `gui/dist` through Playwright with `BANANA_HARNESS_BIN` pointing nowhere: `app.isPackaged` is true, one window opens, the Open folder button shows, `session.list` goes through and main's state is `ready` (reached only on the sidecar's ready frame), and the test key is not in the app's output. It has its own config (`playwright.packaged.config.ts`); the main config ignores `packaged.spec.ts` because that test needs a build.
- Gatekeeper (tried on this Mac, Apple silicon): the ZIP was extracted, the quarantine attribute set on the app (`0081;...;Safari;`) and `open` run: the app did not start. After `xattr -d com.apple.quarantine "Banana Harness.app"`, `open` started it (and its sidecar). Recorded in the README. Not observed: the wording of the dialog (this session could not capture the screen), and the right-click Open and System Settings paths. `spctl --assess` and `codesign --verify --deep --strict` both say "code has no resources but signature indicates they must be present": the bundle carries only Electron's own ad-hoc signatures, which electron-builder's edits no longer seal. It runs as built; it is not a valid signature and nothing here claims one.
- CI: a `package` job in `ci.yml` on `macos-latest`, `needs: [check, gui]`, skipped on pull requests (so it runs on pushes to `main` and by hand), runs `make gui-setup`, `gui-package` and `gui-package-smoke`, then uploads the DMG and ZIP as `banana-harness-macos-arm64-unsigned` (14 days; the step fails if no file matches, and it comes after the build and smoke steps). It has not run on GitHub yet.
- Out of scope and absent (checked): Windows, Linux packages, universal or Intel builds, signing, notarization, auto-update.
- Deviations: (1) `verify-package.mjs` is an addition: it holds the acceptance checks so `make gui-package` fails by itself. (2) The `RUSTFLAGS` remap rebuilds the release binary on the first package build and leaves `target/release` holding that binary; it behaves the same. (3) `package.json` has no `author`, which electron-builder warns about.
- Not covered: running the DMG itself (the smoke test runs the unpacked app that went into it); a Mac with Intel; macOS versions other than this one.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
