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

**Status:** ready-for-agent

- [ ] Seam-2 tests: the sidebar renders at 120 columns and is hidden at 90 columns; the summary card renders after a turn-done event; the help overlay renders.
- [ ] Manual check: the look reads as polished in a real terminal, on both a truecolor and a 256-colour terminal.
