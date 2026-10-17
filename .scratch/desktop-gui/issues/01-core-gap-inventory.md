# Which GUI operations the core already supports

Type: research
Status: resolved
Blocked by:

## Question

For each v1 feature (workspace-grouped session list, resume with transcript replay, send turn, stream events, approvals/questions, cancel, undo, model/provider picker, Changes panel with real diffs, rename session), which `harness-core` / `harness-store` operations already exist, and which are missing? Report with file paths. Known so far: `Harness::open` has one fixed workspace; `sessions()` spans workspaces; `Answer`, `Session`, `Provider` and `UndoOutcome` are not serde; `workspace_changes` lives in `harness-tui`; there is no session-rename or model-list API. Confirm and extend, and note which gaps would break the crate-boundary or single-owner rules.

Findings: [research/01-core-gaps.md](../research/01-core-gaps.md)

## Answer

Send turn, stream events, cancel, approval reply and undo exist. Missing or needing change: transcript replay, real diffs (only counts exist, in the TUI crate), rename, serde on `Answer`/`UndoOutcome`/`Provider`/`Session`, a provider/model list (each provider has one fixed model), and per-workspace operation (one `Harness` per workspace; `Store` ownership needs a shared-handle plan). Full inventory: [research/01-core-gaps.md](../research/01-core-gaps.md). Follow-up decisions are in [Core API additions](10-core-api-additions.md).
