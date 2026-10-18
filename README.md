# Banana Harness

A local-first, terminal-first coding-agent harness: one binary with a ratatui **TUI** and a
**headless run**, plus a macOS **desktop app** that drives the same binary, all driving DeepSeek or
Qwen to resolve a GitHub issue in a local repository.

The code is a Cargo workspace at the repository root (`crates/`). The design is in `.scratch/banana-harness/spec.md`,
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

`make test` runs the offline test suite; with `AI_API_KEY` set it also runs a live smoke test (a
small fixture repository with one failing test) against each provider, skipping the one that
rejects the key. `make clean` removes
build artefacts.

## Desktop app

The desktop app is an optional macOS window over the same engine: a session list grouped by
workspace, a live transcript with tool cards, a Changes panel, and approval dialogs. It needs
Node 24 and pnpm; the evaluator quickstart above needs neither.

    make gui-setup     # Node 24 check, locked install, fetch the Electron binary
    make gui-dev       # run the app against a debug build (AI_API_KEY comes from your shell)
    make gui-package   # macOS (Apple silicon) only: an unsigned DMG and ZIP in gui/dist/

The app reads the key from `AI_API_KEY` like everything else. Launched from a terminal it uses that
environment; launched from the Dock it also looks in your login shell, and otherwise lets you paste a
key that is kept in memory for that session only. It never stores the key.

Settings starts with a **Profile**: an optional display name (1 to 60 characters) kept only in the
app's preferences file on this Mac. It is not an account and is never sent anywhere; clear it to
remove it. Under it, Settings shows whether a key is available and where it came from, never the key.
A key you typed for the session can be signed out: the app forgets it and restarts the sidecar, and
the window returns to the no-key panel. Sign-out is refused while a turn runs, and a key from
`AI_API_KEY` or your login shell has to be removed there.

A workspace with its own `.banana-harness/config.toml` opens in **restricted mode**: that file can
start programs, loosen guardrails and change where your key is sent, so it is ignored until you
review and trust that exact file. Trust is remembered per file content; if the file changes you are
asked again. A provider that points anywhere other than the baseline, or at a non-HTTPS address
that is not loopback, is blocked.

Version 1 is macOS on Apple silicon only, and the package is neither signed nor notarized. macOS
blocks a copy that came from the internet: with the quarantine attribute set the app does not open.
Remove the attribute once, then open it as usual (a copy you built yourself is not affected):

    xattr -d com.apple.quarantine "Banana Harness.app"

## Configuration

Settings are TOML, layered so each later file wins:

1. the committed default, embedded in the binary (`config/default.toml`)
2. the user config directory (`$XDG_CONFIG_HOME/banana-harness/config.toml`, or the platform
   equivalent)
3. the workspace's own `.banana-harness/config.toml`

The API key is never read from a config file — only from `AI_API_KEY` in the environment.

### Running commands in a container

By default `bash` runs on your machine. To run each command in its own container instead, add this
to your **user** config (a workspace's own config cannot set it, and the desktop app lists such a
setting as ignored in the trust dialog):

    [tools.bash]
    runner = "container"

    [tools.bash.container]
    image = "banana-harness-dev"   # required: an image already on this machine, never pulled
    network = "none"               # "none" (default) or "bridge"
    memory = "2g"                  # optional: memory limit, swap included
    pids_limit = 256               # optional
    tmp_size = "256m"              # optional: size of the writable /tmp

Only `bash` is containerized; the file tools and the harness stay on the host, and no API key or
other environment variable enters the container. With Docker stopped or the image missing, commands
fail with what to do (such as `docker pull`); they never fall back to the host. The desktop app
shows the effective setting under Settings, Command execution (read-only), and marks the title bar
when a container is configured.

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

The desktop app has three keys, listed in its Settings: `Enter` sends the message, `Shift+Enter`
inserts a newline, and `Esc` closes a dialog that can be dismissed (the undo and trust dialogs).
Approvals and questions need an answer and ignore `Esc`.

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
make test    # offline suite (plus the live smoke when AI_API_KEY is set); also the desktop app's unit tests once `make gui-setup` has run
make lint    # cargo fmt --check, and clippy with warnings denied; also the desktop app's checks once `make gui-setup` has run
make gui-e2e # the desktop app's end-to-end suite against a fake provider (needs a display; CI uses xvfb on Linux)
```

Tests live in each crate's `tests/` folder. `scripts/live_smoke.sh <deepseek|qwen>` runs the
live smoke test standalone (needs `AI_API_KEY`); `make test` wires it in automatically when the
key is present.
