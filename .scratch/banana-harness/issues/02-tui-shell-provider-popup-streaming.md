# 02: TUI shell: provider popup, input and a streaming transcript

**What to build:** `make run` opens the ratatui TUI.

1. A modal popup asks the user to choose DeepSeek or Qwen, preselecting the last choice stored in settings. If `WORKSPACE` is unset, it also asks for a workspace path. If `AI_API_KEY` is unset, it shows a blocking error.
2. The user types or pastes a multi-line message (Enter sends, Alt+Enter adds a newline, bracketed paste works) and watches the reply stream into the transcript as markdown.
3. A status line shows a spinner while the turn runs.
4. Ctrl+C cancels a running turn; a second Ctrl+C quits and restores the terminal. `/quit` also exits.

Use the dark theme with the banana-yellow accent from the start, in one theme module. Rendering is throttled to about 30 fps, and deltas append without re-rendering the whole transcript.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Seam-2 test: rendering the startup state into ratatui's test backend shows the provider popup with both options and the preselected choice.
- [ ] Seam-2 test: feeding a turn's events (created, deltas, done) into the app state renders the user message and the assembled assistant reply.
- [ ] Seam-1 test: cancelling a running turn ends it as cancelled.
- [ ] Manual check: `make run` against a real key streams a reply, and the terminal is restored cleanly on exit.
