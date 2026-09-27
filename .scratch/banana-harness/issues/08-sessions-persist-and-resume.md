# 08: Sessions persist and resume

**What to build:** complete persistence of durable state.

- Thread context snapshots are upserted after every LLM or tool step.
- Turn state and usage are recorded.
- Settings are stored (the last provider).

**Slash commands:**
- `/new` starts a fresh session.
- `/sessions` lists earlier sessions (title, workspace, time) and resumes the chosen one from its root thread's last snapshot.
- `/provider` switches provider for the session.

Only the turn runner writes to the database.

**Blocked by:** 02

**Status:** done

- [x] Seam-1 test: after one turn, opening a new harness on the same database and starting a turn in that session sends the earlier messages to the model.
- [x] Seam-1 test: the schema is created on a fresh database, and reopening an existing one doesn't fail or lose data.
- [x] Seam-1 test: the last provider chosen is read back from settings.
- [x] Seam-2 test: the sessions list renders.

## Notes

For ticket 03 (parallel, same base) and later tickets touching persistence/TUI.

**Migration:** `harness-store/migrations/0003.sql` adds `thread(id, session_id, parent_id,
context_json, context_usage_json, updated_at)`, index on `session_id`. `parent_id IS NULL`
identifies a session's root thread (no subagents yet, so there's exactly one thread per session
today). No `title` column on `thread` — the TS reference schema has one, but nothing in this repo
reads or writes it yet; add it in the ticket that actually titles a thread (07, subagents) rather
than scaffolding it now.

**Snapshot format and cadence:** `context_json` is a plain `Vec<harness_llm::Message>` (the flat
chat-messages array, matching the TS reference's `AgentThreadSnapshot.context` — no higher-level
wrapper), **excluding** the system message, which `TurnRunner::execute` rebuilds fresh every turn
from `instructions::system_prompt` (so AGENTS.md/skill changes are picked up on resume too).
Upserted once per LLM step, right after `ChatEvent::Done` (there are no tool steps yet — ticket 03
should upsert similarly after each tool-call batch, appending `Message`s for the call and its
result, matching the existing redaction pattern below). `context_usage_json` is the *latest*
completion's `Usage`, not a running total. Both fields are passed through the same `Redactor` used
for events/turn input, so a leaked key never survives into a resumed context.

**Session resume, in one mechanism:** there's no separate "resume" API. `TurnRunner::execute`
always calls `store.root_thread(session_id)` at the start of a turn and prepends whatever history
it finds before the new system+history+user message array — this is exactly what makes both
"send a follow-up in the same running harness" and "reopen the database in a new process and
continue the session" work identically, and is what ticket 08's own seam-1 tests exercise
(`harness-core/tests/sessions.rs`). `/sessions` just needs to set `session_id` to an existing id
and call `start_turn` as normal.

**Provider is per-session, not per-harness:** `HarnessOptions.provider` only seeds a *brand-new*
session's `provider`/`model` columns (`Harness::create_session`). Every turn afterwards reads
`session.provider` fresh from the store and builds a throwaway `ChatClient` for it
(`ChatClient::new` is a cheap sync call — no client is cached across turns). `Harness::switch_provider(session_id,
provider)` (new) just updates that session's row plus the `last_provider` setting
(`harness_core::LAST_PROVIDER_SETTING`, moved here from harness-tui so both the startup popup and
`/provider` share one constant) — no in-memory harness state to keep in sync. This sidesteps any
`ChatClient`-across-`.await`-and-`Mutex` complexity entirely.

**Session title:** first-write-wins (`Store::set_session_title_if_unset`), set from the first
turn's input truncated to 50 chars (matching the TS reference's
`MAX_SESSION_TITLE_LENGTH`/`deriveSessionTitle`), redacted like everything else persisted.

**Slash-command dispatch in the TUI:** `harness-tui/src/lib.rs`'s `interpret_key` matches
`/new`/`/sessions`/`/provider`/`/quit` literal input the same way `/quit` already worked, returning
new `Action` variants (`NewSession`, `ListSessions`, `ResumeSession(id)`, `CloseOverlay`,
`ToggleProvider`) that `handle_terminal_event` executes against `Harness`. `MainState` gained
`overlay: Option<app::Overlay>` (currently just `Overlay::Sessions{ sessions, selected }`) and a
`TranscriptItem::System(String)` variant for status notes ("Started a new session.", "Resumed
session: …", "Switched provider to …"). While `overlay` is `Some`, `interpret_key` routes every key
through `interpret_overlay_key` (Up/Down move `selected`, Enter resumes, Esc closes) instead of the
normal input handling — extend that match, not a parallel key-handling path, when a second overlay
kind is added later. `/provider` toggles immediately (DeepSeek/Qwen is a 2-way choice, same as the
startup popup) rather than opening a picker.

**Deviations:** `/sessions` doesn't replay the resumed session's prior turns into the visual
transcript — it clears it and shows a "Resumed session: …" note. The underlying model context is
correctly resumed regardless (that's store-side, independent of the TUI), and reconstructing
`TranscriptItem`s from a stored `Vec<Message>` felt like ticket 13's polish scope, not this ticket's
acceptance criteria.

**Reconciling with ticket 03:** migration `0003.sql` is the next free number; if ticket 03 also
grabbed `0003`, whichever merges second should renumber to `0004` (never renumber an already-merged
migration). Ticket 03's tool-call events/messages should extend `harness_llm::Message` and the
`thread.context_json` array additively (e.g. tool_call/tool-result entries) rather than replacing
the snapshot shape — nothing here assumes only user/assistant roles ever appear in `context_json`.
