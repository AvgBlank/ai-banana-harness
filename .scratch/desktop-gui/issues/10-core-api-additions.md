# Core API additions the GUI needs

Type: grilling
Status: resolved
Blocked by:

## Question

Decide the smallest core extensions, each with one canonical owner (see [research](../research/01-core-gaps.md)):

- Transcript replay: which `Harness` read methods, and which typed shape.
- Real diffs for the Changes panel: move `workspace_changes` into core and add a hunk/patch type; delete the TUI copy.
- Rename session: one canonical title writer.
- Where serde lives for `Answer`, `UndoOutcome`, `Provider` and `Session`, without mirror DTOs.
- "Model picker": provider picker only, or grow config with a model list per provider (a user-visible config key).
- Sharing one `Store` across several per-workspace `Harness` instances, and the default provider when there is no startup popup.
- Whether per-workspace `Harness` instances need an ADR of their own.

## Answer

Resolved 2026-10-17 against the code, not just the research report. Decisions, each with one canonical owner. Nothing here is implemented yet.

### What the code check changed

- Persisted events are already the full serialized `Event` (`payload_json` is `serde_json::to_string(&Event)`, written by `TurnRunner::emit`), so replay is a plain deserialize in core.
- `tool.response` already carries a unified `diff` for `write_file`/`edit`, and it is persisted, so a turn's file edits are replayable without git. `bash` edits have no such diff.
- Sharing a `Store` across harnesses is not a blocker: `Store::open` uses WAL plus a 5 s busy timeout, and the core test helper already opens a second `Store` on the same file (`reopen`). Keep `HarnessOptions.store` by value; no signature change.
- `Store` and `Session` are already re-exported from `harness-core`, so the sidebar can list sessions before any workspace is open, with no `Harness`.
- The fake provider is an in-test HTTP stub (`crates/harness-core/tests/common`) reached through the configurable `base_url`. This feeds ticket 05 and is not decided here.

### Decisions

1. **Serde lives with the owner of each type; no mirror DTOs.**
   - In `harness-core`: derive `Serialize`/`Deserialize` on `Answer` (externally tagged, snake_case: `allow`, `allow_for_session`, `deny`, `{"text": "..."}`), `UndoOutcome` and `Provider` (lowercase, matching `as_str`).
   - In `harness-store`: derive `Serialize` on `Session` (the workspace-pinned `serde` dependency joins that leaf; leaves still don't depend on each other). Add `Deserialize` only if a consumer needs it.
   - Ticket 03 owns the envelope that wraps these; it adds no copies of them.
2. **Transcript replay.** New `Harness::transcript(session_id) -> Result<Vec<TurnRecord>>`.
   - `TurnRecord` is a core type: `{ turn_id, seq, input, state: TurnState, error: Option<String>, usage: Option<Usage>, events: Vec<Event> }`.
   - Core builds it from `Store::turns` and `Store::events`, deserializing each `payload_json` into `Event`. Rows that fail to parse fail the call with context naming the turn and event, never silently skipped.
   - Deltas are never stored, so replay shows final `model.message` content. The GUI renders replayed and live turns from the same event kinds.
   - Resume then means `transcript` plus ordinary `start_turn`, no new turn API.
   - ponytail: returns the whole session; page by turn when long sessions slow resume.
3. **Real diffs.** Move `workspace_changes` out of `harness-tui` into `harness-core` and delete the TUI copy and its `git` helper.
   - Core owns `FileChange { path, status (Added | Modified | Deleted | Renamed), added, removed, patch: Option<String>, truncated: bool }`. It replaces `harness-tui`'s `FileChange`; the TUI uses core's.
   - Two sync methods on `Harness`, both reading its fixed workspace (so correct per workspace under one-`Harness`-per-workspace):
     - `workspace_changes()` against `HEAD` plus untracked files, now with unified patches. This is the "all changes" scope and includes `bash` edits.
     - `turn_changes(session_id)` for the session's last turn, compared with that turn's checkpoint (`stash_ref` or `head_sha`), with files untracked now but not at checkpoint time as additions. Empty when the turn took no checkpoint (no mutation, or not a git repo).
   - Git plumbing is shared with `checkpoint.rs` (one `git` helper owner); update the `checkpoint.rs` doc that points at the TUI.
   - Non-git workspace or missing `git` returns an empty list, as today. Binary files have counts `0`, no patch. Patches over 256 KiB per file are cut with `truncated: true` (ponytail: raise or stream if it bites).
   - Callers run these on a blocking thread; the sidecar uses `spawn_blocking` (the TUI's existing ponytail note moves with the code).
   - The per-edit `diff` string on `tool.response` stays as the inline tool-card diff. The Changes panel does not parse it.
4. **Rename session.** One canonical title writer in `harness-store`: `set_session_title(id, title)` that overwrites. `set_session_title_if_unset` is deleted and its one caller (the first turn's auto-title) uses the new method guarded by `title.is_none()` inside core, so no second writer remains. Core exposes `Harness::rename_session(id, title)`, which trims, rejects empty, caps at the existing 50 chars, and redacts like the auto-title. No migration needed.
5. **Provider/model options (v1).** Keep one fixed model per provider.
   - New `Harness::providers() -> Vec<ProviderOption { provider: Provider, model: String }>`, in the fixed order DeepSeek, Qwen, built from config. A session already carries `provider` and `model`; the UI shows both.
   - `switch_provider(session_id, Provider)` stays as is. No multi-model config, no discovery.
   - Extensibility: the protocol identifies an option by its `provider` string and leaves room for an optional `model` field later. No core change for that now.
   - `compaction_threshold(provider)` stays for the context bar.
6. **Default provider without a startup popup.** Add `pub fn last_provider(store: &Store) -> Option<Provider>` in core reading `LAST_PROVIDER_SETTING`; the TUI's inline read (`crates/harness-tui/src/lib.rs`, startup preselection) switches to it. The sidecar resolves a new `Harness`'s provider as: explicit request, else `last_provider`, else DeepSeek.
7. **Workspaces and stores.** No core change.
   - The sidecar owns a map workspace to `Harness`, opened lazily and reused.
   - Each `Harness` gets its own `Store::open` on the same database file; one extra `Store` lists sessions for the sidebar before anything is open.
   - Per-workspace `Harness` instances are recorded in ADR 0003 (ticket 09), not in a separate ADR: nothing in core changes, so nothing is hard to reverse.
8. **Not added.** No session delete, no pagination, no cross-workspace undo (undo is per `Harness`, so correct per workspace), no change to `Harness::open`.

### Tests (core seam, in `crates/*/tests/`, using the in-test fake provider)

- `harness-core/tests/transcript.rs`: after two turns (one with a tool call, one cancelled) `transcript` returns both turns in order with their `Event`s, deltas absent, state/usage/error right, and a reopened `Harness` returns the same; a corrupted event row makes it error naming the turn.
- `harness-core/tests/workspace_changes.rs` (extend): modified, added (untracked), deleted and binary files give the right status, counts and patch; a non-git workspace gives an empty list; oversized patch sets `truncated`.
- `harness-core/tests/checkpoint.rs` (extend): `turn_changes` after a turn that edits via `write_file` and via `bash` lists both, and excludes edits made before the turn; empty after a read-only turn.
- `harness-core/tests/sessions.rs` (extend): `rename_session` renames, trims, rejects empty, caps length; a later turn does not overwrite a renamed title; `providers()` lists both with their configured models; `last_provider` reflects `switch_provider`; two harnesses on one database file run turns concurrently without a locking error.
- `harness-core/tests/approvals.rs` (extend): `Answer`, `UndoOutcome` and `Provider` round-trip through JSON with the documented shapes.
- `harness-store/tests/`: `set_session_title` overwrites; `Session` serializes.
- `harness-tui/tests/polish.rs`: the existing `workspace_changes` test moves to core; TUI tests keep passing against core's `FileChange`.

### Acceptance criteria for the build run

- All of the above merged with `make lint` and `make test` green; no duplicate definitions; no unused code left (the TUI `workspace_changes`, its `git` helper, its `FileChange` and `set_session_title_if_unset` are gone).
- README `/help` and `CONTEXT.md` stay in step for any user-visible change (none expected in the TUI).
- The sidecar (ticket 03) can implement every v1 core feature with these APIs plus existing ones, never reading store rows itself.

### Effects on other tickets

- 03: the method list is now concrete: list sessions (`Store`), open workspace, create/rename/resume session, transcript, start turn, cancel, reply, undo, providers, switch provider, workspace changes, turn changes, MCP status.
- 05: the fake provider is an HTTP stub behind `base_url`; decide how the sidecar and Playwright reach one without a production shim.
- 07: Changes panel scopes are "Last turn" (`turn_changes`) and "Workspace" (`workspace_changes`).
- 09: ADR 0003 also records one `Harness` per workspace.

### Amendment (from ticket 03, 2026-10-17)

`TurnRecord.state` is `Option<TurnState>`, not `TurnState`: a turn that never finished (the process died) is stored as `running`, so `None` means interrupted. The `transcript` test gains a case with a turn row left `running`. Core also gains `user_config_path()`, `workspace_config_path(&Path)` (extracted from `main.rs`) and a public `Redactor`; see [Sidecar placement and wire protocol](03-sidecar-placement-and-protocol.md#core-changes-this-design-needs-in-addition-to-ticket-10).

### Amendment (from ticket 06, 2026-10-17)

`ProviderOption` is `{ provider, model, host }`, where `host` is the host of the effective `base_url` (no path, query or credentials; userinfo is stripped). The `providers()` test also asserts that a workspace-layer `base_url` override is reflected in `host` and that credentials in a URL never appear. Reason: the GUI blocks a trusted workspace whose config redirects where the API key goes (see [Electron main/preload design](06-electron-process-and-security.md#3-workspace-trust)).

### Amendment (from ticket 07, 2026-10-17)

- `workspace_changes()` returns `WorkspaceChanges { git: bool, files }` and `turn_changes(session_id)` returns `TurnChanges { checkpoint: bool, files }`.
- New durable event `turn.undone { turn_id }`; `Harness::undo` records it in the undone turn's log, returns it, and treats a last turn that already has it as `NothingToUndo`. The TUI's exhaustive event matches get a no-op arm. The doc on `EventKind::ModelMessage` is corrected: the start marker is streamed only, never persisted. Tests are listed in [the renderer ticket](07-renderer-state-and-changes-panel.md#amendments-to-earlier-tickets).

### Amendment (from ticket 05, 2026-10-17)

`ProviderOption.host` is the URL **authority** (host plus an explicit non-default port, no userinfo) and `ProviderOption` gains `scheme`. Reason: host alone cannot distinguish two loopback stubs, and would let a workspace config redirect the key to another port or downgrade to `http://` on the same host name. The `providers()` test also asserts that a differing port and a differing scheme are reflected, and that userinfo never appears. See [the fake-provider ticket](05-fake-provider-for-smoke-tests.md#amendments-to-earlier-tickets).
