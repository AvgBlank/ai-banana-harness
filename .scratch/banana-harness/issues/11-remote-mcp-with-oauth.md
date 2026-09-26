# 11: Remote MCP with OAuth

**What to build:** remote MCP servers over streamable HTTP, with headers that support `{env:VAR}`.

**OAuth, for servers marked oauth:**
1. The browser opens with the platform opener.
2. A random localhost port receives the callback.
3. An MCP-auth-required event shows the URL in a TUI modal.
4. The tokens come back to core, which persists them in SQLite and reuses them on later runs.

A 401 without OAuth fails clearly.

**Blocked by:** 10

**Status:** ready-for-agent

- [ ] Seam-1 test against an in-test streamable-HTTP MCP server with a static bearer header: tools are listed and called.
- [ ] Seam-1 test: persisted OAuth tokens are loaded and sent on the next run, with no auth event.
- [ ] Seam-2 test: the MCP auth modal renders the URL.
- [ ] Manual check: the OAuth login completes against one public OAuth-protected MCP server.
