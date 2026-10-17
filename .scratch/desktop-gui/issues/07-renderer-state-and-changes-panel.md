# Renderer state model and Changes panel data

Type: grilling
Status: resolved
Blocked by:

## Question

How does the renderer turn the event stream into UI state (per workspace and session; streaming deltas; thread/subagent grouping; tool cards; required actions; reconnect and replay on resume)? What is the Changes panel's source of truth (tool-response diffs per turn vs git working-tree status, "Last turn" vs "All changes" scopes), how are diffs rendered (Monaco vs lightweight), and how do Undo and View changes behave?

## Answer

Resolved 2026-10-17 against [the wire protocol](03-sidecar-placement-and-protocol.md), [the core additions](10-core-api-additions.md), [the Electron design](06-electron-process-and-security.md), the approved [prototype](04-banana-design-system.md), and the real event code in `crates/harness-core/src/{event,harness}.rs` and the TUI's `crates/harness-tui/src/app.rs` (`apply_event`). Nothing is implemented. Three small amendments to tickets 03, 06 and 10 are listed at the end; none changes the approved layout.

### Facts from the code that shaped the design

- **Live and replayed streams differ.** Every durable event is persisted by `emit`, but two kinds are only streamed: `model.message.delta`, and the `model.message` **start marker** (`message: None`), which is sent with `forward`, not `emit`. So a replayed turn has the final `model.message` with no marker before it. The doc comment on `EventKind::ModelMessage` ("Sent twice per LLM call") is true of the live stream only. The TUI's `apply_thread_event` would drop a final message with no marker; the GUI reducer must create the message on the final event.
- **Event ids are monotonic ULIDs** from one generator, so id order is event order, ids dedupe, and a ULID's first 48 bits are a millisecond timestamp (durations need no extra field).
- **Ordering inside a round:** `model.message` (final), then every `tool.call` of the round, then thread and approval events while tools run, then every `tool.response` of the round together after the slowest call finishes, in call order. A card therefore stays "running" until the whole round ends, and an approval is not resolved by any event: `tool.response` arrives later with the denial text.
- **Subagents:** `create_sub_agent` is an ordinary tool call in the root thread (it has `tool.call` and `tool.response`). `thread.created{thread_id, parent, title}` follows, `title` is the `name` the model gave in the call, only the root thread creates subagents, and only leaf threads run, so there is exactly one level of nesting. Up to 5 run in parallel.
- `ask_question` is also a tool call, plus a separate `tool.response_required` event (no `tool_call_id`), root thread only.
- `tool.response` has `blocked` and an optional unified `diff`, but no error flag. The diff has `--- before` / `+++ after` headers and `@@` hunks, and carries no file path (the path is in the call's `arguments` JSON: `path` for `read_file`, `edit`, `write_file`). Bash output starts with an `exit code: N` line.
- Turn rows left `running` after a crash have no `turn.done` (ticket 03: `TurnRecord.state === null`).
- The prototype's `parseUnifiedDiff` treats `--- before` and `+++ after` as a deleted and an added line. The build must skip everything before the first `@@`.

### 1. Boundaries and data flow

```
Rust (SQLite)  --session.resume-->  renderer reducer  -->  React views
       \--> sidecar --event frames--> main (live tail buffer) --live.attach / push--> renderer reducer
```

- **The reducer is a pure function in the renderer**, `reduceTurn(turn, event) -> turn`, with no I/O and no clock. UI state (expanded cards, selected file, scroll position, open dialogs) is kept in a separate slice that is never touched by events.
- **Truth ownership is as ticket 06 fixed it:** durable data is Rust's; main holds a discardable live-tail cache; the renderer derives everything else. The renderer never writes back event-derived data to disk.
- **Store:** a small external store (a `useSyncExternalStore` store with plain immutable updates, no state library) so the reducer and its tests do not depend on React.

### 2. State model

```ts
interface RendererState {
  sidecar: "starting" | "ready" | "needs_key" | "crashed" | "failed";       // from banana:sidecar:state
  workspaces: Record<WorkspacePath, WorkspaceView>;
  sessions: Record<SessionId, SessionMeta>;                                 // from session.list (all workspaces)
  views: Record<SessionId, SessionView>;                                    // loaded transcripts (LRU of 6)
  selected: { workspace: WorkspacePath; session: SessionId | null; view: "home" | "session" | "settings" };
  changes: Record<`${WorkspacePath}|turn:${SessionId}` | `${WorkspacePath}|workspace`, ChangesEntry>;
  ui: UiState;                                                              // never produced by events
}
interface WorkspaceView { path; provider; restricted: boolean; activeTurn: { sessionId; turnId: string | null; stopping: boolean } | null }
interface SessionView { turnOrder: TurnId[]; turns: Record<TurnId, TurnView>; pending: PendingAction[]; loadedAt: number }
interface TurnView {
  turnId: TurnId | null;           // null only for the optimistic turn until turn.created
  seq: number | null; input: string;
  status: "running" | "done" | "error" | "cancelled" | "interrupted";
  error: string | null; startedMs: number | null; finishedMs: number | null;   // decoded from ULIDs
  seen: Set<EventId>;              // dedupe
  root: ThreadView;                // thread_id of the root is the first thread id seen with no thread.created
  threads: Record<ThreadId, ThreadView & { title: string; status: "running" | "done" | "error"; error?: string }>;
  undone: boolean;                 // from turn.undone
}
interface ThreadView { items: Item[] }   // ordered
type Item =
  | { kind: "assistant"; messageId; text: string; final: boolean }
  | { kind: "tool"; toolCallId; name; args: ParsedArgs | null; rawArgs: string; state: ToolState; output?: string; blocked?: boolean; diff?: DiffFile; exitCode?: number; approvalActionId?: string; group?: SubagentGroup }
  | { kind: "notice"; text: string };      // context.compacted
type ToolState = "running" | "awaiting_approval" | "done" | "blocked";   // plus "interrupted" derived when the turn ended without a response
type PendingAction = { actionId; threadId; turnId; kind: "approval"; toolCallId; name; rawArgs; rule } | { actionId; threadId; turnId; kind: "question"; question; options };
```

- Events carry `thread_id`; the root thread is the one whose events have no matching `thread.created`. Subagent threads nest inside their `create_sub_agent` tool card (section 5), never in the root list.
- `input` comes from `TurnRecord.input` on replay and from the optimistic turn on `turn.start`.
- `status` is derived: `turn.done{state}` gives `done|error|cancelled`; a turn with no `turn.done` is `running` only while `WorkspaceView.activeTurn` names it, otherwise `interrupted`.
- Only the 6 most recently viewed sessions keep a full `SessionView`. Every session, loaded or not, keeps a small `SessionMeta` plus badge flags (running, awaiting approval, interrupted), maintained from frames and `workspace.list`.

### 3. Event reduction

One entry point, `applyEvent(state, frame)`, for live frames and replayed events. It resolves the target turn from `frame.workspace` + `frame.session_id` + `frame.turn_id` (a `null` turn id means the optimistic turn) and calls `reduceTurn`.

**Idempotence and order.**
1. If `event.id` is in `turn.seen`, return the turn unchanged. Otherwise add it. This is the whole dedupe rule.
2. Initial loads sort events by id before reducing. Live frames arrive in order and are applied as they come. If a live frame's id is lower than the turn's highest applied id (it cannot happen on one stream, but a merged replay can), the turn is rebuilt by re-reducing its sorted events; the cost is one turn, once.
3. Unknown event types are ignored (forward compatibility, ticket 03), after being added to `seen`.

**Per event.**

| Event | Reduction |
|---|---|
| `turn.created` | Binds `turn_id`, sets `startedMs` (ULID), status `running`, replaces the optimistic turn. |
| `model.message` with `message: null` (live start marker) | Upsert an assistant item keyed `(thread_id, message_id)`, `final: false`. |
| `model.message.delta` | Upsert the item (create it if the marker was missed, for example after a buffer overflow) and append `content`. |
| `model.message` with a message (final) | **Upsert the item and set its text to `message.content`; mark `final`.** This is authoritative: it repairs missing deltas and is the only form in a replay. An empty `content` (a tool-only round) is removed so it renders nothing. |
| `tool.call` | Append a tool item `running`, keyed `tool_call_id`; parse `arguments` once (tolerating invalid JSON: keep `args: null`, show the raw text). |
| `tool.approval_required` | Mark the card (found by `tool_call_id`) `awaiting_approval`, add a `PendingAction`. |
| `tool.response_required` | Add a question `PendingAction` (no card). |
| `tool.response` | Resolve the card: `blocked` gives `blocked`, otherwise `done`; store `content`, parse an `exit code: N` first line into `exitCode` for `bash`, parse `diff` (skipping the two header lines) with the call's `path`. Remove any pending approval for that call. |
| `thread.created` | Create a thread, attach it to a subagent group (section 5). |
| `thread.done` | Set the thread `done` or `error`. |
| `context.compacted` | Append a notice item to its thread ("Context compacted"). The 48k to 9k token figures in the prototype are not available from the event; the notice shows the text only. |
| `mcp.auth_required` | Not part of the transcript: it opens the external-link confirmation (ticket 06) with the OAuth URL and is recorded once per `(server, url)` per turn. |
| `turn.done` | Set status and `error`, `finishedMs`, clear the turn's pending actions, mark any tool still `running` as `interrupted`, close the workspace's `activeTurn`, mark threads that never finished as `interrupted`. |
| `turn.undone` (new, see amendments) | Set `undone: true` and add a notice. |

- **Streaming render:** deltas are applied to the store in batches, one flush per animation frame, so a fast stream causes at most 60 renders per second. Final messages and every non-delta event flush immediately.
- **Replay equivalence (acceptance test):** for any recorded turn, reducing the live stream and reducing only its persisted events give the same `TurnView` apart from `seen` (this holds because the final message and every other state are authoritative, and the live start marker adds nothing the final message does not).
- **No timers or clocks in the reducer.** Durations come from ULIDs; relative times in the sidebar are computed at render from `Session.updated_at`.
- **Bash exit code** is parsed only from the documented `exit code: N` first line; if it is missing the chip is simply not shown. No other error heuristics: tool output is shown verbatim.

### 4. Reconnection and reload

**Opening a session (also used after a renderer reload and after a sidecar restart):**
1. `session.resume {workspace, session_id}` gives `turns` (`TurnRecord`s). Each becomes a `TurnView` by sorting its events and reducing them.
2. `live.attach {workspace, session_id}` returns `{ frames, active_turn_id }` in one synchronous step in main. The renderer applies each frame whose `session_id` matches (the `seen` set drops overlap with step 1), then continues receiving frames.
3. Status for each turn is then derived as in section 2. A turn with no `turn.done` and `active_turn_id !== its id` is `interrupted`.
4. `workspace.list` supplies `pending_action_ids`, which is authoritative for what is still waiting; pending actions in the reduced state that are not in that list are dropped.

**On every renderer start,** after `workspace.list`, the renderer calls `live.attach` for each workspace's `active_turn` (at most one per workspace), so the running dot and the awaiting-approval badge work for sessions that are not on screen. When the user sends a prompt the renderer calls `live.attach` first and then `turn.start`, so the first frame cannot be missed.

**Late attach race.** A turn can finish between the `session.resume` response and the `live.attach` call. Ticket 06 discarded the buffer at `turn.done`, which would leave the resumed view stuck at "running". Amendment to ticket 06: main keeps each workspace's most recent turn buffer until the next `turn.start`, `workspace.close` or a sidecar crash, so a late attach still receives the tail including `turn.done`.

**Sidecar crash or restart.** On `banana:sidecar:state: crashed|failed` the renderer immediately marks every running turn `interrupted` (no `turn.done` will ever arrive), clears pending actions, keeps showing the transcript, and disables the composer. After `ready` and main's reopening of workspaces (ticket 06), the visible session is resumed again from the database; the interrupted turn then comes from `state: null`. Nothing is re-sent.

**Buffer overflow** (a turn over about 5 MiB of frames): early deltas may be gone from the live tail. Durable events come from `resume`; the final `model.message` repairs any message whose early deltas were lost; an in-progress message shows only its tail until it finishes. This is accepted and logged once.

**Switching sessions while a turn runs:** frames for sessions that are not loaded only update badges; selecting such a session runs the opening sequence above. The composer is disabled for any session in a workspace whose `activeTurn` is set (one turn per workspace, ticket 03), with the prototype's "A turn is running in this workspace" text; the sidebar shows which session is running.

**Stopping:** `turn.cancel` sets `stopping: true` ("Stopping…" on the Stop button); the UI waits for `turn.done{cancelled}`, which also closes any open approval dialog (core denies pending actions on cancel).

### 5. Tool-call grouping and rendering

- **Placement.** Items render in event order per thread: an assistant message, then the tool cards of that round in call order. A card is created at `tool.call` and updates in place at `tool.response`, so cards of a parallel round show `running` together and resolve together.
- **Subagent group.** A `create_sub_agent` tool card becomes a *group card* (the prototype's subagent card, now expandable): its header is the call's `name`, its body lists the child threads, each child expands to that thread's own items (assistant text and tool cards). A `thread.created` belongs to the **unresolved `create_sub_agent` call in the parent thread whose `name` equals the thread `title`**; if none matches (a retried or renamed call), it goes to the oldest unresolved one. Because only the root creates subagents there is no deeper nesting. The group's status is the worst of its children (error over done over running) and its resolved state comes from the call's `tool.response`.
- **Questions.** `ask_question` renders as a card showing the question and, once answered, the chosen text from its `tool.response`; the live question UI is the pending-action dialog, not the card.
- **Approvals.** The card shows `awaiting_approval` with the rule text; the dialog (prototype screen 5) shows the pending action. Several approvals can be pending at once (parallel tools); they are queued oldest first and shown one at a time, with the count ("1 of 3") in the dialog. A reply removes that action optimistically, sends `action.reply`, and an error from the sidecar (`unknown_action`) simply drops it.
- **Card content.** `edit` and `write_file` show the path and `+N −M` counted from the parsed diff, and an inline diff using the shared diff component; `bash` shows the command and the output tail with an `exit N` chip; `read_file`, `grep` and `glob` show their summary only; MCP tools (names containing the server prefix) show name and output; blocked calls show "Blocked by guardrail <rule>. The command did not run." (the rule comes from the response text `Blocked by guardrail: <rule>`).
- **Output caps.** Long output is truncated in the card at 200 lines or 64 KiB with a "Show all" control; the full text stays in state. The diff view windows long diffs with `content-visibility: auto` rows, and the transcript uses the same CSS for long sessions; no virtualisation library in v1.
- **Summary card per turn:** "Edited N files +A −R" is derived from the turn's `edit` and `write_file` diffs (one file counted once, counts summed), so it is identical live and replayed and needs no request. It does not include files changed by `bash`; the Changes panel is the authoritative view of the working tree (section 6). The card shows the duration from the ULIDs and the Undo and View changes actions.
- **Scroll.** The transcript sticks to the bottom while the user has not scrolled up; scrolling up detaches ("Jump to latest" appears), and sending a prompt reattaches.

### 6. Changes panel

**Sources (both from ticket 10, through `rpc.call`):**
- Scope **Last turn**: `changes.turn {workspace, session_id}`: the session's last turn against its checkpoint, including `bash` edits.
- Scope **Workspace**: `changes.workspace {workspace}`: everything uncommitted against `HEAD` plus untracked files.
- The title-bar badge and the panel header show the totals of the current scope for the selected session.

**State per scope:** `ChangesEntry = { status: "idle" | "loading" | "ready" | "error"; files: FileChange[]; git: boolean | null; checkpoint: boolean | null; fetchedAt; error? }`. A refresh keeps showing the previous files (stale-while-revalidate) with a thin loading indicator; it never blanks the list.

**Refresh triggers.** One refresh function per entry with single-flight (a refresh requested while one is running schedules exactly one follow-up) and a 400 ms trailing debounce:
- while the panel is open or the badge is shown: after a `tool.response` for `edit`, `write_file` or `bash`, and on `turn.done` (immediately, no debounce);
- after a successful undo; after the workspace is opened or the app window regains focus (the user may have edited files); when the selected session or the scope changes; on the panel's manual refresh button (a small icon in the header that is not in the approved layout would be new UI, so refresh is the keyboard shortcut ⌘R inside the panel and the focus trigger only);
- never while no session is selected or the sidecar is not ready.

**States (copy from the prototype):**
- no files: "No changes yet. Files the agent edits or creates show up here with their diffs." If the workspace result says `git: false`: "This workspace is not a git repository, so changes and undo are not available." If scope is Last turn and `checkpoint: false` while the last turn finished: "The last turn didn't change any files."
- error: inline "Couldn't load changes" with the sidecar's message and a Retry button; the previous list stays visible.
- truncated patch: the existing notice; binary: counts 0, "Binary file" row; large lists (hundreds of files) are windowed.
- `status: renamed` shows the new path (the type carries one path); deleted shows a "deleted" tag and the removed lines.

**Selection and navigation.** The selected file is kept by path per scope; if it disappears the first file is selected. "View changes" on a summary card opens the panel in Last turn scope on that session; clicking an `edit` or `write_file` card's path opens the panel on that file when it is in the current list, otherwise just opens the panel. The panel's open state, width and last scope are persisted preferences (ticket 06 `preferences.json`); default closed, as approved.

**Diff rendering.** The lightweight renderer from the prototype (about 50 lines) is the v1 diff view: unified diff, line numbers, tints, hunk headers, horizontal scroll. **Monaco is not used in v1.** Ticket 00 listed it "where appropriate"; nothing in the approved UI needs an editor or side-by-side diff, and leaving it out removes the worker, CSP (`blob:` workers) and CDN risks that tickets 02 and 06 flagged and drops a large dependency. It can be added later behind the same diff component without changing state. The renderer must skip lines before the first `@@` (the `--- before` / `+++ after` headers) and handle the "\ No newline at end of file" marker as plain context.

### 7. Undo flow

`session.undo` restores the session's **last** turn's checkpoint, so Undo exists only for that turn.

**Eligibility** (all must hold; otherwise the button is hidden or disabled with a tooltip):
- the turn is the session's last, its status is `done`, `error` or `cancelled` (not `running`), and the workspace has no running turn;
- `changes.turn` reported `checkpoint: true` for it (a turn that never mutated, or a non-git workspace, has none);
- `undone` is false.

**Steps.**
1. Click Undo. The renderer refreshes `changes.turn` and opens a confirmation dialog (the existing dialog component and style, same pattern as the quit confirmation): "Undo the last turn? This restores N files to how they were before the turn and deletes M files it created. Edits you made yourself since then are lost." with the file list from `changes.turn`. Buttons: **Keep changes** (default focus) and **Undo turn** (danger). This confirmation is the one new surface in this ticket; it is required because the restore overwrites any edits made after the turn.
2. On confirm: call `session.undo`, disable the button, show "Undoing…".
3. Result `restored`: apply the returned `turn.undone` event (new, below) so the turn shows an "Undone" state in the summary card (counts struck through, Undo gone, View changes kept), refresh both Changes scopes, show the composer hint "The agent still remembers this turn. Tell it you undid it."
4. Result `nothing_to_undo`: hide the button and refresh changes (the checkpoint was missing or already used); no error.
5. Error (git failure, locked index): keep the button, show an inline error with the message from the sidecar; the turn state is unchanged.

**Why a persisted marker.** Undo today is not a stream event, so after a reload the summary card of an undone turn would still offer Undo, and a second undo would restore the same checkpoint again and wipe whatever the user did since. Core therefore records undo (amendment below), and the reducer treats `turn.undone` like any event: it is replayed, deduped and deterministic.

**Limitation (documented, not solved):** undo restores files but does not change the session's conversation context. The next turn's model still believes the edits exist. v1 shows the composer hint; see the questions below.

### 8. Other renderer behaviours decided here

- **Sidebar data:** `session.list` is fetched at start, on `turn.created` and `turn.done` (the auto-title is set by core on the first turn), after create, rename and switch_provider, and when the window regains focus. Rename is optimistic and reverts with an error toast if the sidecar rejects it.
- **Badges:** running dot (a workspace `activeTurn` names the session), "awaiting approval" count dot (pending actions for the session), interrupted marker (last turn `interrupted`); the dock/title shows a total pending count when any session in a background workspace needs an answer.
- **Optimistic send:** `turn.start` immediately appends a `running` turn with the input text and `turn_id: null`, disables the composer, and binds the id at `turn.created`. If `turn.start` fails (`turn_in_progress`, `api_key_missing`, sidecar down) the optimistic turn is removed, the text is restored to the composer, and the error is shown inline.
- **Restricted workspaces and `needs_key`:** purely driven by the sidecar and workspace state from ticket 06; the reducer is unaware of them.
- **Not in v1:** the context-usage bar and token counters of the TUI (there is no method for the compaction threshold in the protocol and the prototype has no such element), session delete, pagination.

### 9. Where the code goes (for ticket 08)

`gui/src/renderer/state/` holds `reduceTurn.ts`, `store.ts`, `selectors.ts`, `changes.ts`, `diff.ts` (pure, no React or Electron imports); `gui/src/renderer/components/` the views carried over from the prototype; `gui/src/shared/protocol.ts` the zod schemas shared by main and renderer (ticket 06). Reducer and diff parser have no dependency on `window.banana`, so they run under Node in Vitest.

### 10. Tests and acceptance criteria

All reducer tests are plain Vitest (no DOM); view tests use React Testing Library; end-to-end uses the Playwright suite from ticket 06.

**Reducer (golden fixtures are the checked-in NDJSON frames from ticket 03 plus recorded real turns):**
- Replay equivalence: for each recorded turn (plain answer, tool calls, blocked call, approval, question, subagents, cancellation, error), `reduce(live stream)` equals `reduce(persisted events only)` except `seen`.
- A final `model.message` with no prior start marker creates the message; a final after missing deltas repairs the text; an empty final is removed.
- Dedupe: every event applied twice, and live frames overlapping a resumed prefix, give the same state; shuffled initial events are sorted by id first; a late lower-id frame triggers a rebuild that matches the in-order result.
- Tool cards: running until `tool.response`; blocked flag; `exit code` chip parsed and absent when the first line differs; invalid argument JSON keeps the card with the raw text; a diff with `--- before`/`+++ after` headers parses into hunks with correct line numbers and counts, and never shows header lines as changes; `\ No newline` handled.
- Subagents: two parallel children with different titles attach to the matching `create_sub_agent` calls; an unmatched title attaches to the oldest unresolved call; the group status rolls up; no children appear in the root list.
- Pending actions: approval and question added and removed by reply, by `turn.done`, and by `workspace.list`'s `pending_action_ids`; several approvals queue oldest first; none survive a finished turn.
- Interrupted: a turn with no `turn.done` and no active turn is `interrupted` with its running tools marked; with the active turn it is `running`.
- `turn.undone` sets `undone` and is idempotent.
- Summary derivation: files and counts from diffs, same live and replayed; durations from ULIDs.

**Reconnect (store tests with a fake `window.banana`):** resume then attach with overlapping frames yields no duplicate rendered items; attach after `turn.done` returns the retained tail and ends `done`; sidecar `crashed` marks the running turn interrupted and clears pending actions; after `ready` the visible session is resumed and the interrupted turn comes from `state: null`; buffer-overflow case ends correct once the final message arrives; attaching for every workspace `active_turn` at start sets background badges; `live.attach` is called before `turn.start`.

**Changes panel (RTL):** each state in section 6 (loading with stale list, error with retry, non-git, no checkpoint, empty, truncated, binary, deleted, renamed); single-flight and debounce (ten `tool.response` events in 100 ms cause at most two fetches); refresh on `turn.done`, after undo and on focus; selection survives refresh and falls back when the file disappears; "View changes" and a card path open the right scope and file; scope switch refetches; header totals match the file list.

**Undo (RTL plus store):** eligibility matrix (last turn only, finished, checkpoint present, not undone, no running turn; non-git hides it); the confirmation lists the files and defaults focus to Keep changes; confirm calls `session.undo` once and disables the button; `restored` marks the turn undone, refreshes both scopes and shows the composer hint; `nothing_to_undo` hides the button without an error; a sidecar error keeps the button and shows the message; after a reload the undone turn (from `turn.undone` in the transcript) offers no Undo.

**End to end (Playwright with the stub provider, extending ticket 06):** reload the renderer mid-turn and mid-approval and see the transcript, partial message and pending approval restored without duplicate ids in the DOM; kill the sidecar mid-turn and see the interrupted state and the banner, then resume; run a turn that edits a file and runs a `bash` edit, check both appear in Last turn and Workspace scopes, undo, and verify the file system is restored and the turn shows Undone after a reload.

### Amendments to earlier tickets

- **Ticket 10 (core):**
  1. `Harness::workspace_changes()` returns `WorkspaceChanges { git: bool, files: Vec<FileChange> }` and `turn_changes(session_id)` returns `TurnChanges { checkpoint: bool, files: Vec<FileChange> }`, so the UI can tell "no changes" from "not a git repository" and "no checkpoint".
  2. New durable event `turn.undone { turn_id }` (an `EventKind` variant, serde-tagged like the others). `Harness::undo` writes it to the undone turn's event log with the same id generator, returns it, and treats a last turn that already has it as `NothingToUndo`, so a second undo cannot restore the checkpoint again. The TUI's exhaustive event matches gain a no-op arm. Tests: `undo` twice, then `transcript` shows the event; a turn with no checkpoint; `/undo` in the TUI still reports correctly.
  3. Doc fix: the comment on `EventKind::ModelMessage` says the start marker is part of the event stream; note that it is streamed only and never persisted.
- **Ticket 03 (protocol):** `changes.workspace` result is `{ "git": bool, "files": [FileChange] }`; `changes.turn` result is `{ "checkpoint": bool, "files": [FileChange] }`; `session.undo` result is `{ "outcome": "restored" | "nothing_to_undo", "event"?: Event }` where `event` is the `turn.undone` event for `restored`. Additive; protocol version stays 1.
- **Ticket 06 (Electron):** `live.attach` returns `{ frames, active_turn_id }`; main keeps each workspace's most recent turn buffer until the next `turn.start`, `workspace.close` or a crash (it previously discarded it at `turn.done`), which removes the late-attach race; the renderer attaches to every workspace's active turn at start.

### Questions for you (defaults are used unless you say otherwise)

1. **Undo confirmation dialog.** Undo overwrites any edits you made after the turn, so I added one small confirmation dialog (existing dialog style, Keep changes focused). It is the only new surface in this ticket. Default: yes.
2. **Remind the agent after undo?** Undo restores files but the agent's memory of the turn stays, so the next prompt can assume edits that are gone. v1 shows a composer hint only. The alternative is a core change that inserts a note into the session's context on undo ("the user undid the previous turn"). Default: hint only; say if you want the core change.
3. **No Monaco in v1.** The approved diff view is lightweight and nothing needs an editor, so Monaco (listed "where appropriate" in ticket 00) is dropped, removing its worker and CSP risks. Default: dropped.

### Remaining blockers

Tickets 05 (fake provider sharing), 08 (repo layout, Makefile, CI, packaging) and 09 (ADR 0003 and `CONTEXT.md`) are open and unblocked. Ticket 09 now also records "interrupted turn", "restricted mode" and "undone turn" as vocabulary, and the three amendments above are applied in the build run together with ticket 10's list. After 05, 08 and 09 the map's remaining job is slicing the build into implementation tickets, which becomes possible once those are resolved.

### Decisions from the user (2026-10-17, during ticket 05)

1. The undo confirmation dialog is approved, with **Keep changes** focused by default.
2. v1 uses the composer hint after undo; there is **no** agent-context mutation feature.
3. Monaco is dropped from v1; the lightweight diff viewer is retained.
