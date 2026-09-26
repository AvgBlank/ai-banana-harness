# 03: The agent reads and searches the workspace

**What to build:** the tool loop. The model can call the read-file, glob and grep host tools. The thread runs the calls in parallel, appends the results, and calls the model again until it answers without tool calls.

**Tool rules:**
- Paths are confined to the workspace, with symlinks resolved.
- Reading `.env*` is denied.
- A blocked call returns "Blocked by guardrail: <rule>" to the model, and the event is marked blocked.
- Tool names are sanitised, and tool parameter schemas keep only properties and required.

**In the TUI:** each call shows as a collapsed one-line tool card with its outcome, and blocked calls get a red badge.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] Seam-1 test: the model reads a file in a temporary git workspace, then answers. The tool response event carries numbered lines.
- [ ] Seam-1 tests: a path outside the workspace, a symlink that escapes it, and a `.env` read are each blocked with the guardrail error, and the turn continues.
- [ ] Seam-1 test: glob and grep respect `.gitignore`.
- [ ] Seam-1 test: two tool calls in one assistant message both execute, and both results are appended before the next model call.
- [ ] Seam-2 test: a transcript containing a tool card renders its one-line summary.
