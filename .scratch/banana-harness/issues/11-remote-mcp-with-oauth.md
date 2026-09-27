# 11: Remote MCP with OAuth

**What to build:** remote MCP servers over streamable HTTP, with headers that support `{env:VAR}`.

**OAuth, for servers marked oauth:**
1. The browser opens with the platform opener.
2. A random localhost port receives the callback.
3. An MCP-auth-required event shows the URL in a TUI modal.
4. The tokens come back to core, which persists them in SQLite and reuses them on later runs.

A 401 without OAuth fails clearly.

**Blocked by:** 10

**Status:** done

- [x] Seam-1 test against an in-test streamable-HTTP MCP server with a static bearer header: tools are listed and called.
- [x] Seam-1 test: persisted OAuth tokens are loaded and sent on the next run, with no auth event.
- [x] Seam-2 test: the MCP auth modal renders the URL.
- [ ] Manual check: the OAuth login completes against one public OAuth-protected MCP server. **Unverified**: no interactive browser/display was available in the build sandbox (see Notes).

## Notes

For ticket 15 (cutover) and whoever runs the manual OAuth check.

**Transport (`harness-mcp/src/lib.rs`):** `McpTransport` gained `Remote { url, headers, oauth }`
(`lib.rs:~30`; `headers` and `oauth` both `#[serde(default)]`). `McpClient::connect` (`:81`) is
now `connect(config, cwd, oauth: Option<OAuthContext<'_>>) -> Result<Self>` — the third parameter
is new, so every existing call site needed updating (`harness-core/src/mcp.rs`,
`harness-mcp/tests/stdio.rs`). It dispatches to `connect_local` (unchanged stdio logic, extracted
verbatim) or `connect_remote` (`:156`), which expands `{env:VAR}` in each header value (reusing
ticket 10's `expand_env`), builds a `StreamableHttpClientTransportConfig::with_uri(url)
.custom_headers(expanded)`, and either connects directly (`oauth: false`) or hands off to
`oauth::connect` (`oauth: true`). `rustls::crypto::ring::default_provider().install_default()` is
called (idempotently) at the top of `connect_remote`, because `harness-llm`'s `ChatClient` isn't
guaranteed to run first (a remote MCP server can be the first thing to build a `reqwest::Client`,
e.g. in a test with no LLM client at all).

**OAuth (`harness-mcp/src/oauth.rs`, new file):** built entirely on rmcp's own `auth` feature
(`AuthorizationManager`, `AuthClient`, `CredentialStore`) — no hand-rolled PKCE or token exchange.
- `OAuthTokenStore` (pub trait, sync `load`/`save`/`clear(server) -> Result<...>`) is this crate's
  boundary type for persistence; `StoreAdapter` (private) wraps it as rmcp's async `CredentialStore`,
  (de)serializing rmcp's own `StoredCredentials` as one opaque JSON blob per server — so a refreshed
  or re-issued token round-trips without ever touching this crate's schema.
- `OAuthContext<'a> { server_name, token_store: Arc<dyn OAuthTokenStore>, on_auth_url: Box<dyn Fn(&str) + Send + Sync + 'a> }`
  is the caller-supplied side of the flow.
- `connect()`: builds the manager, `set_credential_store`, `initialize_from_store()` — if that
  returns `true` (a still-valid token was on disk), it skips straight to building the `AuthClient`
  and **`on_auth_url` is never called**, which is what makes "no auth event" true for a persisted
  token. Only on `false` does `log_in()` run: `resolve_metadata` (SEP-985 discovery, falling back
  to legacy `/authorize`+`/token` when a server publishes nothing), bind a `TcpListener` on
  `127.0.0.1:0` to learn the callback port before building `redirect_uri`, dynamic client
  registration (`register_client`, native `application_type`), `get_authorization_url`, then
  `on_auth_url(&auth_url)` **before** `open::that(&auth_url)` (best-effort; failure is silently
  ignored since the URL is shown either way) and before blocking on the callback — so the TUI gets
  the URL immediately, not after the user finishes logging in.
- `accept_callback`: one-shot raw HTTP GET parse (buffer read, no `Content-Length` handling needed
  since a browser redirect has no body) pulling `code`/`state` off the query string via a small
  byte-level `percent_decode` (own function, tested; reassembles decoded bytes as UTF-8 rather than
  casting bytes to `char` one at a time, so a multi-byte encoded `state` value would still round-trip
  correctly even though OAuth `code`/`state` are realistically always ASCII).
- `exchange_code_for_token` is rmcp's own method and **already calls `credential_store.save`
  internally** — nothing extra needed here to persist a fresh token.
- New deps (all pinned in `[workspace.dependencies]`): `open` (browser), `http` + `async-trait`
  (already-transitive, now direct), `oauth2` (dev-only, `default-features = false` — its own
  `reqwest`/`native-tls` stack is not wanted, only its plain data types for the test fixture).

**Persistence (`harness-store`):** new migration `migrations/0005.sql` — `mcp_oauth_token(server
TEXT PRIMARY KEY, credentials_json TEXT NOT NULL, updated_at)`, plus `Store::get_mcp_oauth_token`
/`set_mcp_oauth_token`/`clear_mcp_oauth_token` (`lib.rs`, next to `get_setting`/`set_setting`,
same upsert style). **Ticket 12 is running in parallel from the same base and may also add a
migration** — if it picks `0005.sql` too, that's an add/add filename collision at merge time;
whoever merges second should renumber theirs to the next free number and re-point its
`include_str!` in `MIGRATIONS`.

**Wiring (`harness-core`):**
- `Inner.store` changed from `Store` to `Arc<Store>` (`harness.rs`) so both the turn runner and a
  new `StoreOAuthTokens(Arc<Store>)` wrapper (implements `harness_mcp::OAuthTokenStore`, placed
  next to `Redactor`) can share one instance. Every existing `inner.store.method(...)` call site
  is untouched — `Arc<Store>` derefs transparently.
- `McpRegistry::new` (`mcp.rs`) gained an `oauth_tokens: Arc<dyn OAuthTokenStore>` parameter;
  `connection()` builds an `OAuthContext` only for a `Remote { oauth: true, .. }` server, cloning a
  new `McpAuthNotifier` (`pub(crate) type McpAuthNotifier = mpsc::UnboundedSender<(String, String)>`)
  into its `on_auth_url` closure. `preloaded_tools`/`list_tools`/`tool_info`/`call` all now take
  `&McpAuthNotifier` and thread it through to `connection`.
- **Streaming the event while the connection is still in flight** (`harness.rs`): an OAuth login
  can take a while (the user has to actually log in in the browser), so the event can't wait for
  `connect()` to return. `TurnRunner::with_mcp_auth_watch(thread_id, work: FnOnce(McpAuthNotifier) -> Fut)`
  spawns a watcher that drains the channel and `self.emit(EventKind::McpAuthRequired{..})`s each
  item as it arrives, awaits `work(tx)`, then joins the watcher (which exits once every clone of
  `tx` inside `work` has dropped, i.e. once every connection attempt in that round has finished).
  Wraps both call sites that can reach `McpRegistry::connection`: `TurnTools::build`'s preload step
  and `run_tool_calls` (host + MCP tool calls; the notifier is threaded through
  `ToolInvocation.mcp_auth`, owned — not borrowed — because a host tool call moves its
  `ToolInvocation` into its own `tokio::spawn`ed task). A subagent's own MCP calls report through
  the *root* turn's watcher (whichever thread's tool call triggered the connect), same as every
  other required-action-shaped event in this codebase.
- New event `EventKind::McpAuthRequired { thread_id, server, url }` (`event.rs`) — added to both
  `name()` and `thread_id()` match arms. **Ticket 05 is running in parallel and will also touch
  `event.rs`'s match arms** (for its own `tool.approval_required`/`tool.response_required`) —
  expect a small textual merge conflict there, trivial to resolve (both sides just add arms).

**TUI (`harness-tui`):** `Overlay` gained `McpAuth { server, url }` (`app.rs`, next to `Help`), set
by a dedicated `apply_event` arm (bypasses the subagent-thread lookup entirely, like
`ThreadCreated`/`ThreadDone` — an OAuth prompt needs the user's attention regardless of which
thread's tool call triggered it) and closed by Esc/Enter/`q` (`lib.rs::interpret_overlay_key`,
same as `Help`). Rendered by `draw_mcp_auth` (`ui.rs`, copy of `draw_help`'s centered-box
layout). **Ticket 05 will also add `Overlay` variants** (`Approval`, `Question`) — same
textual-merge-conflict expectation as above.

**Tests:**
- `harness-mcp/tests/streamable_http.rs`: a hand-rolled in-process HTTP/1.1 fixture
  (`tests/fixture/http_fixture.rs` — reused as-is by `harness-core/tests/mcp_remote.rs` via
  `#[path]`, not duplicated) proves list+call over streamable HTTP with a static bearer header,
  and that a bare 401 (no OAuth on our side) surfaces as a clear error mentioning "auth". The
  fixture is deliberately minimal: **no session id, no SSE responses** (`ponytail:` noted in the
  file) — our client's default config is stateless and JSON (not SSE) responses are accepted, so
  this is all these tests need; a future test needing session recovery or SSE would need to
  extend it. It also answers every non-POST request with a clean 404, which is what makes rmcp's
  OAuth metadata discovery (several `GET /.well-known/...` probes before falling back to legacy
  endpoints) resolve gracefully instead of erroring.
- `harness-core/tests/mcp_remote.rs`: seeds a `StoredCredentials` (built from real
  `oauth2`/`rmcp::transport::auth` types via `serde_json::to_string`, not hand-written JSON — the
  wire shape is a library-private serialization detail not worth reverse-engineering) directly
  into a second `Store` connection (`t.reopen_store()`), then runs a turn through a
  `preload = true, oauth = true` remote server and confirms: no `mcp.auth_required` event, the
  tool call succeeds, and the fixture actually received `Authorization: Bearer
  persisted-access-token` — proving the stored token round-trips end to end, not just that the
  store returns *something*.
- `harness-tui/tests/polish.rs`: `the_mcp_auth_modal_renders_the_login_url`, next to the other
  overlay test, feeding the event directly through `apply_event` (no new test helper needed).

**Unverified:** the manual check (a real OAuth login against a public OAuth-protected MCP server)
needs an interactive browser and network egress to a third-party auth server, neither available
in this build sandbox. The code path is exercised end to end except for the actual
`get_authorization_url` → browser → callback round trip against a *real* authorization server;
whoever has both should be able to point `[mcp.<name>]` at one with `oauth = true` and try it
directly — no code changes should be needed if it fails only because of that gap.
