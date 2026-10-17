# Repo layout, Makefile targets, CI and packaging

Type: grilling
Status: resolved
Blocked by:

## Question

Fix the `gui/` directory layout, the Makefile targets (setup, dev, build, package, lint, test, clean) and how `make setup` / `make lint` / `make test` incorporate the JS side without breaking the Rust-only evaluation path, the CI matrix for Linux and macOS (headless Electron on Linux), how the sidecar binary is bundled into the macOS app, and the unsigned-build story.

Inputs from ticket 05 (2026-10-17): add `harness-testkit` and a workspace `default-members` (product crates only, so `make setup` stays lean) to the layout; define the make targets that build and run the e2e suite (`cargo build -p banana-harness -p harness-testkit`, then the Playwright run with `BANANA_HARNESS_BIN` and `FAKE_PROVIDER_BIN`); the CI matrix needs `xvfb-run` on Linux and artifact upload of `test-results/` with a grep for the test key; and the `test` target must export `NO_PROXY=127.0.0.1,localhost` (the offline suite fails behind a proxy today). See [ticket 05](05-fake-provider-for-smoke-tests.md#g-commands-and-ci-proposed-makefile-wiring-is-ticket-8s).

## Answer

Resolved 2026-10-17 against the repository (`Makefile`, `.github/workflows/ci.yml`, `rust-toolchain.toml`, `Cargo.toml`, `.gitignore`, `AGENTS.md`) and the resolved tickets 00 to 07 and 10. **Nothing is implemented**: no `gui/`, no Rust, no Makefile or workflow change. Every claim is labelled **Verified** (observed this session) or **Proposed/Assumed**. Scratch work was done in temporary directories and removed; the repository was not touched.

### Your decisions applied

- `harness-testkit` is approved as development/test-only support with one canonical owner for the fake provider. `AGENTS.md` is updated in the same change (listed in section 1); no test hook enters production code.
- `make test` includes the `NO_PROXY` fix, keeping the user's existing exclusions and always adding `127.0.0.1` and `localhost`.
- Ticket 06's provider rule stands: validate scheme and authority including the port, and allow non-HTTPS endpoints only for loopback testing (ticket 05's amendment is the precise form).

### A. What was verified, and what is still an assumption

**Verified this session** (macOS arm64, Node 24.22.0 fetched through `npx` for the smoke, pnpm 10.29.2, Rust 1.95, GNU Make 3.81):

1. **Node range.** Registry `engines`: vitest 5.0.5 `^22.12.0 || ^24.0.0 || >=26.0.0`, jsdom 30.1.3 `^22.22.2 || ^24.15.0 || >=26.0.0`, electron 44.8.0 `>= 22.12.0`, electron-vite 6.0.0 and vite 8.3.4 `>=22.12`, Playwright 1.64.1 `>=20`. So **Node 25 is outside Vitest's supported range**. This machine's default `node` is 25.9.0 (nvm is installed; Homebrew's own `node` 25.8.1 is broken here, a missing `libsimdjson` dylib), so the GUI must pin Node 24.
2. **ESLint cannot be used.** `typescript-eslint` 8.71.2 has peer `typescript >=4.8.4 <6.1.0`, which excludes TypeScript 7.0.2. **Biome 2.5.15** has its own parser: run on the approved prototype it parsed every one of the 35 `.ts`/`.tsx` files; its only parse error was `src/styles/index.css` (Tailwind's `@theme`, which needs Biome's `tailwindDirectives` CSS option; the option itself was not applied or re-run). It reported 40 real accessibility findings, none visual (28 buttons missing `type`, 3 ARIA-role/prop misuses, 3 `useSemanticElements`, 2 labels without a control, and one each for focusable-interactive, aria-props-for-role, autofocus).
3. **TypeScript 7.** `tsc` rejects `baseUrl` (TS5102, seen earlier); `"paths": { "@/*": ["./src/*"] }` with no `baseUrl` typechecks (also confirmed in an Electron-shaped project).
4. **Install.** With `engine-strict=true` and exact pins (electron 44.8.0, electron-builder 26.15.3, electron-vite 6.0.0, vite 8.3.4, typescript 7.0.2, @playwright/test 1.64.1, zod 4.6.7) `pnpm install` completed in about 5 s under Node 24.22.0. `electron-winstaller`'s build script is reported as ignored, which is harmless (Windows only).
5. **Electron 44 has no `postinstall` script.** `pnpm install` leaves no binary; `node_modules/electron/index.js` downloads lazily on first launch, and the package's `install-electron` bin (`node install.js`) fetches it (about 8 s). This corrects ticket 02, which assumed `onlyBuiltDependencies` was needed for Electron: **it is not**, and CI/Make must fetch the binary explicitly for determinism and caching.
6. **Build and preload.** `electron-vite build` (v6 on Vite 8.3.4) produced main, preload and renderer. The preload output is **CommonJS** (`require("electron")`) as long as `package.json` has no `"type": "module"`, which resolves ticket 02's "sandboxed preload must be CJS" question for this configuration.
7. **pnpm layout is a non-issue if every dependency is a `devDependency`.** With `zod` in `devDependencies` and imported by the main process, electron-vite bundled it into `out/main/index.js` (135 KB, no runtime `require("zod")`); the packaged `app.asar` contained **0** `node_modules` entries, and `zod` validated correctly inside the packaged app. So `node-linker=hoisted` is not needed, and the packaged app ships no `node_modules`.
8. **Unsigned packaging.** `electron-builder` 26.15.3 with `mac.identity: null` built `dir`, then `dmg` and `zip` for arm64 in about 27 s: `<productName>-<version>-arm64.dmg` and `<productName>-<version>-arm64-mac.zip`, about 139 MB each (with a 63 MB debug sidecar inside). `hdiutil verify` reported the DMG checksum valid.
9. **Sidecar bundling.** With `extraResources: [{ from: build/sidecar, to: bin }]` the real `banana-harness` arm64 binary landed at `Contents/Resources/bin/banana-harness` with mode 755 and ran (`banana-harness 0.1.0`) from inside the bundle and from an extracted ZIP (the ZIP preserved the executable bit). The cargo-built arm64 binary is already ad-hoc linker-signed (`flags=0x20002(adhoc,linker-signed)`), so no extra signing was needed for it to run.
10. **The packaged unsigned `.app` launches** (about 0.7 s), `app.getName()` follows `productName`, `--user-data-dir` is honoured, and **Playwright 1.64.1 `_electron` drives both the packaged app and the unpackaged build** (`args: ["out/main/index.js"]`) against Electron 44.8.0 (ticket 02 listed this as unverified). Inside the renderer `window.banana` was the frozen object from the preload and `typeof require` and `typeof process` were `undefined`, confirming sandbox and context isolation work in this setup. (`codesign --verify --deep --strict` reports the unsigned bundle's resources as inconsistent, as expected; it still launched.)
11. **`NO_PROXY` expression**, tested against the real offline suite with all six proxy variables poisoned to a dead proxy: no exclusions fails (4 of 4), the user's own exclusions only (no loopback) fails (4 of 4), the Makefile expression passes when the user has exclusions with a space (`corp.example.com, .internal.test`), when only lowercase `no_proxy` is set, and when nothing is set (4 of 4 each). (A first attempt was invalid because zsh does not word-split an unquoted variable; its control case passed when it should not have, so it was discarded and redone with an array.)
12. **GNU Make 3.81 constructs** (the version macOS ships), run in a scratch Makefile: the `$${NO_PROXY:+$$NO_PROXY,}127.0.0.1,localhost` expansion, a conditional sub-make that **skips GUI checks and exits 0** when `gui/node_modules` is absent, fails when `REQUIRE_GUI=1` and dependencies are missing, and runs when dependencies exist.

**Assumed or not yet exercised (needs a smoke in the build run):**

- `engine-strict` rejecting Node 25 (expected from pnpm's behaviour; my negative test used the broken Homebrew `node`, so it was not observed).
- Cargo `default-members` behaviour and the new crates (`harness-bridge`, `harness-testkit` do not exist yet).
- Linux CI: apt package names for Electron, `xvfb-run`, and whether Electron needs `--no-sandbox` under CI; macOS CI runner details; `pnpm/action-setup`, `setup-node` and artifact action versions.
- Gatekeeper behaviour for a **downloaded** unsigned app (a locally built one is not quarantined) and the `xattr` workaround.
- A release (not debug) sidecar size, universal or x64 builds, Linux packages, an app icon, Vitest 5 + jsdom 30 + Testing Library 16 running on Node 24 (engines are compatible; not executed), and Biome's Tailwind CSS option.

### 1. Final layout

```
Cargo.toml                       workspace; members = ["crates/*"]; NEW default-members (product crates only)
Makefile                         existing targets + gui-* targets (section 3)
rust-toolchain.toml              unchanged (1.95)
crates/
  banana-harness/                binary: TUI, `run`, NEW `serve --stdio`   (depends on core, tui, bridge)
  harness-core/  harness-llm/  harness-tools/  harness-mcp/  harness-store/  harness-tui/     unchanged roles
  harness-bridge/                NEW library: protocol + dispatcher (ticket 03); depends on core only
  harness-testkit/               NEW test-support: FakeProvider, fixtures, TestEnv, scenarios, `fake-provider` bin;
                                 depends on no product crate; dev-dependency only (ticket 05)
    scenarios/*.json  workspaces/<name>/  fixtures/*.sse
gui/                             NEW production desktop app (TypeScript); standalone pnpm project
  AGENTS.md  CLAUDE.md           GUI rules; CLAUDE.md contains only `@AGENTS.md` (AGENTS.md rule)
  package.json  pnpm-lock.yaml   packageManager pin, exact versions, committed lockfile
  .npmrc (engine-strict=true)  .node-version (24)  .gitignore
  electron.vite.config.ts  electron-builder.yml  biome.json  vitest.config.ts  playwright.config.ts
  tsconfig.json  tsconfig.node.json  tsconfig.web.json
  src/main/                      Electron main: window, sidecar supervisor, trust, prefs, key resolver, ipc  (ticket 06)
  src/preload/index.ts           the narrow `window.banana` surface
  src/renderer/                  index.html, main.tsx, App.tsx, components/, state/ (reducer, ticket 07), styles/ (FROZEN tokens.css copy)
  src/shared/                    protocol.ts (zod, ticket 03/06), ipc.ts
  tests/unit/  tests/e2e/        Vitest (main, state, components) and Playwright (scenarios, ticket 05)
  scripts/                       check-tokens.mjs (ported), copy-sidecar.mjs, check-versions.mjs
  build/                         icon.icns (to be made from the approved banana mark); sidecar/ is generated and git-ignored
  out/  dist/  test-results/     generated and git-ignored
.scratch/desktop-gui/            the map, tickets, research
  prototype/                     FROZEN visual reference, kept where it is
docs/adr/0003-...md              written by ticket 09
```

- **The prototype stays at `.scratch/desktop-gui/prototype/`**, an independent pnpm project with its own lockfile. It is not part of `gui/`, not wired into any Makefile target, and not a pnpm workspace member (no root-level JS tooling is added, so the Rust root stays clean). Tickets and the map link to it by that path, so moving it would break them. Build-run housekeeping: keep the 1440x900 screenshots committed and add `screenshots/1100x720/` to the prototype's own `.gitignore` (regenerable with `pnpm shots 1100x720`), roughly halving the committed images; `node_modules` and `dist` are already ignored there.
- **Tokens:** `gui/src/renderer/styles/tokens.css` is a copy of the frozen file and becomes the production owner; `gui/scripts/check-tokens.mjs` (ported unchanged) keeps light/system parity and the AA pairs enforced. The prototype's file stays as the reference and is not cross-compared.
- **Components:** carried over from the prototype by copy, with the 40 Biome findings fixed (no visual change).
- **Rust workspace:** `members = ["crates/*"]` already picks up the two new crates. Add `harness-bridge` and `harness-testkit` to `[workspace.dependencies]` as path entries and `default-members` listing `banana-harness`, `harness-core`, `harness-tui`, `harness-bridge`, `harness-llm`, `harness-tools`, `harness-mcp`, `harness-store`, so `make setup` (`cargo build --release --locked`) builds the product crates only and never the testkit. `cargo test --workspace` and `cargo clippy --workspace` (what `make test` and `make lint` run) still cover everything.
- **`AGENTS.md` edits (same change as the code):** the crate-boundary rule gains `harness-bridge` (depends on core only; the binary depends on core, tui and bridge) and `harness-testkit` (test-support; a `[dev-dependencies]` entry only, depends on no product crate, never linked into the shipped binary); a short GUI rule block (TypeScript lives in `gui/`, exact-pinned JS dependencies and a committed `pnpm-lock.yaml` installed with `--frozen-lockfile`, tests in `gui/tests/`, the renderer's design tokens are frozen, JS comment style follows the Rust comment rules); `gui/AGENTS.md` with the GUI-specific rules and its sibling `gui/CLAUDE.md`; `README.md` gains a "Desktop app" section and the e2e instructions. User-visible behaviour (key bindings, slash commands, guardrails) must stay in step across the TUI, `/help` and `README.md`, and now also the GUI's Settings shortcuts list.

### 2. JS toolchain and lockfile (all exact, no carets)

- **Node 24 LTS.** `gui/.node-version` is `24`; `engines.node` is `^24.15.0` and `gui/.npmrc` sets `engine-strict=true`. The default Node 25.9 on this machine is not acceptable: use `nvm install 24 && nvm use` (nvm is present) or any Node 24 manager. `make gui-setup` checks `node -v` and fails with that instruction.
- **pnpm 10.29.2**, pinned in `gui/package.json` as `"packageManager": "pnpm@10.29.2"` (the version that produced and verified the prototype's lockfile; pnpm 10 downloads the pinned version itself when `manage-package-manager-versions` is on, its default). `pnpm-lock.yaml` is committed and every install uses `pnpm install --frozen-lockfile`, the analogue of Cargo's `--locked`. A lockfile update is a deliberate `pnpm update` commit (Dependabot was dropped from this repository).
- **Pins** (from tickets 02 and the verified build; re-run `npm view` before the first commit and freeze via the lockfile): electron 44.8.0, electron-vite 6.0.0, electron-builder 26.15.3, vite 8.3.4, @vitejs/plugin-react 6.1.2, react and react-dom 19.3.1, typescript 7.0.2, tailwindcss and @tailwindcss/vite 4.3.3, vitest 5.0.5, jsdom 30.1.3, @testing-library/react 16.3.3 (+ @testing-library/dom 10, jest-dom latest), @playwright/test 1.64.1, @biomejs/biome 2.5.15, zod 4.6.7, a TOML parser for the trust summary (`smol-toml`, pin at install), lucide-react 1.58.0, class-variance-authority 0.7.1, clsx 2.1.1, tailwind-merge 3.7.0, the Radix packages the prototype uses (dialog, popover, checkbox, tooltip), `vite-plugin-singlefile` not needed in `gui/`. `motion` and Monaco stay out (tickets 04, 07).
- **Every dependency, including React and zod, is a `devDependency`; `dependencies` is empty.** Verified: electron-vite bundles dev dependencies into main, preload and renderer, and the packaged app contains no `node_modules`. If a package must stay external (a native module), that is a deliberate exception recorded in `gui/AGENTS.md`.
- **No `postinstall` reliance.** `gui-setup` runs `pnpm exec install-electron` after the frozen install to fetch the Electron binary (verified bin). `electron-builder` fetches its own Electron zip and the DMG tooling into the user cache on first packaging. `pnpm.onlyBuiltDependencies` stays empty; `electron-winstaller`'s ignored script is left unapproved on purpose.
- **TypeScript 7:** no `baseUrl` anywhere; `paths` use `./src/*`. Components are written by hand in shadcn's style as the prototype does, so the shadcn CLI (whose docs still show `baseUrl`) is not part of the toolchain.
- **Lint and format: Biome 2.5.15** (one tool for both), with the Tailwind CSS parser option on, the a11y rules at their recommended level (they found real defects in the prototype), and `tsc --noEmit` for types over `tsconfig.node.json` (main, preload, shared, tests) and `tsconfig.web.json` (renderer). This replaces the ESLint plan, because `typescript-eslint` cannot be used with TypeScript 7. If the team prefers ESLint later, the fallback is the TypeScript 6.x line that ticket 02 named, at the cost of the TS 7 choice.
- **Version single owner:** `gui/package.json`'s `version` is the app version; `gui/scripts/check-versions.mjs` (run by `gui-lint`) asserts it equals the `version` in `crates/banana-harness/Cargo.toml`.
- **Product metadata (ADR 0003 exception you approved):** `productName` ("Banana Harness"), `appId` and the window title live in `gui/package.json` and `gui/electron-builder.yml` only. `appId` needs your value, see the questions.

### 3. Makefile (Proposed; the `$$` expansion and the conditional sub-make were checked in a scratch Makefile on GNU Make 3.81; `$(abspath)`, `$(if)` and `$(shell)` are standard 3.81 functions but were not exercised)

The existing targets keep their meaning. The Rust-only path (`make setup`, `make run`, `make test`, `make lint`) needs no Node and never fails because Node is missing.

```make
GUI := gui
PNPM ?= pnpm
SIDECAR_DEBUG := $(abspath target/debug/banana-harness)
# Loopback must bypass any proxy; the user's own exclusions are kept (checked against the real suite).
TEST_ENV = NO_PROXY="$${NO_PROXY:+$$NO_PROXY,}127.0.0.1,localhost" no_proxy="$${no_proxy:+$$no_proxy,}127.0.0.1,localhost"

.PHONY: setup run test lint clean check-cc install-rust \
        gui-setup gui-sidecar gui-dev gui-build gui-package gui-lint gui-test gui-e2e gui-package-smoke gui-clean gui-maybe-lint gui-maybe-test

test:
	$(TEST_ENV) $(CARGO) test --workspace --locked
	@# ...the existing live-smoke block, unchanged...
	@$(MAKE) --no-print-directory gui-maybe-test

lint:
	$(CARGO) fmt --check
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	@$(MAKE) --no-print-directory gui-maybe-lint

clean: gui-clean
	$(CARGO) clean

# GUI checks run inside `make lint` / `make test` only when the user has opted in (gui/node_modules exists)
# or REQUIRE_GUI=1 (CI); a Rust-only machine prints one line and exits 0.
gui-maybe-lint:
	@if [ -d $(GUI)/node_modules ] || [ "$(REQUIRE_GUI)" = 1 ]; then $(MAKE) --no-print-directory gui-lint; \
	 else echo "GUI checks skipped: run 'make gui-setup' (Node 24 + pnpm) to enable them."; fi
gui-maybe-test: (same shape, runs gui-test)

gui-setup:                       # clean checkout to a working GUI toolchain
	@node -e 'process.exit(+process.versions.node.split(".")[0] === 24 ? 0 : 1)' || { echo "error: the GUI needs Node 24 (found $$(node -v)); e.g. 'nvm install 24 && nvm use 24'." >&2; exit 1; }
	@command -v $(PNPM) >/dev/null || { echo "error: pnpm is required (npm i -g pnpm)." >&2; exit 1; }
	$(PNPM) --dir $(GUI) install --frozen-lockfile
	$(PNPM) --dir $(GUI) exec install-electron

gui-sidecar:                     # debug sidecar for dev and e2e
	$(CARGO) build --locked --bin banana-harness

gui-dev: gui-sidecar             # AI_API_KEY is inherited from the shell, as for `make run`
	BANANA_HARNESS_BIN=$(SIDECAR_DEBUG) $(PNPM) --dir $(GUI) dev

gui-build:                       # release sidecar + production bundles into gui/out
	$(CARGO) build --release --locked --bin banana-harness
	$(PNPM) --dir $(GUI) build

gui-package: gui-build           # macOS only in v1
	@[ "$$(uname -s)" = Darwin ] || { echo "error: v1 packages for macOS only." >&2; exit 1; }
	$(PNPM) --dir $(GUI) package   # copy-sidecar.mjs then electron-builder --mac dmg zip, host arch, unsigned

gui-package-smoke: gui-package   # macOS: launch the packaged .app through Playwright _electron; expect the window and a sidecar `ready` frame
	$(PNPM) --dir $(GUI) test:packaged

gui-lint:
	$(PNPM) --dir $(GUI) lint      # biome check; tsc -p tsconfig.node.json; tsc -p tsconfig.web.json; check-tokens; check-versions
gui-test:
	$(PNPM) --dir $(GUI) test      # vitest run

gui-e2e: gui-sidecar             # Playwright Electron suite on the ticket 05 scenarios; NOT part of `make test`
	$(CARGO) build --locked -p harness-testkit --bin fake-provider
	$(PNPM) --dir $(GUI) build
	BANANA_HARNESS_BIN=$(SIDECAR_DEBUG) FAKE_PROVIDER_BIN=$(abspath target/debug/fake-provider) \
	  $(if $(shell command -v xvfb-run),$(if $(DISPLAY),,xvfb-run -a),) $(PNPM) --dir $(GUI) test:e2e

gui-clean:
	rm -rf $(GUI)/out $(GUI)/dist $(GUI)/build/sidecar $(GUI)/test-results $(GUI)/playwright-report
```

- `make clean` removes build output only (`cargo clean` plus `gui-clean`); `gui/node_modules` is kept, so cleaning does not force a reinstall (remove it by hand to start fresh).
- **Why e2e is not in `make test`:** it needs the Electron binary, a display and the testkit, and `make test` must stay fast, offline and Rust-only for evaluators. CI runs `make gui-e2e` as its own step; ticket 00's "JavaScript tests join `make test`" is met by the Vitest suite through `gui-maybe-test`.
- **Evaluator path:** on a machine with only Rust, `make setup && make test && make lint` behave as today plus the `NO_PROXY` fix and a one-line "GUI checks skipped" notice. The live smoke block is unchanged.
- **`make setup`** still builds the release binary only; it does not install Node or Electron.
- `gui-package` stages the release binary through `gui/scripts/copy-sidecar.mjs`, which refuses a missing or debug binary, copies to `gui/build/sidecar/banana-harness` and sets mode 755.

### 4. CI (Proposed)

The existing `check` job (matrix `ubuntu-latest`, `macos-latest`: `make lint`, `make test`) is unchanged; it benefits from the `NO_PROXY` fix and prints the "GUI checks skipped" line. Two jobs are added to `.github/workflows/ci.yml` (actions pinned by tag, as the file does today; `permissions: contents: read` stays):

```yaml
  gui:
    name: gui (${{ matrix.os }})
    strategy: { fail-fast: false, matrix: { os: [ubuntu-latest, macos-latest] } }
    runs-on: ${{ matrix.os }}
    timeout-minutes: 30
    env: { REQUIRE_GUI: "1", CI: "1" }
    steps:
      - uses: actions/checkout@v4
      - run: rustup toolchain install
      - uses: Swatinem/rust-cache@v2
      - uses: pnpm/action-setup@v4              # version comes from gui/package.json "packageManager"
        with: { package_json_file: gui/package.json }
      - uses: actions/setup-node@v4
        with: { node-version-file: gui/.node-version, cache: pnpm, cache-dependency-path: gui/pnpm-lock.yaml }
      - uses: actions/cache@v4                  # Electron binary
        with: { path: "~/.cache/electron\n~/Library/Caches/electron", key: electron-${{ runner.os }}-${{ hashFiles('gui/pnpm-lock.yaml') }} }
      - if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y xvfb libnss3 libgtk-3-0 libgbm1 libasound2t64   # names Assumed; smoke on first run
      - run: make gui-setup
      - run: make gui-lint
      - run: make gui-test
      - run: make gui-e2e                       # xvfb-run is applied by the Makefile on Linux when DISPLAY is unset
      - id: leak                                # the test key must not appear in any saved artifact
        if: always()
        run: |
          if grep -rIF "sk-test-0123456789abcdef" gui/test-results 2>/dev/null; then echo "found=true" >> "$GITHUB_OUTPUT"; exit 1; fi
          echo "found=false" >> "$GITHUB_OUTPUT"
      - if: failure() && steps.leak.outputs.found != 'true'
        uses: actions/upload-artifact@v4
        with: { name: gui-test-results-${{ matrix.os }}, path: gui/test-results, retention-days: 7 }

  package:                                       # unsigned macOS DMG and ZIP
    name: package (macos)
    needs: [check, gui]
    if: github.event_name == 'push' && github.ref == 'refs/heads/main' || github.event_name == 'workflow_dispatch'
    runs-on: macos-latest
    timeout-minutes: 30
    steps:                                       # same checkout/rust/pnpm/node/cache steps, then:
      - run: make gui-setup
      - run: make gui-package
      - run: make gui-package-smoke              # launches the packaged .app via Playwright, asserts the window and the sidecar `ready` frame
      - uses: actions/upload-artifact@v4
        with: { name: banana-harness-macos-unsigned, path: "gui/dist/*.dmg\ngui/dist/*.zip", retention-days: 14 }
```

- Add `on: workflow_dispatch` and a workflow-level `concurrency` group that cancels superseded runs on the same ref.
- **Deterministic fake-provider tests:** all e2e and scenario tests run through ticket 05's `TestEnv` (empty environment, isolated `HOME` and `XDG_*`, `NO_PROXY=127.0.0.1,localhost` plus a deliberately dead outbound proxy, fixed fake key), so no CI secret and no network are needed.
- **Artifacts:** only `gui/test-results/` on failure (sidecar stderr, the stub's redacted request log, frames, the diff of `expect` versus actual, the kept temp root), uploaded only when the leak grep found nothing; the packaged DMG and ZIP are uploaded by the `package` job only. No signing credentials exist in this effort.
- **Headless Electron:** Linux runs under `xvfb-run -a`; macOS runs the real window (invisible to nobody, which is fine on a runner). Under Linux CI the e2e launch adds Chromium's `--no-sandbox` only when `CI` is set (a standard Electron-on-Actions requirement, **Assumed**); packaged builds never carry it.
- PRs do not package (macOS minutes); packaging runs on pushes to `main` and manually. The e2e suite exercises the unpackaged build on every PR.

### 5. Sidecar build, bundling and macOS packaging (Proposed on verified mechanics)

- **Sidecar:** the same `banana-harness` binary the TUI users build. Dev and e2e use the debug build through `BANANA_HARNESS_BIN` (honoured only when the app is not packaged); packaging uses `cargo build --release --locked --bin banana-harness`.
- **Bundling:** `electron-builder.yml`:
  ```yaml
  appId: <decision>                     # see questions
  productName: Banana Harness
  directories: { output: dist, buildResources: build }
  files: ["out/**", "package.json"]
  extraResources: [{ from: build/sidecar, to: bin }]     # -> Contents/Resources/bin/banana-harness (verified layout)
  artifactName: "${productName}-${version}-${arch}.${ext}"
  mac:
    target: [dmg, zip]
    identity: null                      # unsigned v1 (verified to build and launch)
    category: public.app-category.developer-tools
  ```
  Main resolves `process.resourcesPath/bin/banana-harness` when packaged (ticket 06).
- **Architecture:** the host's architecture only (arm64 on `macos-latest`). Universal or x64 builds need a second Rust target plus `lipo`; they are out of v1.
- **Unsigned development builds:** `make gui-package` produces `gui/dist/Banana Harness-<version>-arm64.dmg` and `…-arm64-mac.zip`. A locally built app runs as built (verified). For a **downloaded** unsigned app macOS Gatekeeper will warn; the documented workaround is right-click Open, or `xattr -dr com.apple.quarantine "Banana Harness.app"` (**Assumed**, not tested on this macOS). Signing, notarization and auto-update stay out of v1 (map fog; credentials would live outside the repository).
- **Scope excluded from v1:** Windows (no `win` target, no Windows CI, and ticket 03's stdout descriptor guard is unix-only); Linux **packages** (Linux is covered by CI lint, unit and e2e only; `gui-package` exits with a message off macOS; a `tar.gz` or AppImage can be added later if cheap and is not in any acceptance criterion).
- **Icon:** a proper `build/icon.icns` is needed (the smoke used Electron's default icon). It is derived from the approved banana mark in the build run; this is an asset task, not a design change.

### 6. Clean-checkout acceptance criteria and exact commands

**A. Rust-only machine (no Node), the evaluator path, unchanged:**
```
git clone git@github.com:AvgBlank/ai-banana-harness.git && cd ai-banana-harness
export AI_API_KEY=...          # only for `make run` and the live smoke
make setup                     # builds the release binary only; no Node, no testkit
make test                      # offline suite passes even with HTTP_PROXY set and no NO_PROXY; prints "GUI checks skipped: ..."
make lint                      # fmt + clippy clean; same skip notice
make run                       # TUI opens
```
Acceptance: all exit 0; `target/release/banana-harness` exists; `harness-testkit` was not built; with `HTTP_PROXY=http://127.0.0.1:9 make test` the offline suite still passes; with `NO_PROXY=corp.example.com make test` the user's entry is preserved (effective `NO_PROXY=corp.example.com,127.0.0.1,localhost`).

**B. GUI machine (Node 24, pnpm):**
```
make gui-setup                 # Node 24 check, `pnpm install --frozen-lockfile`, `install-electron`
make gui-lint                  # biome, tsc (node + web), tokens, versions
make gui-test                  # vitest: reducer, main-process modules, components
make gui-e2e                   # builds debug sidecar + fake-provider + gui, runs the Playwright scenarios
make lint && make test         # now also run gui-lint and gui-test because gui/node_modules exists
make gui-dev                   # opens the app against target/debug/banana-harness (AI_API_KEY from the shell)
make gui-package               # macOS: gui/dist/Banana Harness-<version>-arm64.dmg and …-arm64-mac.zip, unsigned
make gui-package-smoke         # macOS: launches the packaged .app, expects the window and a sidecar `ready` frame
make clean                     # removes target/, gui/out, gui/dist, gui/build/sidecar, gui/test-results
```
Acceptance:
1. `make gui-setup` on Node 25 fails with the Node 24 instruction; on Node 24 it succeeds and a second run changes nothing; a modified `package.json` without a matching lockfile makes `--frozen-lockfile` fail.
2. `make gui-lint` and `make gui-test` pass with zero Biome findings (the prototype's 40 a11y findings are fixed in the ported components), `tsc` clean under TypeScript 7 with no `baseUrl`, and `check-tokens` passes.
3. `make gui-e2e` passes on macOS and, under `xvfb-run`, on Linux; the scenarios from ticket 05 give identical model-facing requests across the bridge, process and Electron seams; failure artifacts exist and contain no occurrence of the test key.
4. `make gui-package` on macOS yields the DMG and ZIP; the `.app` contains `Contents/Resources/bin/banana-harness` with mode 755 that runs `--version`; the package contains no `node_modules` (`app.asar` listing); `hdiutil verify` is valid; `make gui-package-smoke` passes; on another OS it exits non-zero with the macOS-only message.
5. `REQUIRE_GUI=1 make lint` fails on a checkout where `gui/node_modules` is missing (this is what CI relies on); without the variable it skips with the notice.
6. CI: both existing `check` legs stay green; the `gui` job passes on Linux and macOS with `REQUIRE_GUI=1`; the `package` job uploads the unsigned DMG and ZIP from `main`; no job needs a secret.
7. `AGENTS.md`, `README.md` and `gui/AGENTS.md` (+ `gui/CLAUDE.md`) are updated in the same change; `git grep harness-testkit` outside `[dev-dependencies]`, tests and docs finds nothing in product source.
8. `make setup` is not slower or larger than today (it builds product crates only).

### Amendments to earlier tickets

- **Ticket 02 (toolchain), corrections:** Electron 44 has no `postinstall` (fetch with `install-electron`); `onlyBuiltDependencies` is not needed; the sandboxed preload is CJS by default; the pnpm layout works with electron-builder when all dependencies are `devDependencies` (bundled, no `node_modules` in the package); `@playwright/test` 1.64.1 drives Electron 44.8.0 (packaged and unpackaged); **Node 25 is unsupported** (use 24); `typescript-eslint` cannot be used with TypeScript 7 (**Biome** replaces ESLint).
- **Ticket 05 (fake provider):** `harness-testkit` approved; `gui-e2e` is the e2e entry point and `BANANA_HARNESS_BIN` / `FAKE_PROVIDER_BIN` are test-side variables only.
- **Ticket 06 (Electron):** packaged builds never carry `--no-sandbox`; the e2e launch adds it only under Linux CI; the packaged-app smoke test (`gui-package-smoke`) uses the same `_electron` mechanism verified here.
- **Ticket 09 (ADR and vocabulary):** ADR 0003 records the GUI package metadata exception (`productName`, `appId`, window title), the new crates and their boundaries, the unsigned macOS-only v1 packaging, and the testkit seam; `AGENTS.md` edits are listed in section 1.

### Remaining limitations

macOS and Linux packaging beyond the host architecture, signing and notarization, auto-update, Windows, universal binaries, and every item under "Assumed" above. The first build-run task for the GUI is a short CI smoke that closes the Linux and Gatekeeper assumptions.

### Questions that need your decision

1. **`appId` (the macOS bundle identifier).** It identifies the app to macOS (preferences, permissions, future signing and updates), so it is awkward to change once people have installed it. A default derived from the repository owner would be `io.github.avgblank.banana-harness`. Which identifier should it be? Default if you do not say: `io.github.avgblank.banana-harness`.
2. **Biome instead of ESLint.** Forced by TypeScript 7 (`typescript-eslint` does not support it); the alternative is dropping to TypeScript 6.x to keep ESLint. Default: Biome, keep TypeScript 7.
3. **Packaging cadence.** Packaging runs on pushes to `main` and manually, not on every PR. Default: yes.
