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

**Status:** ready-for-agent

- [ ] No TypeScript package, pnpm file or TS config remains in the repo.
- [ ] CI is green on Linux and macOS.
- [ ] `make setup`, `make test` and `make run` still work from a fresh clone.
- [ ] Every remaining doc describes the Rust product. No stale references to the API, SDKs or web UI remain.
