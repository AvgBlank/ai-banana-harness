# Guardrail policy

Type: grilling
Status: resolved
Blocked by:

## Question

What guardrails does the harness enforce so the agent can run autonomously on the evaluator's machine without per-step approval?

Starting proposal to grill:
- file tools confined to the workspace, with symlink escapes blocked
- a `bash` denylist: `sudo`, destructive commands outside the workspace, `git push`, pipe-to-shell installs
- per-command timeouts and output caps
- step, token and wall-clock budgets per turn
- a git checkpoint before the first edit
- redaction of `AI_API_KEY` from transcripts, logs and tool output
- how a blocked action is reported back to the model and to the TUI

## Answer

The user asked (2026-09-27) for recommended answers, with opencode as the reference where there's a gap. The full detail is in [spec.md](../spec.md) §7.

- **Permission model** (opencode's): each tool is `allow`, `ask` or `deny`. `bash` and `edit` accept pattern maps where the last match wins, and MCP tools match on name patterns. The committed default config is **autonomous**: everything is `allow` except the denylist below. There's no separate "evaluation mode".
- **Workspace confinement:**
  - File tools canonicalise every path, symlinks included, and reject anything outside `WORKSPACE`, apart from the harness's own spill directory.
  - Reading `.env*` is denied by default (opencode's default).
  - `bash` runs with cwd set to the workspace.
- **`bash` denylist:** `sudo`, `su`, `rm -rf` of `/` or `~`, `git push`, piping a download into a shell (`curl|wget … | sh|bash`), `mkfs`, `dd`, `shutdown`/`reboot`, and `chmod -R 777 /`.
- **`bash` limits:** a 120 s default timeout (at most 600 s) that kills the whole process group, and output capped at 30 KB. Anything past the cap goes through large-response handling.
- **Budgets:**
  - 100 LLM calls per turn (the TS AgentSpec default)
  - 30 minutes of wall-clock time per turn by default
  - no token cap beyond compaction
- **Doom-loop check:** the same tool with the same arguments three times in a row injects a warning to the model. A fifth time ends the turn with an error.
- **Git checkpoint:** before the first mutating tool call in a turn, if the workspace is a git repo, snapshot it with `git stash create` and store the ref. `/undo` restores it.
- **Secrets:**
  - The value of `AI_API_KEY` is redacted to `[REDACTED]` in anything persisted, displayed, or returned by a tool.
  - `AI_API_KEY` is removed from the environment of `bash` subprocesses.
- **Blocked actions:** the model gets the tool result `{"error":"Blocked by guardrail: <rule>"}`, and the TUI shows a red "blocked" badge.
- **`ask` in interactive use:** an in-turn prompt offering Allow once, Allow for session, or Deny.
