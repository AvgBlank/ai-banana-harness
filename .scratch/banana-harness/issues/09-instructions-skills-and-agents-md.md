# 09: Instructions, skills and workspace AGENTS.md

**What to build:** the system prompt, as nested XML sections with empty sections dropped:

- identity
- capabilities:
  - internal-message rule
  - workspace rules, including the workspace path
  - skills
  - deferred-tools instructions (empty until 10)
  - subagents (when enabled)
- user instructions, on the root thread only: the config instructions plus the workspace `AGENTS.md` when it exists

**Skill loader:**
- Scans the configured skills paths for `SKILL.md` frontmatter (name, description).
- Name rules and a limit of 50 skills.
- Renders TrueForge's skill blocks with absolute paths.

**Config layering:** the committed default, then the user config, then the workspace config, with later layers winning.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Seam-1 test: a golden snapshot of the system prompt the fake server receives for a workspace with an `AGENTS.md` and one skill.
- [ ] Seam-1 test: a workspace config overrides the user config, which overrides the default (for example, the model name sent to the fake server).
- [ ] Seam-1 test: invalid skill names and skills past 50 are skipped with a warning, not a crash.
