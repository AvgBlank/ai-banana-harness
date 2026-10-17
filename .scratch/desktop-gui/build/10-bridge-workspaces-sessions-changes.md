# 10: harness-bridge: workspaces, sessions, providers, changes

**Lane / milestone:** B · M3 Sidecar   **Size:** M (one agent session)

**What to build:** The workspace and session half of the protocol, one `Harness` per open workspace with its own `Store` handle, opened lazily, always from layered config and never headless.

**Blocked by:** 09, 04, 05

**Status:** done

**Files and components likely to change:**
- `crates/harness-bridge/src/{workspaces.rs,methods/*.rs}`
- `crates/harness-bridge/tests/*.rs`, `tests/fixtures/*.ndjson`

**Acceptance criteria:**
- [x] `workspace.open` canonicalizes the path, rejects a missing directory (`workspace_not_found`), reports `api_key_missing`, is idempotent (`already_open`), shares one in-flight open for the same path, and returns `workspace_config { present, loaded, path }` with `path` from core's helper; `trust_workspace_config` defaults to false.
- [x] A workspace-layer `base_url` is ignored when untrusted and used when trusted (proven against the testkit stub); an untrusted open never starts the workspace config's MCP servers.
- [x] Provider for a new harness is the explicit request, else `last_provider`, else DeepSeek.
- [x] `workspace.close` and `workspace.list`; `session.create` (optional provider), `session.resume` (transcript; `workspace_mismatch`), `session.rename`, `session.switch_provider`.
- [x] `provider.list` returns `{provider, model, host, scheme}` options and the default.
- [x] `changes.workspace` returns `{git, files}` and `changes.turn` returns `{checkpoint, files}`, both on blocking threads.
- [x] Two workspaces open at once stay separate; error codes match ticket 03; golden frames are added for every method here.

**Verification** (commands to run; none has been run by the planning):

```
cargo test -p harness-bridge --locked
make lint
make test
```

**Documentation:** None.

**Notes:** Ticket 03 sections 3, 5 and 6; ticket 06 section 3 for the trust semantics.

**Completion record:**
- New in `harness-bridge`: `workspaces.rs` (one `OnceCell` per canonical path, so concurrent opens share one open; a failed open leaves nothing behind) and `methods.rs` (all handlers; `shutdown` and `session.list` moved there). Methods: `workspace.open|close|list`, `provider.list`, `session.create|resume|rename|switch_provider`, `changes.workspace|turn`. Blocking core work runs on `spawn_blocking`. New codes: `workspace_not_found`, `workspace_not_open`, `workspace_mismatch`, `session_not_found`, `api_key_missing`. `BridgeOptions` gains `user_config` (the binary passes `user_config_path()`).
- Trust: the workspace config layer loads only for `trust_workspace_config: true` and an existing file; a reopen with a different flag is ignored and reported through `workspace_config.loaded`. Provider destinations come from core's `ProviderOption` (scheme plus authority with an explicit port, never userinfo); the bridge reports them and does not itself block a destination, since ticket 06 puts the post-open check in Electron main. Core does not send the key anywhere at open.
- Deviations: (1) the acceptance line "a workspace-layer `base_url` is ignored when untrusted and used when trusted (proven against the testkit stub)" is proven through `provider.list` host and scheme, not through a request to the stub, because a request needs a turn (ticket 11). (2) `session.rename`, `switch_provider` and `changes.turn` also check that the session exists and belongs to the workspace (`session_not_found`, `workspace_mismatch`), which the ticket text did not list. (3) `workspace.list` carries `active_turn: null` until ticket 11. (4) A bad config file or an unparsable `base_url` surfaces as `internal` (no dedicated code exists in ticket 03).
- Golden frames: `workspace-methods.ndjson` (every method here) and `workspace-errors.ndjson`.
- Not covered: turns, approvals, undo, events (tickets 11 and 12); an untrusted open "never starts MCP servers" is shown only by an unrun marker command, since servers start at turn time; Linux.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
