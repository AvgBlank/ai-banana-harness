# Banana Harness

A local-first, terminal-first coding-agent harness: one binary, a ratatui **TUI** and a
**headless run**, driving DeepSeek or Qwen to resolve a GitHub issue in a local repository.

The code lives in `rust/` (a Cargo workspace). The design is in `.scratch/banana-harness/spec.md`,
the vocabulary in `CONTEXT.md`, and the key decisions in `docs/adr/`.

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

1. the committed default, embedded in the binary (`rust/config/default.toml`)
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
| `Enter` | send the message (or run the highlighted `/command`) |
| `Shift+Enter` / `Alt+Enter` / `Ctrl+J` | insert a newline |
| `←` `→` `↑` `↓`, `Home` / `End` (`Ctrl+A`) | move the cursor (`↑`/`↓` by visual row) |
| `Alt+←` / `Alt+→` (also `Alt+B`/`Alt+F`, `Ctrl+←`/`Ctrl+→`) | move by word |
| `Alt+Backspace` / `Ctrl+W` | delete the word before the cursor (`Alt+Delete`/`Alt+D` after it) |
| `Ctrl+U` / `Ctrl+K` | delete to the start / end of the line |
| `Ctrl+C` | cancel the running turn; twice to quit |
| `Ctrl+B` | toggle the sidebar |
| `Ctrl+E` | edit the input in `$EDITOR` |
| `Ctrl+O` (or `Enter` on empty input) | expand/collapse the last tool card |
| mouse wheel, `PgUp` / `PgDn` | scroll the transcript |
| `Esc` | close an overlay |

On macOS, Option is Alt for the word keys above. If Option+key types a symbol instead, turn on
"Use Option as Meta key" (Terminal.app) or set Left Option to "Esc+" (iTerm2); `Ctrl+W` and
`Ctrl+←`/`Ctrl+→` work either way. The TUI captures the mouse for wheel scrolling, so select text
with `Shift`+drag (`Option`+drag in iTerm2).

Slash commands: `/new` (fresh session), `/sessions` (list and resume), `/provider` (switch
DeepSeek/Qwen), `/undo` (restore the workspace to the last turn's checkpoint), `/help`, `/quit`.
Typing `/` shows matching commands as you type: `↑`/`↓` pick one, `Tab` completes it, `Enter`
runs it, `Esc` hides the list.

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
make test    # offline suite (plus the live smoke when AI_API_KEY is set)
make lint    # cargo fmt --check, and clippy with warnings denied
```

Tests live in each crate's `tests/` folder. `rust/scripts/live_smoke.sh <deepseek|qwen>` runs the
live smoke test standalone (needs `AI_API_KEY`); `make test` wires it in automatically when the
key is present.
