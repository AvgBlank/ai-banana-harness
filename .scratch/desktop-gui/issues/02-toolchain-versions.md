# Compatible toolchain and versions

Type: research
Status: resolved
Blocked by:

## Question

Which current, mutually compatible versions and tooling should be pinned for Electron, React, Vite, Tailwind CSS, shadcn/ui, Monaco (and its Vite/Electron worker setup), Vitest + React Testing Library, Playwright's Electron support, and the Electron build/packaging layer (electron-vite vs Forge vs electron-builder; macOS dmg/zip, unsigned vs signed)? Use current docs (Context7), not memory. Also: package manager choice and lockfile/`--frozen-lockfile` equivalent to match the repo's `--locked` convention.

Findings: [research/02-toolchain.md](../research/02-toolchain.md)

## Answer

Recommended: Node 24 LTS, pnpm with a committed lockfile (`pnpm install --frozen-lockfile`), electron-vite for dev/build and electron-builder for macOS dmg/zip packaging (unsigned via `mac.sign.identity: null`). Pins and gotchas (Tailwind 4 CSS-first config with shadcn 4, Monaco `?worker` setup with `loader.config({ monaco })` to avoid the CDN, Playwright's experimental `_electron` API) are in [research/02-toolchain.md](../research/02-toolchain.md). Versions came from the live npm registry; the items the report marks unverified (TypeScript 7 across tools, Electron 44 with Playwright, pnpm's Electron postinstall key, preload CJS) must be smoke-tested when the build starts.

### Corrections from ticket 08 (2026-10-17, verified in a smoke build)

- Electron 44 has **no `postinstall` script**: `pnpm install` does not fetch the binary; run `pnpm exec install-electron` (or let the first launch download it). `pnpm.onlyBuiltDependencies` is **not needed** for Electron.
- The sandboxed **preload is CommonJS** by default with electron-vite 6 when `package.json` has no `"type": "module"`.
- **pnpm with electron-builder works** with the default isolated layout when every dependency is a `devDependency`: electron-vite bundles them and the packaged app contains no `node_modules` (`node-linker=hoisted` is not needed).
- **Playwright 1.64.1 `_electron` drives Electron 44.8.0**, packaged and unpackaged.
- **Node 25 is unsupported** by Vitest 5 and jsdom 30 (`^22.12 || ^24 || >=26`); the GUI pins Node 24 (`.node-version`, `engines`, `engine-strict`).
- **`typescript-eslint` cannot be used with TypeScript 7** (peer `typescript <6.1.0`); lint and format use **Biome 2.5.15**, which parsed all 35 prototype TypeScript files.
- Unsigned (`mac.identity: null`) DMG and ZIP build and the unsigned app launches (details in ticket 08).
