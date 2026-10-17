# Scope, stack and constraints for the desktop app

Type: grilling
Status: resolved
Blocked by:

## Question

What is the destination, v1 scope, theme, platforms, stack, bridge, ADR, navigation, terminology and test layers for a Banana Harness desktop GUI?

## Answer

Settled with the user (2026-10-17); not to be reopened unless repository inspection finds a genuine technical contradiction.

- **Destination:** a build-ready spec: architecture, screens, tokens, protocol, repo layout, ADR/terminology updates, dependency-ordered implementation tickets with acceptance criteria, and the test/packaging strategy. The app itself is built in a separate run.
- **v1 scope:** icon rail; sidebar with workspaces and sessions; centered home screen with composer; conversation view with tool-call cards; Changes/diff panel; approvals; model picker. Excluded: voice input, marketing/update badges. Every included feature is backed by real behaviour; gaps become core extensions or documented blockers, never mocks.
- **Theme:** dark-first on `#1A1914` with `#FFD60A` accent, warm browns, restrained green/red diff tints; warm banana-cream light theme. System/Dark/Light, persisted, CSS variables. Yellow used selectively, never as a flood.
- **Platforms:** macOS first; shared code stays cross-platform; Linux packaging only when cheap.
- **Stack:** Electron + React + strict TypeScript + Vite in top-level `gui/`; Tailwind, shadcn/ui (one primitive system), Lucide, Motion, Monaco for code/diffs; Vitest + React Testing Library; Playwright for Electron smoke tests. Makefile targets for GUI workflows.
- **Bridge:** Rust sidecar speaking NDJSON over stdio, launched and supervised by Electron's main process. Typed requests/replies with ids, structured errors, async event notifications, approval replies, clean shutdown. stdout is protocol only; logs go to stderr. Reuse the tagged `Event` types. No HTTP/WebSocket or native bindings. The Rust core stays the source of truth.
- **ADR/terms:** ADR 0003 adds the desktop app as a third surface, supersedes the "only surfaces" restriction in ADR 0001 (history kept), defines Electron as a thin client and records security/testing implications. `CONTEXT.md` drops the "avoid GUI" prohibition.
- **Navigation:** sessions grouped by workspace; open-folder action; one lazily opened `Harness` per workspace, reused when already open; events and approvals routed by workspace and session.
- **Terminology:** "session" everywhere in visible UI (New session, Recent sessions, Resume session); never "chat" or "thread".
- **Tests:** sidecar integration tests in its crate's `tests/` against the fake provider (requests, replies, events, approvals, errors, shutdown, stdout is valid NDJSON); Vitest/RTL for renderer; at least one Playwright Electron smoke test against the fake provider and real protocol. JS checks join `make lint` / `make test`, run in CI on Linux and macOS.
- **Security:** context isolation on, Node integration off, narrow typed preload API, validated IPC and sidecar messages, process management only in main, no shell/filesystem access from React, core approval boundaries preserved.
