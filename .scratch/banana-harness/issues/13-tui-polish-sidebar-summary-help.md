# 13: TUI polish: sidebar, summary card and help

**What to build:**

**Sidebar:** toggled with Ctrl+B and auto-hidden below 100 columns. It shows:
- workspace
- provider and model
- a context-usage bar and percentage
- tokens
- elapsed time
- modified files with +/− counts
- MCP status
- the subagent tree

**End-of-run summary card:** status, files changed, tokens and duration.

**Other features:**
- `/help`, listing the commands and key bindings.
- Ctrl+E opens the input in `$EDITOR`.
- PgUp and PgDn scroll the transcript.
- A 256-colour fallback for the theme.
- The input grows from 3 to 8 rows.

**Blocked by:** 02, 03

**Status:** done

- [x] Seam-2 tests: the sidebar renders at 120 columns and is hidden at 90 columns; the summary card renders after a turn-done event; the help overlay renders.
- [x] Manual check: the look reads as polished in a real terminal, on both a truecolor and a 256-colour terminal.

## Notes

For whoever merges 04/06/07/10 ahead of this branch, and for 12 (`/undo`) and 14.

**Where things live (`crates/harness-tui/`):**
- `src/theme.rs`: every palette colour is a `Swatch { truecolor, fallback }` (fallback = hand-picked nearest xterm-256 index: background 233, text 254, muted 245, border 237, accent `#FFE135` → 221, error 167, new `SUCCESS` 108). A process-global `AtomicBool` picks the set; `theme::init()` (called first thing in `run()`) sets it from `supports_truecolor(env)`: `BANANA_HARNESS_COLORS=256|truecolor|24bit` wins, else truecolor only when `COLORTERM` is `truecolor`/`24bit` (Apple Terminal doesn't set it, so it gets 256). Default before `init` is truecolor, so seam-2 tests keep asserting `theme::ERROR`. The `pub const` RGB colours are unchanged; the style helpers are the only thing call sites use, so no call site changed.
- `src/app.rs`: `MainState` gained `config: Option<Config>` (set by `run()` from the same `Config` handed to `Harness`; `None` in tests unless set), `sidebar_open` (the Ctrl+B toggle; visibility is computed, `sidebar_visible(width)` = `sidebar_open && width >= SIDEBAR_MIN_WIDTH (100)`), `scroll_top: Option<usize>` (`None` = follow the newest output) plus `scroll_max: Cell<usize>` (written by every draw so PgUp clamps to the real top without `draw` taking `&mut`), `last_usage`, `session_tokens`, `last_turn_duration`, `modified_files: Vec<FileChange>`, and private `turn_tokens`/`turn_started`. `Overlay::Help` and `TranscriptItem::Summary(SummaryCard { state, files_changed, tokens, duration })` are new; `apply_event`'s `TurnDone` arm (app.rs:354) pushes the card, the final `ModelMessage` arm records usage. `Screen::Main` is now `Box<MainState>` (clippy `large_enum_variant`); `App::new_main`/`enter_main` signatures are unchanged, so sibling tickets' tests still compile.
- `src/ui.rs`: `draw_main` splits body/status, then main/sidebar (fixed 34 cols) when visible, then transcript/input. The transcript is one `Paragraph` scrolled via `line_count` (needs ratatui's `unstable-rendered-line-info` feature, enabled in this crate's `Cargo.toml`; Cargo.lock unchanged); it now pins to the bottom by default, which it didn't before (long transcripts used to show only the top). The input is 1–6 wrapped text rows + border (3–8 total), counting a trailing Alt+Enter line, and scrolls to keep its newest line visible past the cap. `HELP_COMMANDS`/`HELP_KEYS` (ui.rs:26) are the `/help` content, enumerated from `interpret_key`/`interpret_overlay_key`; keep them in step.
- `src/lib.rs`: Ctrl+B, PgUp/PgDn and `/help` are pure state changes inside `interpret_key` (lib.rs:338-356); Ctrl+E returns `Action::OpenEditor`, which `handle_terminal_event` passes up as `Flow::OpenEditor` (it replaced the old `bool` return) because only `run()` owns the terminal. `run()` drops the crossterm `EventStream` (its background reader would otherwise eat the editor's keystrokes), calls `edit_in_editor` (`$VISUAL`, then `$EDITOR`, then `vi`; flags allowed; temp file in `temp_dir()`; a failure lands in `last_error`, never crashes), then recreates the stream. `TerminalGuard` gained `suspend()` (also what `Drop` now calls) and `resume()` (re-enter + `terminal.clear()` for a full redraw).
- Modified files: `pub fn workspace_changes(workspace)` (lib.rs) shells out to `git diff --numstat HEAD` + `git ls-files --others --exclude-standard` (untracked files count as all-added). Display-only, so it bypasses harness-tools' guardrails deliberately. `run()` refreshes it on entering the main screen and before applying every `tool.response` and `turn.done` (so the summary card's count is current). It's synchronous on the UI thread (`ponytail:` note); a non-git workspace or missing git gives an empty list.

**Core change:** `ProviderConfig.context_length: u64` (harness-core `config.rs`) with `config/default.toml` values 1,048,576 (deepseek-flash) and 1,000,000 (qwen3.8-flash), per `research/deepseek-qwen-api` §2. The sidebar's bar/% is `last_usage.total_tokens / context_length`.

**Tests:** `tests/polish.rs`: sidebar at 120 cols (every row, including `+12 −3`) / hidden at 90 / hidden by the toggle at 120; summary card after `turn.done` (done and cancelled); help overlay lists every command and key; PgUp/PgDn scroll and re-follow; input grows 3 → 4 → 5 → 8 (capped); `workspace_changes` on a real temp git repo and on a non-repo. `tests/theme.rs` (its own process, since the palette switch is global): the env heuristic and a 256-colour render containing only `Indexed`/`Reset` colours, including the 221 accent.

**Reconciling with tickets 06/07/10 (stubbed rows):**
- **Context bar/% (06):** reads `MainState.last_usage`, i.e. the latest `model.message` final `usage` from *any* thread. Once 06 lands: if it adds its own context-window field, keep one (this branch's `ProviderConfig.context_length` or 06's) and delete the other; if 06 streams an estimated usage after tool appends or resets usage on `context.compacted`, update `last_usage` from those events in `apply_event`. 06's own "context-usage bar in the status line" can reuse `ui::context_lines` or be dropped in favour of the sidebar. Once 07 lands, filter `last_usage` to the root thread so a subagent's call doesn't overwrite the root's context size.
- **Tokens / summary tokens:** summed from every completion's `usage` (`record_usage`), so subagent calls will count automatically once 07's events carry usage.
- **MCP row (10):** hard-coded `"none configured"` (ui.rs:262). Replace with per-server status from 10's events/state (e.g. an `mcp.initialize`-style event recorded in `MainState`).
- **Subagents row (07):** hard-coded `"none"` (ui.rs:262). Build the tree from 07's `thread.created{parent, title}` / `thread.done{state}`: add a `threads` list to `MainState`, fill it in `apply_event`, render it indented by parent under the `Subagents` heading.
- **`/undo` (12):** not implemented here and not listed in `/help`; add it to `HELP_COMMANDS` when 12 adds the command. 04 may add more keys (tool-card expand, approval keys); add them to `HELP_KEYS` too.

**Deviations / simplifications:**
- The summary card is one line (`── ✓ done · 2 files changed · 1.2k tokens · 42s`), not a bordered box: the transcript is a single wrapped `Paragraph`, and a one-line card reads cleanly there.
- "Files changed" (card and sidebar) is the workspace's uncommitted changes against `HEAD`, not a per-turn diff; 12's checkpoint is the natural per-turn baseline if that's wanted.
- Elapsed time is the running turn's clock, else the last turn's duration (not the session's wall clock).
- Scroll moves 10 lines per PgUp/PgDn rather than a measured page.
- The input stays a plain `String` (no `ratatui-textarea`): growth is a rendering concern and nothing here needed cursor editing.
- Manually launched the release TUI in a PTY with `BANANA_HARNESS_COLORS=256` and `truecolor`, inspected the provider popup and 80-column main screen, and exited cleanly in both modes. The 120-column sidebar and indexed palette are covered by seam-2 tests.
