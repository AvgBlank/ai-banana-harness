# Core gap inventory for the v1 GUI

Produced by a research subagent (2026-10-17). Paths are relative to the repo root.

## Constraints from AGENTS.md / CONTEXT.md

- Leaf crates (`harness-llm`, `-tools`, `-mcp`, `-store`) must not depend on each other; core depends on all four; a GUI crate sits beside `harness-tui` and depends on core only.
- One canonical owner per type/helper; code made unused is deleted in the same change; multiple same-typed params take an options struct; shipped migrations are never edited.
- `CONTEXT.md` calls the TUI "the only interactive interface" and says to avoid "GUI"; a change needs a `CONTEXT.md` update and an ADR.
- Each new core API needs a `tests/` case at the core seam.

## Per-feature status

| v1 feature | Status | Evidence |
|---|---|---|
| Workspace-grouped session list | needs-change | `Harness::sessions()` (`crates/harness-core/src/harness.rs:185`) returns all sessions by `updated_at DESC`; `Session.workspace` is a plain string (`crates/harness-store/src/lib.rs:38`). Grouping is client-side; no workspace filter. |
| Resume with transcript replay | missing | Store has `turns()` (`lib.rs:264`) and `events(turn_id)` (`:435`); `Harness` exposes neither. The TUI resumes with a blank transcript (`crates/harness-tui/src/app.rs:428-435`). Deltas are never persisted, so replay uses final `model.message` plus tool events. |
| Send turn | exists | `Harness::start_turn` (`harness.rs:234`). |
| Stream events | exists | `TurnHandle.events` (`harness.rs:96`); `Event`/`EventKind` already serde (`event.rs:5-14`). |
| Approvals/questions | exists, serde gap | `TurnHandle::reply` (`harness.rs:111`); `Answer` has no serde (`harness.rs:35`). |
| Cancel | exists | `TurnHandle::cancel()` (`harness.rs:105`). |
| Undo | exists, serde gap | `Harness::undo` (`harness.rs:221`); `UndoOutcome` not serde (`:128`); restores into the harness's single workspace (`:229`). |
| Model/provider picker | needs-change | `switch_provider` exists (`harness.rs:196`); `Provider` is a closed two-variant enum without serde (`config.rs:116-147`); no list-providers/models API; each provider has exactly one fixed model, so "model picker" can only mean provider picker unless config grows a model list. |
| Changes panel with real diffs | missing | `workspace_changes` lives in `harness-tui` (`lib.rs:631`), counts only (`FileChange{path, added, removed}`, `app.rs:109`), no hunks. Core's checkpoint diff is display-only and private. |
| Rename session | missing | Only `set_session_title_if_unset` exists (`lib.rs:208`). |

## Extra gaps

1. **Cross-workspace:** `Inner.workspace` is fixed (`harness.rs:86`) and used for tools, AGENTS.md/skills, checkpoints, undo, MCP registry. `start_turn` never reads a session's own workspace, so one `Harness` per workspace is the low-risk shape.
2. **Store ownership:** `Harness::open` takes the `Store` by value; it is one `Mutex<Connection>`. Several harnesses plus list calls contend; the TUI uses `store.take()` (`crates/harness-tui/src/lib.rs:265`).
3. **Re-exports:** core re-exports `Session`, `Store`; not `Turn`, store `Event`, `Thread`.
4. **Provider chosen at startup:** `HarnessOptions.provider` is required; a GUI needs a default, e.g. the `LAST_PROVIDER_SETTING` value.

## Rule collisions to resolve

1. Move `workspace_changes` into core (new owned hunk/patch type there), have the TUI call it, delete the TUI copy.
2. Serde for `Answer`, `UndoOutcome`, `Provider`, `Session`: one canonical type, no mirror DTOs.
3. Rename needs one canonical title writer (generalize or replace the if-unset method).
4. Transcript replay: new `Harness` read methods; core converts store rows into typed `Event`s; the GUI never parses store rows.
5. Model list: config-driven models change `Providers`, `Config::provider`, `client_dialect` and a user-visible config key (README and `/help` in step).
6. Vocabulary/ADR update (already planned as ADR 0003).
7. Per-session workspace in core would be hard to reverse; likely needs its own ADR.

No gap forces a dependency between leaf crates.
