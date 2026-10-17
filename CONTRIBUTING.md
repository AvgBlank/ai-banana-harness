# Contributing

Thanks for helping improve Banana Harness. It's a single Rust binary: a ratatui TUI and a headless
run over one agent loop, plus a macOS desktop app in `gui/` that drives the same binary as a sidecar.
Read `README.md` for what it does, `CONTEXT.md` for the vocabulary and
`AGENTS.md` for the conventions every change follows.

## Setup

You need a C compiler (it builds SQLite and TLS). `make setup` installs Rust through rustup when
it's missing; `rust-toolchain.toml` pins the toolchain.

```
make setup   # locked release build
make run     # the TUI (needs AI_API_KEY for a real run)
```

The desktop app is optional: it needs Node 24 and pnpm (`make gui-setup`), and `make gui-dev`
runs it. See `gui/AGENTS.md` for its rules.

## Layout

```
Cargo.toml               the workspace (dependencies pinned under [workspace.dependencies])
config/default.toml      the committed config, embedded in the binary
crates/harness-llm       DeepSeek/Qwen streaming chat client
crates/harness-tools     host tools and the guardrail engine
crates/harness-mcp       MCP client (stdio, streamable HTTP, OAuth)
crates/harness-store     SQLite persistence and migrations
crates/harness-core      config, events, the turn runner and thread loop
crates/harness-tui       the terminal app
crates/harness-bridge    the sidecar protocol (`banana-harness serve --stdio`)
crates/banana-harness    the binary (TUI, plus `run` for headless and `serve` for the sidecar)
crates/harness-testkit   the fake provider, `TestEnv`, scenarios and the `fake-provider` binary (dev only)
gui/                     the desktop app (Electron, TypeScript)
fixtures/, scripts/      the live smoke fixture and script
```

## Tests

```
make test   # the offline suite; with AI_API_KEY set, also the live smoke against the provider your key belongs to
make lint   # cargo fmt --check, and clippy with warnings denied
```

Tests live in each crate's `tests/` folder and drive the system from the outside: `harness-core`
tests run real turns against the fake provider from `harness-testkit`, and `harness-tui` tests feed events and key
presses into the app state and render it with ratatui's `TestBackend`. `harness-bridge` tests drive the
sidecar protocol over an in-memory stream, and the binary's `serve_stdio` test runs the real process. The desktop app is tested in `gui/tests/` (`make gui-test`, and `make gui-e2e` against a fake provider).

## Pull requests

- Keep each PR to one change, with tests.
- `make lint` and `make test` must pass (they include the desktop app's checks once `make gui-setup` has run), and `make gui-e2e` for a change that touches `gui/` or the sidecar; CI runs them on Linux and macOS.
- Update `README.md` and `/help` when behaviour, config or key bindings change.
- Never commit an API key or other secret.
