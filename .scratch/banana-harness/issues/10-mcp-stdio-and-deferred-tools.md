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

**Status:** done

- [x] Seam-1 test with a fixture stdio MCP server: a preloaded tool is called directly and returns its result.
- [x] Seam-1 test: a non-preloaded server is reached through list-tools → call-tool, and isn't started until then.
- [x] Seam-1 test: an MCP tool denied by a name pattern is blocked.
- [x] Seam-1 test: an MCP error result reaches the model as an error.

## Notes

For tickets 11 (remote MCP), 13 (sidebar) and for reconciling with 04/07 (parallel, same base).

**Boundary (`harness-mcp/src/lib.rs`):** `McpServerConfig { transport: McpTransport, preload,
description }` (`:17`) deserializes one `[mcp.<name>]` table; `McpTransport` is
`#[serde(tag = "type")]` with only `Local { command, env }` — ticket 11 adds a `Remote` variant
there, and `preload`/`description` stay shared via `#[serde(flatten)]`. `McpClient::connect(config,
cwd)` (`:63`) spawns the child with rmcp's `TokioChildProcess` (cwd = workspace, stderr → null so
the server can't scribble over the TUI) and does the handshake; `list_tools()` (`:84`) pages with a
`HashSet` of seen cursors (any repeat ends the listing, which also covers A→B→A cycles);
`call_tool(name, args) -> CallOutcome { content, is_error }` renders via `render` (`:123`): error →
`{"error": text}`, else `structuredContent` JSON, else text parts joined with `\n` (JSON-encoded
content when there's no text). `expand_env` (`:149`) is pub so ticket 11 can reuse it for headers;
it runs at connect time, and an unset variable is an error (not an empty string). No rmcp type
crosses the crate boundary.

**Fixture server:** hand-written newline-delimited JSON-RPC (`harness-mcp/tests/fixture/
mcp_fixture.rs`, serde_json only), declared as an `[[example]]` so release builds never ship it.
`cargo test --workspace` builds examples, and both `harness-mcp/tests/stdio.rs` and
`harness-core/tests/mcp.rs` find it at `<target>/<profile>/examples/mcp_fixture` relative to
`current_exe()` (a clear panic tells you to run `--workspace` if it's missing, e.g. with
`cargo test -p harness-core`). Two-page `tools/list`; knobs `FIXTURE_STARTED_FILE` (appends a
line per start — how the lazy-start test proves a server wasn't spawned) and
`FIXTURE_REPEAT_CURSOR=1`. Tests use `{env:CARGO_PKG_NAME}` (set by cargo at runtime) to exercise
expansion without mutating the test process environment.

**Registry (`harness-core/src/mcp.rs`):** `McpRegistry` (`:34`) lives in `Inner` for the harness's
lifetime, so a server connected in one turn is reused by later ones. Each server holds a
`tokio::sync::OnceCell<Arc<Connection>>` (single-flight connect + tool list cached once; a failure
isn't cached and is retried by the next call) and `last_error` for status.
`preloaded_tools()` (`:108`) connects preload servers (a failure is `eprintln!`ed and skipped, like
skill warnings, rather than failing the turn). `call()` (`:157`) runs the permission check *before*
connecting, so a denied deferred call never starts its server. `Harness::mcp_status()` →
`Vec<McpServerStatus { name, state: Idle | Connected{tools} | Failed(msg) }>`.

**Tool registry/dispatch (`harness-core/src/tools.rs`):** `TurnTools::build` (`:104`) is called
once per turn (`harness.rs:273`, under the cancel `select!` since a preload connect can be slow),
producing the wire `definitions` and a `HashMap<name, ToolSource>` where `enum ToolSource { Host,
Mcp{server, tool}, Deferred(MetaTool) }` (`:16`). Name pool order: host tools, then
`list_tools`/`get_tool_info`/`call_tool` (only when at least one server is non-preload), then
preload MCP tools (servers by name, tools by name), all through `harness_tools::sanitize_name` with
one `used` set — e.g. a fixture `read_file` becomes `read_file1`. MCP descriptions get
`mcp server: {name}\n`, schemas go through `strip_schema`. `TurnTools::call` (`:151`) routes by
name; host calls still run on `spawn_blocking`, and `harness.rs:372` now `tokio::spawn`s every call
of a round (results still recorded in `tool_calls` order). An unknown name falls through to
`harness_tools::call_by_name`, which already reports `{"error": "unknown tool ..."}` (kept instead
of the TS `"Tool X not found in tool mapping"` wording so existing behaviour is unchanged). MCP
arguments that don't parse as a JSON object become `{}` and the server validates.
`harness-tools` changes: `sanitize_name`, `strip_schema` and `ToolResult::{ok,error,blocked}` are
now `pub` so core renders MCP outcomes with the same canonical formats.

**Permission (`config.rs`):** `[permission.mcp]` is a `pattern = "allow"|"ask"|"deny"` table,
matched against the MCP tool's own name (as the server lists it; for `call_tool` the underlying
`tool_name`), last match wins (`PatternRules::resolve`, `config.rs:68`, using
`harness_tools::pattern::glob_match`). Order needs the `toml` crate's `preserve_order` feature,
now enabled in the workspace `Cargo.toml`. `ask` behaves as deny for now. A block renders exactly
like host guardrails: ``Blocked by guardrail: mcp permission `<pattern>` ``, `blocked: true`.

**Prompt (`instructions.rs:116`):** `deferred_tools_section` sits after `<workspace>` in
`capabilities_body`, as `<deferred-tools-instructions>` (TS tag name), listing
`- name: "description"` (description cut to 200 chars) or `- name`; dropped when no server is
deferred. `SystemPromptInput` gained `deferred_mcp_servers`.

**TUI:** `MainState.mcp` is refreshed from `Harness::mcp_status()` on harness open and after every
turn event; `ui.rs:239` renders `mcp: fs ● other ○ broken ✗` right-aligned in the status row
(seam-2 test in `harness-tui/tests/transcript.rs`). Ticket 13's sidebar can read the same field.

**Reconciling with ticket 04:** 04 builds the full permission engine and a `[permission]` config.
This ticket added a minimal `PermissionConfig { mcp: PatternRules }` + `PermissionAction` in
`harness-core/src/config.rs` and the toml `preserve_order` feature; at merge, fold `mcp` into 04's
engine (keep "check before connect" and "underlying tool name for `call_tool`"). Both tickets also
touch `harness-tools/src/lib.rs` visibility.
**Reconciling with ticket 07:** the dispatch merge point is `tools.rs`'s `ToolSource` and
`TurnTools::build/call` plus the spawn loop at `harness.rs:372` — `create_sub_agent` should become
another `ToolSource` variant (e.g. `Builtin`) rather than a parallel branch in `harness.rs`; its
definition joins the same `used` name pool. A child thread can reuse the same `Arc<TurnTools>`.
**Reconciling with ticket 13:** use `MainState.mcp` / `Harness::mcp_status()` for the sidebar.
**Reconciling with ticket 06:** MCP results are plain `ToolResult`s, so large-output spilling can
wrap `TurnTools::call`'s result uniformly.

**Deviations:**
- Deferred server descriptions come from a new optional `description` config key, not from the
  server: rmcp exposes `InitializeResult.server_info.description`/`instructions`, but a deferred
  server isn't connected when the prompt is built (that's the point of lazy connection), and the
  TS reference also took this text from its connector config rather than from `initialize`.
- The fixture is hand-written JSON-RPC rather than built on rmcp's `server` feature (spec §10 says
  "rmcp fixture server (dev-dependency)"): the tests need a real child process, and a Cargo
  example is the one test-only target both crates can locate; hand-rolling it also tests rmcp's
  client against an independent implementation and needs no extra rmcp features.
- `call_tool` can target any configured server (preload ones too), matching the TS reference.
- `get_tool_info` returns `{description, inputSchema}` (outputSchema dropped per the inventory);
  `get_tool_output_schema` is not implemented.
- MCP children inherit the harness environment (plus `env`); `AI_API_KEY` is not stripped for
  them (the spec only requires that for `bash`).
