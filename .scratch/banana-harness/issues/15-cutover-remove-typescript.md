# 15: Cutover: remove the TypeScript product

**What to build:** the repository contains only what ships.

**Delete:**
- the TypeScript packages, the Python SDK, the Helm chart, the Docker and compose files
- pnpm/TypeScript/lint/format tooling, changesets, patches, vendored archives
- the TS scripts, the OpenAPI copies, and the docs that no longer apply
- the nested TS agent-convention files

**Replace** CI with a single Rust workflow (fmt, clippy and test) on Linux and macOS.

**Rewrite** the root agent conventions for Rust (errors, no `unwrap`, tests in tests folders, glossary upkeep), keeping the `CLAUDE.md` → `AGENTS.md` pattern.

**Blocked by:** 01–14

**Status:** done, except CI on GitHub, which runs on the cutover PR

- [x] No TypeScript package, pnpm file or TS config remains in the repo.
- [ ] CI is green on Linux and macOS.
- [x] `make setup`, `make test` and `make run` still work from a fresh clone.
- [x] Every remaining doc describes the Rust product. No stale references to the API, SDKs or web UI remain.

## Notes

**Deleted (2,833 tracked files, list confirmed by the user first):** `packages/` (all six TS
packages and their nested `AGENTS.md`/`CLAUDE.md` pairs), `python/`, `charts/`, `benchmark/`,
`scripts/`, `tests/` (only held the chart-version script's test), `.changeset/`, `.husky/`,
`.cursor/`, `.railway/`, `.github/fern/` (with both OpenAPI copies), `Dockerfile`,
`Dockerfile.npm`, both compose files, `.dockerignore`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`,
root `package.json`, `tsconfig.base.json`, `eslint.config.mjs`, `.prettierrc`, `.prettierignore`,
`.npmrc`, `.nvmrc`, `RELEASING.md`, `.github/CODEOWNERS`, the five TS/release workflows, and every
hosted-product doc under `docs/` (only `docs/adr/` remains). `patches/` and vendored archives never
existed in this repo.

**Replaced / rewritten:** `.github/workflows/ci.yml` is one Rust job (`rustup toolchain install`
from `rust/rust-toolchain.toml`, `make lint`, `make test`) on `ubuntu-latest` and `macos-latest`;
`dependabot.yml` tracks cargo in `/rust` (ignoring the exactly-pinned `rmcp`) and actions. The
README moved from `rust/README.md` to the root (one copy; paths fixed). `AGENTS.md` is rewritten
for Rust (crate boundaries, anyhow, no `unwrap`, tests in `tests/`, glossary/ADR upkeep, the
`CLAUDE.md` → `AGENTS.md` pattern); `CONTRIBUTING.md`, `SECURITY.md`, the PR/issue templates and
`.gitignore` are rewritten. `LICENSE` keeps TrueFoundry's MIT notice (the licence requires it) and
adds the fork's copyright line. Security/conduct contacts and repo links point at
`github.com/avgblank/banana-harness`.

**Kept on purpose:** `rust/`, `.scratch/`, `docs/adr/`, `CONTEXT.md`, `.env.example`. The
"Ported from TrueForge" comments in the Rust code are accurate provenance. The compaction prompt
still says "typescript code changes" because the spec requires TrueForge's prompt verbatim.

**Fresh-clone check (macOS, this machine):** `git clone --branch banana/impl` into a new
directory, then `make setup` (59 s release build), `make test` (all green, live smoke skipped
without a key) and `make run` in a tmux PTY (provider popup, main screen, clean exit 0).
The Linux and fresh-macOS-user runs are ticket 16.
