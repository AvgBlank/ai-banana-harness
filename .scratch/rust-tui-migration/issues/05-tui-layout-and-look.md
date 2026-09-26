# TUI layout and look

Type: prototype
Status: resolved
Blocked by:

## Question

What should the Banana Harness TUI look like and how should it behave, so it reads as polished during evaluation? Cover:

- the first screen, where the evaluator pastes the issue into the workspace
- the live transcript: streaming model text, reasoning, tool-call cards, file diffs, bash output
- subagent threads
- a status bar showing model, tokens, budget and elapsed time
- key bindings
- the end-of-run summary

## Answer

The user waived the hands-on prototype (2026-09-27), so this follows opencode's TUI conventions. The full detail is in [spec.md](../spec.md) §8.

- **Startup:**
  - A provider popup (DeepSeek or Qwen) that the organisers require. The last choice is preselected.
  - If `WORKSPACE` is unset, a workspace-path field in the same popup.
- **Main screen:**
  - the transcript pane
  - a sidebar that toggles with Ctrl+B and hides itself below 100 columns. It shows the workspace, provider and model, a context-usage bar, tokens, elapsed time, modified files with +/− counts, MCP status and the subagent tree.
  - a multi-line input at the bottom: Enter sends, Alt+Enter adds a newline, bracketed paste works
  - a status line with a spinner and key hints
- **Transcript:**
  - user messages
  - streamed assistant markdown
  - tool-call cards that collapse to one line (`✓ edit src/x.rs +12 −3`) and expand to show a coloured diff or the tail of bash output
  - subagent cards
  - compaction notices
  - red error and "blocked" badges
  - an end-of-run summary card showing status, files changed, tokens and duration
- **Keys:**
  - Ctrl+C cancels the turn; pressing it twice quits
  - PgUp and PgDn scroll
  - Ctrl+E opens the input in `$EDITOR`
- **Slash commands:** `/new`, `/sessions`, `/provider`, `/undo`, `/help`, `/quit`.
- **Prompts:** an approval modal and a question modal with options plus free text.
- **Theme:** one dark theme with a banana-yellow accent, truecolor with a 256-colour fallback, defined in a single module.
