# Contributing

Thanks for helping improve Banana Harness. It's a single Rust binary: a ratatui TUI and a headless
run over one agent loop. Read `README.md` for what it does, `CONTEXT.md` for the vocabulary and
`AGENTS.md` for the conventions every change follows.

## Setup

You need a C compiler (it builds SQLite and TLS). `make setup` installs Rust through rustup when
it's missing; `rust/rust-toolchain.toml` pins the toolchain.

```
make setup   # locked release build
make run     # the TUI (needs AI_API_KEY for a real run)
```

## Layout

```
rust/
  config/default.toml      the committed config, embedded in the binary
  crates/harness-llm       DeepSeek/Qwen streaming chat client
  crates/harness-tools     host tools and the guardrail engine
  crates/harness-mcp       MCP client (stdio, streamable HTTP, OAuth)
  crates/harness-store     SQLite persistence and migrations
  crates/harness-core      config, events, the turn runner and thread loop
  crates/harness-tui       the terminal app
  crates/banana-harness    the binary (TUI, plus `run` for headless)
  fixtures/, scripts/      the live smoke fixture and script
```

## Tests

```
make test   # the offline suite; with AI_API_KEY set, also the live smoke against both providers
make lint   # cargo fmt --check, and clippy with warnings denied
```

Tests live in each crate's `tests/` folder and drive the system from the outside: `harness-core`
tests run real turns against an in-test fake provider, and `harness-tui` tests feed events and key
presses into the app state and render it with ratatui's `TestBackend`.

## Pull requests

- Keep each PR to one change, with tests.
- `make lint` and `make test` must pass; CI runs them on Linux and macOS.
- Update `README.md` and `/help` when behaviour, config or key bindings change.
- Never commit an API key or other secret.
