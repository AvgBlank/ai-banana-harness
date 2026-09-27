# Banana Harness

A local-first, terminal-first coding-agent harness: one binary, a ratatui **TUI** and a
**headless run**. See `../.scratch/banana-harness/spec.md` for the full design.

## Evaluator quickstart

```
export AI_API_KEY=...   # a DeepSeek or Qwen key
make setup               # installs Rust if missing, builds the release binary
make run                 # opens the TUI
```

`make run` opens a popup to choose the provider (**DeepSeek** or **Qwen**) and, if `WORKSPACE`
isn't set, to enter the workspace path. Set it directly to skip that prompt:

```
WORKSPACE=/path/to/repo make run
```

Paste a GitHub issue into the input box and press Enter; the agent reads, edits and runs
commands in the workspace until it resolves it, streaming every step into the TUI.

`make test` runs the offline test suite; with `AI_API_KEY` set it also runs a live smoke test
against both providers (a small fixture repository with one failing test). `make clean` removes
build artefacts.

## Configuration

Settings are TOML, layered so each later file wins:

1. the committed default, embedded in the binary (`config/default.toml`)
2. the user config directory (`$XDG_CONFIG_HOME/banana-harness/config.toml`, or the platform
   equivalent)
3. the workspace's own `.banana-harness/config.toml`

The API key is never read from a config file — only from `AI_API_KEY` in the environment.

## Key bindings

Startup popup: `↑`/`↓` toggle the provider, type to fill the workspace path, `Enter` confirms,
`q`/`Ctrl+C` quits.

Main screen:

| Key | Action |
| --- | --- |
| `Enter` | send the message |
| `Alt+Enter` | insert a newline |
| `Ctrl+C` | cancel the running turn; twice to quit |
| `Ctrl+B` | toggle the sidebar |
| `Ctrl+E` | edit the input in `$EDITOR` |
| `Ctrl+O` (or `Enter` on empty input) | expand/collapse the last tool card |
| `PgUp` / `PgDn` | scroll the transcript |
| `Esc` | close an overlay |

Slash commands: `/new` (fresh session), `/sessions` (list and resume), `/provider` (switch
DeepSeek/Qwen), `/undo` (restore the workspace to the last turn's checkpoint), `/help`, `/quit`.

## Guardrails

Every host tool goes through a permission engine (`[permission]` in config): each of `read`,
`edit`, `glob`, `grep`, `bash`, `external_directory` and `mcp` resolves to `allow`, `ask` or
`deny`, with pattern tables (last match wins) for `bash`, `edit`, `read` and `mcp`. The committed
default is autonomous except for:

- a bash denylist (`sudo`, `su`, `rm -rf /` or `~`, `git push`, piping curl/wget into a shell,
  `mkfs`, `dd`, shutdown/reboot, a recursive `chmod 777 /`)
- reading `.env*` files
- any directory outside the workspace

Paths are confined to the workspace (symlinks included); the API key is redacted from every
tool result, persisted event and the environment of shell commands; shell commands time out and
their whole process group is killed; three identical consecutive tool calls warn the model, a
fifth ends the turn.

## Development

```
cd rust
cargo test --workspace --locked   # offline suite
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Tests live in each crate's `tests/` folder. `rust/scripts/live_smoke.sh <deepseek|qwen>` runs the
live smoke test standalone (needs `AI_API_KEY`); `make test` wires it in automatically when the
key is present.
