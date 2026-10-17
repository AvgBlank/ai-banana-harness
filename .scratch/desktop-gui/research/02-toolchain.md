# Toolchain and versions (researched 2026-10-17)

Sources: `npm view` against the live registry for versions and engines/peer ranges (verified); Context7 docs for Electron, Vite/electron-vite, shadcn/ui, Monaco, Playwright, electron-builder (verified where stated). Items marked UNVERIFIED were not confirmed.

## Recommended pins (exact versions, no carets)

| Package | Pin | Notes |
|---|---|---|
| Node (dev/CI) | 24 LTS (>=24.15) | vitest 5 needs ^22.12/^24/>=26; jsdom 30 needs ^22.22.2/^24.15/>=26; electron 44 package needs >=22.12. Node 26 also works but is not LTS (UNVERIFIED when 26 goes LTS). |
| electron | 44.8.0 | npm `latest`. Electron supports the latest 3 majors, so 42-44 are supported. |
| react / react-dom | 19.3.1 | |
| vite | 8.3.4 | Node ^20.19 or >=22.12 |
| @vitejs/plugin-react | 6.1.2 | peer `vite ^8`; this is why Vite is 8 |
| electron-vite | 6.0.0 | peer `vite ^6 \|\| ^7 \|\| ^8` |
| typescript | 7.0.2 | latest tag. UNVERIFIED that every tool (shadcn, vitest, typescript-eslint) is clean on TS 7; fall back to the 6.x line if anything breaks. |
| tailwindcss + @tailwindcss/vite | 4.3.3 | v4 |
| shadcn (CLI) | 4.22.0 | dev-time tool; generated components are copied into the repo |
| monaco-editor | 0.57.0 | |
| @monaco-editor/react | 4.7.0 | peer `monaco-editor >=0.25 <1` (fine) |
| vitest | 5.0.5 | peer `vite ^6.4 \|\| ^7 \|\| ^8`; jsdom peer `*` |
| jsdom | 30.1.3 | |
| @testing-library/react | 16.3.3 | needs `@testing-library/dom` ^10 as a peer; install it explicitly |
| @playwright/test | 1.64.1 | |
| electron-builder | 26.15.3 | |
| @types/node | match Node major (24.x) | |

Versions were the registry `latest` on 2026-10-17; re-run `npm view` before the first commit and then freeze via the lockfile. Note: `@testing-library/jest-dom` returned 6.x only by memory, not verified; install the current latest.

## Build / packaging recommendation: electron-vite + electron-builder

- electron-vite 6 gives one config (`electron.vite.config.ts`) for main, preload and renderer, with the renderer as a plain Vite 8 app, HMR in dev, and a shared Vite plugin ecosystem (React, Tailwind, Monaco workers). Main/preload dependency externalization is on by default since v5 (`externalizeDepsPlugin` is deprecated). ESM output for main/preload is opt-in via `rollupOptions.output.format: 'es'`.
- electron-builder handles packaging only: `mac.target` of `dmg` and `zip` (arch `arm64`, optionally `universal`/`x64`). It is the more flexible packager for dmg+zip, signing/notarization and later auto-update; `zip` is the target electron-updater needs.
- Why not Electron Forge: `@electron-forge/cli` 8.0.1 requires Node >=22.13 (fine) but its official Vite plugin is a separate, historically experimental path; electron-vite is more mature for a Vite renderer. Forge is a reasonable alternative, but I did not evaluate its Vite plugin's current state (UNVERIFIED).
- Unsigned vs signed: electron-builder `mac.sign.identity: null` skips signing entirely. `"-"` is ad-hoc signing, which runs only on the building machine (needs hardened-runtime tweaks). For internal distribution, ship unsigned dmg/zip and document the Gatekeeper right-click/`xattr -cr` workaround (UNVERIFIED on current macOS). Signed + notarized needs an Apple Developer ID certificate (`CSC_LINK`, `CSC_KEY_PASSWORD`) and notarization credentials; keep those out of the repo (matches the AGENTS.md secret rule).
- Native modules: none needed in the renderer; the Rust harness is a separate process, so no rebuild step is expected (depends on how the GUI talks to it; out of scope here).

## Known incompatibilities and gotchas

1. **Tailwind v4 and shadcn**: shadcn 4.x targets Tailwind v4 (CSS-first). Vite setup is `@tailwindcss/vite` plugin plus `@import "tailwindcss";` in the CSS; there is no `tailwind.config.js` or PostCSS config. Do not mix with Tailwind v3 docs. shadcn also needs a `@` path alias (in both `tsconfig` paths and the Vite alias; with electron-vite put the alias in the `renderer` section) and `@types/node`. `shadcn init` now asks for a base library (`radix`, `base`, `aria`); choose and record it.
2. **Monaco workers under Vite**: use the `?worker` import pattern from the official ESM guide (implement `MonacoEnvironment.getWorker`, not `getWorkerUrl`), importing `monaco-editor/esm/vs/editor/editor.worker?worker` and the json/css/html/ts workers. The doc example uses `/monaco-editor/...` URLs; in a bundled app import via the package specifier. Ship only the languages needed to keep the bundle small.
3. **@monaco-editor/react defaults to loading Monaco from a CDN**. In Electron this breaks offline use and a strict CSP. Call `loader.config({ monaco })` with the locally imported `monaco-editor` (documented behaviour of the wrapper; version-specific detail UNVERIFIED for 4.7.0). Alternatively use `monaco-editor` directly.
4. **Worker CSP**: set a renderer CSP that allows `worker-src 'self' blob:` for Monaco workers (UNVERIFIED against Electron 44 specifics).
5. **Vitest 5 and Node floor**: vitest 5 and jsdom 30 drop Node 20. CI Node must be >=22.12 (24 recommended).
6. **Vite 8 / plugin-react 6**: plugin-react 6 requires Vite 8, so Vite 7 pins would force plugin-react 5. Vite 8 uses Rolldown, so older Rollup-specific plugins may misbehave (UNVERIFIED for the plugins above, none known to be needed).
7. **Playwright and Electron**: supported through the experimental `_electron` API (`electron.launch({ args })`, `firstWindow()`, `app.evaluate()` for main-process access). It is still underscore-prefixed/experimental. Native dialogs are not intercepted; stub them via `electronApp.evaluate`. Point `args` at the built output (`out/main/index.js`) after `electron-vite build`. UNVERIFIED whether Playwright 1.64.1 is formally tested against Electron 44; pin both and run a smoke test in CI.
8. **Playwright on CI**: Electron tests need a display on Linux (xvfb); macOS runners are fine.
9. **ESM main process**: if main/preload are built as ESM, update `package.json` `main` and preload paths (`.mjs`); sandboxed preload scripts may need CJS (UNVERIFIED; default to CJS preload).

## Package manager and locked install

Recommend **pnpm** (11/12 line; registry shows pnpm 12.14.0, and pnpm is installed locally at /opt/homebrew/bin/pnpm, version not checked). Pin it with `"packageManager": "pnpm@<exact>"` in package.json and `engines.node`.

- Commit `pnpm-lock.yaml`.
- Locked install (equivalent of cargo `--locked`): `pnpm install --frozen-lockfile` (this is also pnpm's default in CI environments).
- Electron needs its postinstall binary download allowed; recent pnpm blocks dependency build scripts by default, so list `electron`, `esbuild` (if present) in `pnpm.onlyBuiltDependencies` (or pnpm's equivalent setting in `pnpm-workspace.yaml`). UNVERIFIED for pnpm 12 exact key naming; check pnpm docs when scaffolding.
- Alternative: npm with `npm ci` (equivalent semantic, no extra tool), at the cost of slower installs and hoisting quirks with electron-builder (pnpm needs `node-linker=hoisted` in `.npmrc` for electron-builder in some setups; UNVERIFIED for current electron-builder). If that causes friction, choose npm to avoid the setting.

Add `make` targets (e.g. desktop setup/test/lint) wrapping `pnpm install --frozen-lockfile` per the repo's Makefile convention.
