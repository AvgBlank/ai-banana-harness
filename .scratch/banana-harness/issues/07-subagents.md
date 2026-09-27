# 07: Dynamic subagents

**What to build:** the root-only create-subagent tool (name plus task), so depth is 1.

**What a child gets:**
- the subagent identity prompt
- its task as the only message
- the parent's tools
- no question tool and no subagent tool

**Scheduling:**
- Only leaf threads run: a parent with live children waits.
- At most five run at once.
- Events from concurrent threads fan into one stream with thread ids.
- A child's final text becomes the parent's tool result.
- Thread-created and thread-done events are emitted.

**In the TUI:** subagent cards nested in the transcript, and a subagent tree (a simple list is fine until 13's sidebar).

**Blocked by:** 03

**Status:** done

- [x] Seam-1 test: the root spawns two children that run concurrently. Each child's answer arrives as the parent's tool result, and then the parent answers.
- [x] Seam-1 test: when seven children are requested, no more than five run at the same time.
- [x] Seam-1 test: a child can't call the subagent or question tools.
- [x] Seam-2 test: a subagent card renders under its parent.

## Notes

For tickets 04/05 (tool dispatch, approvals), 06 (compaction/large responses), 11 (question tool),
13 (sidebar tree) and whoever merges the parallel branches.

**Thread loop factoring (`harness-core/src/harness.rs`):** `TurnRunner::execute` now only sets the
turn up (turn row, session, client, both system prompts, root history) and calls
`run_thread(shared, thread, messages, usage) -> Result<ThreadEnd>` (`harness.rs:286`), which is
the old flat LLM-then-tools loop verbatim, parameterised by a `ThreadInfo { id, parent, title }`
(`:601`; `parent: None` = root) and returning `ThreadEnd::Done(final_text) | Cancelled`. Per-turn
state shared by every thread lives in `Arc<TurnShared { session_id, client, child_prompt }>`
(`:593`). `TurnRunner` is now `#[derive(Clone)]`; each child task owns a clone, so event fan-in is
simply every thread writing through the same `mpsc::Sender` (multiple producers, one consumer)
via the same `emit`/`forward` — no per-child receivers to merge. The tool step calls
`run_tool_calls` (`:423`), which splits one round's calls into host calls (`spawn_blocking`, as
before) and `create_sub_agent` calls; the latter each become a `JoinSet` task gated by an
`Arc<Semaphore>` of `MAX_PARALLEL_SUB_AGENTS = 5` (`:452`), and the parent awaits every child
before emitting that round's `tool.response`s in original call order (this is "only leaves run").
`run_child` (`:479`) is a plain fn returning a boxed `dyn Future + Send`: spawning a future that
recursively awaits `run_thread` otherwise makes the opaque future type cyclic. It acquires the
permit, runs `run_child_thread` (`:516`: emits `thread.created`, messages are exactly
`[system: child_prompt, user: input]`), then emits `thread.done{error?}` and resolves to the
parent's `ToolResult`: the child's final text, or `{"error": "..."}` on any failure (e.g.
`max_tokens breached`, a provider error, cancel) — a child error never fails the turn, the parent
reacts to it like any tool error. Cancel is shared (one `CancellationToken`); the parent returns
`Cancelled` after its children stop. Turn `usage` stays the root's latest completion.

**Builtins (`harness-core/src/builtins.rs`, new):** a closed `enum Builtin { CreateSubAgent }` with
`root_only()` and `enabled(config)`; `Builtin::definitions(root, config)` builds the extra wire
tools for a thread and `Builtin::find(name, root, config)` is the dispatch check in
`run_tool_calls`. Anything `find` rejects falls through to `harness_tools::call_by_name`, so a
child that calls `create_sub_agent` gets `{"error": "unknown tool \"create_sub_agent\""}` and no
grandchild. **Ticket 11 (`ask_user_question`)** adds a variant with `root_only() = true` and gets
the child exclusion for free; `current_datetime` would be `root_only() = false`. The tool
description is TrueForge's `DynamicSubAgents` text minus the sandbox/model-set wording; arguments
are `{name, input}`, both required and non-empty (`SubAgentRequest::parse`).

**Prompts:** the child prompt is built once per turn with `SystemPromptInput { root: false,
subagents_enabled: false, config_instructions: "", agents_md: None, skills }` — identity is
`SUB_AGENT_IDENTITY`, no `<user-instructions>`, no `<sub-agents>` (it can't spawn), skills and
workspace sections kept. `config/default.toml` now has `subagents = true`; the flag gates
both the `<sub-agents>` section and the tool itself. `SUBAGENTS_TEXT` already matched the built
behaviour (five-way, root-only, final answer as tool result), so it is unchanged; the
instructions golden and `tool_calls.rs`'s tool-name set were updated for the now-on default.

**Events (`event.rs`):** `thread.created{thread_id, parent, title}` (title = the call's `name`) and
`thread.done{thread_id, error?}`, both durable, `thread_id()` returns the child's id so their
`event` rows are tagged with it. `thread.created` is emitted when the child actually starts (after
it gets a scheduler slot), so with seven requested the 6th/7th appear only as slots free up.
Only subagents get these events; the root thread doesn't.

**Persistence — migration number:** `harness-store/migrations/0004.sql` adds `thread.title TEXT`
(`ALTER TABLE`), appended to `MIGRATIONS`. **If another parallel ticket also took `0004`, whichever
merges second renumbers to the next free number.** `UpsertThread` gained `title: Option<&str>`,
`Thread` gained `title`, and there's a new `Store::threads(session_id)` (all threads, creation
order; used by the seam-1 test). Each child is upserted with `parent_id = <root thread id>` after
every LLM/tool step, like the root. `root_thread` still means `parent_id IS NULL`, so ticket 08's
resume is unchanged; children are never resumed.

**Test infra (`harness-core/tests/common/mod.rs`):** `FakeProvider::push_sse_matching(needle, body,
chunk_delay)` queues a reply served only to a request whose body contains `needle` (first eligible
reply in FIFO order; unmatched replies still serve anything), so concurrent children get their
own answers regardless of arrival order. `FakeProvider::max_in_flight()` reports the most
requests being answered at once — the concurrency assertion. `sse_text(content)` and
`sse_tool_calls(&[(id, name, args)])` build DeepSeek-format replies inline instead of new
fixture files. Tests: `harness-core/tests/subagents.rs` (two concurrent children → results →
parent answer, plus thread rows; seven children → max 5 in flight; child can't use
`create_sub_agent`/`ask_user_question`; failing child → error result, turn still done) and
`harness-tui/tests/subagents.rs` (card nested under the spawning call). Ran 15x, no flakes.

**TUI:** `TranscriptItem::Subagent(SubagentCard { parent, title, status, items })`
(`app.rs:60`); `thread.created` pushes the card at the end of the transcript (it lands right
after the parent's `create_sub_agent` tool cards, since all of a round's `tool.call`s are emitted
before children start), `thread.done` sets `SubagentStatus::Done | Error(msg)`, and any other
thread event whose `thread_id` names a card is applied to that card's `items` by the same
`apply_thread_event` (`:292`) the top level uses. `ui.rs`'s `push_subagent_lines` (`:206`) renders
`◆ subagent <title> <status>` then the child's items prefixed with `┊ `. **The subagent tree for
ticket 13** is `MainState::subagents()` (`app.rs:236`), an iterator over the cards (parent, title,
status) — derived from the transcript rather than a second list to keep in sync.

**Headless:** the plain-text `run` output skips subagents' deltas (tracked from `thread.created`),
so only the root's answer prints; `--json` still prints every event.

**Deviations:** no `JoinSet` batch draining like TS's "batches of 5": a semaphore starts the next
child as soon as any slot frees (the ticket's "at most five at once" holds either way). No
iteration/wall-clock limit applies to children yet, because none exists for the root either.
