# Exact edits to existing documents (apply in the build run)

Files that do not exist yet are created by the build run from the sibling drafts: `docs/adr/0003-desktop-app-surface.md` from `0003-desktop-app-surface.md`, `gui/AGENTS.md` from `gui-AGENTS.md`, and `gui/CLAUDE.md` containing exactly the line `@AGENTS.md`. `CONTEXT.md` and `AGENTS.md` are replaced by the sibling drafts of the same name.

`docs/system_architecture.md` and `docs/easy_architecture_guide.md` are untracked and are not touched.

## docs/adr/0001-rust-local-first-fork.md

Append one paragraph at the end (history stays intact; nothing above is edited):

```
**Update:** the "only surfaces" part of this decision is superseded by [ADR 0003](0003-desktop-app-surface.md), which adds a desktop app as a third surface. The rest of this decision stands.
```

## docs/adr/0002-in-turn-waiting.md

No edit. (ADR 0003 refers to it for interrupted turns.)

## README.md

1. Replace the opening paragraph (lines 3 and 4) with:

```
A local-first, terminal-first coding-agent harness: one binary with a ratatui **TUI** and a
**headless run**, plus a macOS **desktop app** that drives the same binary, all driving DeepSeek or
Qwen to resolve a GitHub issue in a local repository.
```

2. Insert a new section after "Evaluator quickstart" (before "Configuration"):

```
## Desktop app

The desktop app is an optional macOS window over the same engine: a session list grouped by
workspace, a live transcript with tool cards, a Changes panel, and approval dialogs. It needs
Node 24 and pnpm; the evaluator quickstart above needs neither.

    make gui-setup     # Node 24 check, locked install, fetch the Electron binary
    make gui-dev       # run the app against a debug build (AI_API_KEY comes from your shell)
    make gui-package   # macOS only: an unsigned DMG and ZIP in gui/dist/

The app reads the key from `AI_API_KEY` like everything else. Launched from a terminal it uses that
environment; launched from the Dock it also looks in your login shell, and otherwise lets you paste a
key that is kept in memory for that session only. It never stores the key.

A workspace with its own `.banana-harness/config.toml` opens in **restricted mode**: that file can
start programs, loosen guardrails and change where your key is sent, so it is ignored until you
review and trust that exact file. Trust is remembered per file content; if the file changes you are
asked again. A provider that points anywhere other than the baseline, or at a non-HTTPS address
that is not loopback, is blocked.

Version 1 is macOS only and the package is unsigned: after downloading it, macOS asks you to
confirm opening it.
```

3. Under "Development", replace the `make test` / `make lint` block with:

```
make test         # offline suite (plus the live smoke when AI_API_KEY is set); also the desktop app's unit tests once `make gui-setup` has run
make lint         # cargo fmt --check, and clippy with warnings denied; also the desktop app's checks once `make gui-setup` has run
make gui-e2e      # the desktop app's end-to-end suite against a fake provider (needs a display; CI uses xvfb on Linux)
```

   and add one sentence after the existing "Tests live in each crate's `tests/` folder" sentence: "The desktop app's tests live in `gui/tests/`; its fake provider is `crates/harness-testkit`."

(Key bindings for the desktop app are added to the README's "Key bindings" section when the build fixes them; the map lists keyboard shortcuts as not yet specified.)

## CONTRIBUTING.md

1. Replace lines 3 and 4 ("It's a single Rust binary: a ratatui TUI and a headless / run over one agent loop.") with:

```
It's a single Rust binary: a ratatui TUI and a headless run over one agent loop, plus a macOS
desktop app in `gui/` that drives the same binary as a sidecar. Read `README.md` for what it does,
```

   and keep the following lines unchanged.

2. In "Setup", after the code block, add:

```
The desktop app is optional: it needs Node 24 and pnpm (`make gui-setup`), and `make gui-dev`
runs it. See `gui/AGENTS.md` for its rules.
```

3. In the "Layout" block, add these lines (keeping the alignment):

```
crates/harness-bridge    the sidecar protocol (`banana-harness serve --stdio`)
crates/harness-testkit   test support only: the fake provider, fixtures and scenarios
gui/                     the desktop app (Electron, TypeScript)
```

   and change the `crates/banana-harness` line to `the binary (TUI, plus `run` for headless and `serve` for the sidecar)`.

4. In "Tests", append: "The sidecar protocol is tested in `harness-bridge` against scenarios from `harness-testkit`, and the desktop app in `gui/tests/` (`make gui-test`, `make gui-e2e`)."

5. In "Pull requests", change the second bullet to: "`make lint` and `make test` must pass (and `make gui-e2e` for a change that touches `gui/` or the sidecar); CI runs them on Linux and macOS."
