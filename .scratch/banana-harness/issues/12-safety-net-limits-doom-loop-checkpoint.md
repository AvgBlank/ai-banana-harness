# 12: Safety net: limits, doom loop, checkpoint and undo

**What to build:**

**Turn limits:** per-turn limits of 100 LLM calls and 30 minutes of wall clock, both configurable. Hitting a limit ends the turn with a clear error.

**Doom loop:** three identical consecutive tool calls inject an internal warning to the model; a fifth ends the turn.

**Checkpoint:**
- Before a turn's first mutating call in a git workspace, record a stash-style snapshot ref in the database.
- `/undo` restores the workspace to it and removes the files the turn created.
- The TUI confirms the undo.

**Blocked by:** 04

**Status:** ready-for-agent

- [ ] Seam-1 test: a model that keeps calling tools stops at the configured iteration limit, with an error.
- [ ] Seam-1 test: repeated identical calls produce the warning after three and end the turn after five.
- [ ] Seam-1 test: after a turn edits one file and creates another, undo restores the edited file and removes the created one.
- [ ] Seam-1 test: in a non-git workspace, no checkpoint is taken and undo reports that nothing is available.
