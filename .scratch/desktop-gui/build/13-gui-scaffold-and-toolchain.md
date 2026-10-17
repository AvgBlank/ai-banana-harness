# 13: gui/ scaffold, toolchain and make targets

**Lane / milestone:** G · M4 GUI foundation   **Size:** L (one agent session)

**What to build:** Create the production `gui/` project with its toolchain and make targets (ticket 08), a window that opens, and the frozen design tokens. No product behaviour yet. This ticket also closes the toolchain assumptions listed in ticket 08. Split point if over budget: project and toolchain (13a) then the make-target integration and docs (13b).

**Blocked by:** 01, 07

**Status:** open (not started)

**Files and components likely to change:**
- `gui/package.json` (name, `productName`, `appId` `io.github.avgblank.banana-harness`, `packageManager` pnpm 10.29.2, `engines` `^24.15.0`, exact devDependencies only, scripts), `pnpm-lock.yaml`, `.npmrc` (`engine-strict=true`), `.node-version`, `.gitignore`
- `gui/{electron.vite.config.ts,electron-builder.yml (skeleton),biome.json,tsconfig*.json,vitest.config.ts}`
- `gui/src/{main,preload,renderer,shared}/` minimal: a sandboxed window, a frozen preload object, `renderer/styles/tokens.css` (the frozen copy) and `index.css`, a themed empty shell
- `gui/scripts/{check-tokens.mjs,check-versions.mjs}`
- `gui/AGENTS.md`, `gui/CLAUDE.md` (exactly `@AGENTS.md`)
- `Makefile`: `gui-setup`, `gui-sidecar`, `gui-dev`, `gui-build`, `gui-lint`, `gui-test`, `gui-clean`, `gui-maybe-lint`, `gui-maybe-test`, `.PHONY`, and `clean` calling `gui-clean`

**Acceptance criteria:**
- [ ] `make gui-setup` on Node 25 fails with the Node 24 instruction (closes the unobserved `engine-strict` assumption); on Node 24 it succeeds, is repeatable, and `--frozen-lockfile` fails when `package.json` and the lockfile disagree.
- [ ] `make gui-lint` is clean: Biome with the Tailwind CSS option (closes that assumption), `tsc` for both projects under TypeScript 7 with no `baseUrl`, `check-tokens`, `check-versions` (app version equals the `banana-harness` crate version).
- [ ] `make gui-test` runs Vitest 5 with jsdom 30 on Node 24 (closes that assumption) with a first smoke test.
- [ ] `make gui-dev` opens the themed shell; `make gui-build` produces `gui/out`.
- [ ] `REQUIRE_GUI=1 make lint` fails where `gui/node_modules` is missing; without it `make lint` and `make test` skip with the one-line notice and exit 0 on a machine without Node.
- [ ] All dependencies are devDependencies; no `postinstall` is relied on (`install-electron` is run by `gui-setup`).
- [ ] `make setup`, `make run`, `make test` and `make lint` behave as before on a Rust-only machine.

**Verification** (commands to run; none has been run by the planning):

```
make gui-setup
make gui-lint
make gui-test
make gui-build
REQUIRE_GUI=1 make lint
make lint
make test
```

**Documentation:** Applies from the drafts: `gui/AGENTS.md` and `gui/CLAUDE.md`; `AGENTS.md` lines 1 (the `gui/` clause and the targets that exist now), 4 (name exception), 8 (JavaScript pinning) and 10 (tokens example); `CONTRIBUTING.md` setup, layout (`gui/`) and the pull-request bullet without `gui-e2e`; the `README.md` Development test and lint lines.

**Notes:** Specification: ticket 08 sections 1 to 3.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
