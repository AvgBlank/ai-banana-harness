# 10: MCP over stdio, with deferred tools

**What to build:** local MCP servers configured by command and environment, with `{env:VAR}` expansion, using the official Rust MCP SDK pinned to an exact version.

**Preload servers:** tools from servers marked preload are registered directly, with the server-name prefix on their descriptions.

**Other servers:**
- They sit behind the list-tools, get-tool-info and call-tool meta-tools.
- They're summarised in the system prompt, with descriptions capped at 200 characters.
- They connect lazily: only when preloaded or when a deferred call targets them.

**Other behaviour:**
- Tool listing pages with a guard against a repeated cursor.
- MCP tools go through the permission engine by tool-name pattern.
- **In the TUI:** MCP server status appears in the status line (the full sidebar comes in 13).

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] Seam-1 test with a fixture stdio MCP server: a preloaded tool is called directly and returns its result.
- [ ] Seam-1 test: a non-preloaded server is reached through list-tools → call-tool, and isn't started until then.
- [ ] Seam-1 test: an MCP tool denied by a name pattern is blocked.
- [ ] Seam-1 test: an MCP error result reaches the model as an error.
