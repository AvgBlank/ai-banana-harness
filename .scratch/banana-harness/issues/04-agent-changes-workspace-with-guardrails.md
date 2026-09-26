# 04: The agent changes the workspace, behind guardrails

**What to build:** the write-file, edit and bash host tools, plus the opencode-style permission engine.

**Edit:** requires an exact unique match, unless replace-all is set.

**Bash:**
- runs in a login shell, in its own process group, with cwd set to the workspace
- has a configurable timeout (120 s by default, at most 600 s) and kills the process group when it expires
- caps its output at 30 KB
- has `AI_API_KEY` stripped from its environment

**Permission engine:**
- `allow`, `ask` or `deny` per tool key.
- Pattern maps for bash and edit, where the last match wins.
- The committed default is autonomous, apart from the bash denylist and the `.env`/external-directory denies.
- In this ticket `ask` behaves as deny; 05 adds the prompt.

**Redaction:** the key value is redacted from tool output.

**In the TUI:** edit and write cards expand (Enter or Ctrl+O) to a syntax-highlighted coloured diff, and bash cards expand to the last 40 lines plus the exit code.

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] Seam-1 test: read → edit → run bash (a test command) in a temporary git repo. The file content changes on disk, and the bash result includes the exit code.
- [ ] Seam-1 test: an edit with an ambiguous `old_string` fails with a clear error.
- [ ] Seam-1 tests: each denylisted command pattern (`sudo`, `rm -rf /`, `git push`, `curl … | sh`, and so on) is blocked.
- [ ] Seam-1 test: a command that sleeps past its timeout is killed, and the result says so.
- [ ] Seam-1 test: a command that echoes the key's value returns it redacted.
- [ ] Table test (the permitted exception): the permission pattern matcher applies last-match-wins across mixed rules.
- [ ] Seam-2 test: an expanded edit card renders the diff.
