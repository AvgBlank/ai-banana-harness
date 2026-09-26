# Persistence scope

Type: grilling
Status: resolved
Blocked by: 08

## Question

For the one-week version, which runtime state is persisted to SQLite, and which stays in memory only? Candidates:

- sessions
- turns
- the event log, for resume or replay
- compaction state
- MCP OAuth tokens

What schema and migration approach follows from that?

## Answer

The user said (2026-09-27): "dump all the required data into SQLite". The full detail is in [spec.md](../spec.md) §6.

- **The database:** rusqlite (bundled) at `<data dir>/banana-harness/harness.db`, in WAL mode. Migrations are embedded SQL run in order and tracked with `PRAGMA user_version`, with no migration crate.
- **Tables:**
  - `session`
  - `turn`
  - `event`: every durable event, not deltas, keyed by ULID
  - `thread`: a context snapshot after every LLM or tool step
  - `mcp_oauth_token`
  - `setting`: for example, the last provider chosen
  - `checkpoint`: the git refs used by `/undo`
- **Resume:** `/sessions` resumes a session from its last root-thread snapshot. A turn that was waiting on a user answer when the process died is not resumed at that point (ADR 0002).
