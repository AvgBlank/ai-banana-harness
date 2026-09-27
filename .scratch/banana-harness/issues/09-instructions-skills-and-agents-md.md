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

**Status:** done

- [x] Seam-1 test: a golden snapshot of the system prompt the fake server receives for a workspace with an `AGENTS.md` and one skill.
- [x] Seam-1 test: a workspace config overrides the user config, which overrides the default (for example, the model name sent to the fake server).
- [x] Seam-1 test: invalid skill names and skills past 50 are skipped with a warning, not a crash.

## Notes

For tickets 07, 10, 13 and later.

**Where it lives:** `crates/harness-core/src/instructions.rs` (the prompt builder) and
`crates/harness-core/src/skills.rs` (the skill loader), both private modules; only
`Skill`/`LoadedSkills` are re-exported from the crate root. `TurnRunner::execute` in `harness.rs`
is the sole call site: it reads `<workspace>/AGENTS.md`, calls `skills::load_skills`, eprintln!s
any warnings, and calls `instructions::system_prompt` to build the first `system` message. This
happens once per turn today (there is still only one LLM call per turn); the loop ticket should
build the prompt once per thread and reuse it across iterations rather than per LLM call.

**Public seam:** `instructions::system_prompt(SystemPromptInput)` is a plain function, not a
generic tree builder like TrueForge's `InstructionBuilder` — the shape is fixed and known ahead of
time, so `section`/`section_cdata`/`join` are small private helpers (`<tag>\nbody\n</tag>`, empty
body dropped) rather than a reusable builder type. `SystemPromptInput.root: bool` and the
`identity`/`SUB_AGENT_IDENTITY` branch already exist so ticket 07 can pass `root: false` for a
subagent thread; nothing else currently sets it to `false`.

**Deferred tools (ticket 10):** no section is emitted at all yet (not even an empty one) — there is
no MCP config or deferred-tool registry to summarize. Ticket 10 should add a
`deferred_tools_section(...)` alongside `skills_section`/`workspace_section` in
`instructions.rs`, called from the same `capabilities_body` array in `system_prompt`, gated on
whatever non-preloaded MCP tools exist (empty → dropped, same pattern as skills).

**Subagents (ticket 07):** gated on `config.agent.subagents: bool` (added to `AgentConfig`,
default `false` in `rust/config/default.toml` since the `create_sub_agent` tool doesn't exist yet).
The `<sub-agents>` section text in `instructions.rs` (`SUBAGENTS_TEXT`) already describes the
`create_sub_agent` tool and the five-way parallelism limit for when ticket 07 lands and can flip
the default (or wire the flag through more precisely, e.g. only true once the tool is registered).

**Skill loader:** `skills::load_skills(paths: &[String], workspace: &Path) -> LoadedSkills` scans
`<path>/*/SKILL.md` for each configured path (workspace-relative, `~`-relative, or absolute),
sorted for determinism, parsing a minimal `name:`/`description:` frontmatter (no nested YAML).
Name validation is a manual character check (`^[A-Za-z0-9._-/:]+$`, written by hand rather than
pulling in a `regex` dependency for one predicate). Invalid names and everything past the 50th
valid skill are dropped into `LoadedSkills.warnings`, which the caller eprintln!s — there is no
logging crate in the workspace yet, so this is the whole "warning" mechanism.

**AGENTS.md discovery:** exactly `<workspace>/AGENTS.md` (not searched upward or in
subdirectories); `.ok()` on the read, so a missing or unreadable file is silently `None`, not an
error.

**Config layering (`Config::load`, in `config.rs`):** `ConfigPaths { user, workspace }`
(`Option<&Path>` each) are deep-merged as `toml::Value` tables over the embedded default — a
missing overlay file is skipped, a malformed one is an error. `main.rs`'s `headless` now calls
`Config::load` with the real paths (`~/.config/banana-harness/config.toml` via `etcetera`, and
`<workspace>/.banana-harness/config.toml`) instead of `Config::embedded_default()`.
`Config::embedded_default()` itself is untouched and still used by most seam-1 tests that don't
care about layering. Added `AgentConfig.subagents: bool` and a new `SkillsConfig { paths:
Vec<String> }` section (`[skills].paths`, defaulting to `[".banana-harness/skills",
"~/.config/banana-harness/skills"]`, matching the design-reference spec) — both are part of the
merged/deserialized `Config`, so a workspace or user overlay can override either.

**Test infra:** `tests/common/mod.rs`'s `TestHarness::open` now clears `config.skills.paths` before
opening, so tests never depend on (or get polluted by) whatever real directories exist on the
machine running them. Two new constructors were added for this ticket:
`TestHarness::open_configured(provider, |cfg| ...)` (mutate the embedded-default config before
opening — used for the skills tests) and `TestHarness::open_layered(provider, user_toml,
workspace_toml)` (exercises `Config::load`'s real merge through the fake-server request).
