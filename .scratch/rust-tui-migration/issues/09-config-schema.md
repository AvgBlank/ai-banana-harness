# Config schema

Type: grilling
Status: resolved
Blocked by: 07, 08

## Question

What is the on-disk config? Cover:

- file format and locations: user-level and per-workspace
- the schema for model/provider (including temperature and seed), agents, MCP servers (stdio and remote) and skills
- how the product name drives directory names so a rename is a one-line change
- what the committed evaluation config looks like

## Answer

The user asked (2026-09-27) for recommended answers, with opencode as the reference. The full detail is in [spec.md](../spec.md) §5.

- **Format:** TOML.
- **Precedence** (later wins):
  1. `rust/config/default.toml`, committed and embedded in the binary with `include_str!`. This satisfies the brief's "model configuration clearly defined".
  2. `~/.config/banana-harness/config.toml`
  3. `<workspace>/.banana-harness/config.toml`
- **Sections:**
  - `[providers.deepseek|qwen]`: `base_url`, `model`, `temperature`, and `seed` for Qwen only
  - `[agent]`: `instructions`, `max_iterations`, `subagents`, `max_turn_minutes`
  - `[permission]` (as in the guardrail policy)
  - `[mcp.<name>]`: `type = "local"` with `command` and `env`, or `type = "remote"` with `url`, `headers` and `oauth`
  - `[skills] paths`
  - `[tools]`: `bash_timeout_secs`, `output_max_bytes`
- **Agents:** v1 has **one agent**, defined by the config, plus dynamic subagents. Named agents are deferred.
- **Instructions:** `AGENTS.md` at the workspace root is appended to the instructions when it exists (opencode's default).
- **Secrets are never stored in config.** The key comes only from `AI_API_KEY`. Remote MCP headers can use `{env:VAR}`.
- **Renaming:** `APP_NAME` and `APP_SLUG` constants in `harness-core` set every directory name. A rename touches those two constants plus the binary name in its `Cargo.toml`.
