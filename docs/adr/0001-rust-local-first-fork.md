# Rust, local-first fork that drops the hosted product

Banana Harness is a fork of TrueForge. The fork rewrites the harness in Rust as a single local binary, with a ratatui TUI and a headless run as its only surfaces. It drops everything that existed to serve a hosted, multi-tenant product: the HTTP API and generated SDKs, the React UI packages, auth and tenants, session sharing, Postgres and Redis, sandboxes, schedules, and Helm/Docker.

We chose this because the target is a terminal-first coding agent, judged through a Makefile interface on a clean machine with one API key. Keeping the hosted surfaces would have doubled the port for no benefit to that target.

The trade-off is that upstream TrueForge changes can no longer be merged, and existing API, SDK and UI consumers are not supported.

**Update:** the "only surfaces" part of this decision is superseded by [ADR 0003](0003-desktop-app-surface.md), which adds a desktop app as a third surface. The rest of this decision stands.
