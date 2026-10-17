# 13: gui/ scaffold, toolchain and make targets

**Lane / milestone:** G · M4 GUI foundation   **Size:** L (one agent session)

**What to build:** Create the production `gui/` project with its toolchain and make targets (ticket 08), a window that opens, and the frozen design tokens. No product behaviour yet. This ticket also closes the toolchain assumptions listed in ticket 08. Split point if over budget: project and toolchain (13a) then the make-target integration and docs (13b).

**Blocked by:** 01, 07

**Status:** done

**Files and components likely to change:**
- `gui/package.json` (name, `productName`, `appId` `io.github.avgblank.banana-harness`, `packageManager` pnpm 10.29.2, `engines` `^24.15.0`, exact devDependencies only, scripts), `pnpm-lock.yaml`, `.npmrc` (`engine-strict=true`), `.node-version`, `.gitignore`
- `gui/{electron.vite.config.ts,electron-builder.yml (skeleton),biome.json,tsconfig*.json,vitest.config.ts}`
- `gui/src/{main,preload,renderer,shared}/` minimal: a sandboxed window, a frozen preload object, `renderer/styles/tokens.css` (the frozen copy) and `index.css`, a themed empty shell
- `gui/scripts/{check-tokens.mjs,check-versions.mjs}`
- `gui/AGENTS.md`, `gui/CLAUDE.md` (exactly `@AGENTS.md`)
- `Makefile`: `gui-setup`, `gui-sidecar`, `gui-dev`, `gui-build`, `gui-lint`, `gui-test`, `gui-clean`, `gui-maybe-lint`, `gui-maybe-test`, `.PHONY`, and `clean` calling `gui-clean`

**Acceptance criteria:**
- [x] `make gui-setup` on Node 25 fails with the Node 24 instruction (closes the unobserved `engine-strict` assumption); on Node 24 it succeeds, is repeatable, and `--frozen-lockfile` fails when `package.json` and the lockfile disagree.
- [x] `make gui-lint` is clean: Biome with the Tailwind CSS option (closes that assumption), `tsc` for both projects under TypeScript 7 with no `baseUrl`, `check-tokens`, `check-versions` (app version equals the `banana-harness` crate version).
- [x] `make gui-test` runs Vitest 5 with jsdom 30 on Node 24 (closes that assumption) with a first smoke test.
- [x] `make gui-dev` opens the themed shell; `make gui-build` produces `gui/out`.
- [x] `REQUIRE_GUI=1 make lint` fails where `gui/node_modules` is missing; without it `make lint` and `make test` skip with the one-line notice and exit 0 on a machine without Node.
- [x] All dependencies are devDependencies; no `postinstall` is relied on (`install-electron` is run by `gui-setup`).
- [x] `make setup`, `make run`, `make test` and `make lint` behave as before on a Rust-only machine.

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

**Completion record:**
- `gui/` holds the toolchain and an empty themed shell: exact devDependencies only (electron 44.8.0, electron-vite 6.0.0, vite 8.3.4, React 19.3.1, TypeScript 7.0.2, Tailwind 4.3.3, Vitest 5.0.5, jsdom 30.1.3, Testing Library 16.3.3 with dom 10.4.2, Biome 2.5.15, `@types/node` 24.19.2), `packageManager` pnpm 10.29.2, `engines` `^24.15.0`, `.npmrc` `engine-strict=true`, `.node-version` 24, a committed `pnpm-lock.yaml`. The main process opens a sandboxed, context-isolated window with navigation, window opening and permissions denied and the title taken from package metadata; the preload exposes a frozen `window.banana` (`platform` only); the renderer carries the frozen `tokens.css` and the rail and sidebar columns with an empty state.
- Makefile: `gui-setup`, `gui-sidecar`, `gui-dev`, `gui-build`, `gui-lint`, `gui-test`, `gui-clean`, `gui-maybe-lint`, `gui-maybe-test`, `.PHONY`; `lint` and `test` call the `maybe` targets; `clean` calls `gui-clean`.
- Docs: `gui/AGENTS.md` and `gui/CLAUDE.md`; root `AGENTS.md` lines 1, 4, 8 and 10; `CONTRIBUTING.md` and `README.md` as listed above.
- Observed (the assumptions this ticket closes): `engine-strict` rejects Node 25 through `pnpm install` and `make gui-setup` names Node 24 first; `--frozen-lockfile` fails with `ERR_PNPM_OUTDATED_LOCKFILE` when `package.json` and the lockfile differ; Biome parses `@theme` only with `css.parser.tailwindDirectives` (off gives a parse error); `tsc` rejects `baseUrl` (TS5102) and passes with `paths`; Vitest 5 with jsdom 30 ran a first test under Node 24.22.0; `make gui-dev` started the debug sidecar build, the dev server and Electron; the built app (`electron .`) was probed over the DevTools protocol: page loaded, `window.banana` frozen, `typeof require` and `typeof process` undefined, `window.open` returned null, `data-theme` system, dark background.
- Deviations: (1) `check-tokens.mjs` is the prototype's script with two behaviour-preserving edits for Biome (no assignment inside an expression, a template string) and the new tokens path. (2) Biome does not format the frozen `tokens.css`. (3) `electron-builder.yml` is a skeleton and `electron-builder` is not installed yet; `appId` appears in both `package.json` and the yml, and `check-versions` requires them to match. (4) The renderer shell does not contain the product name; the window title comes from `app.getName()`. (5) `vitest` and `electron-vite` load their `.ts` config as CommonJS and Vite prints a one-line notice about it; `"type": "module"` is not set because the preload must stay CommonJS.
- Not covered: Linux; macOS packaging and Playwright (later tickets); a pixel-level look at the window (only structure and computed style were probed).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
