# 03: The agent reads and searches the workspace

**What to build:** the tool loop. The model can call the read-file, glob and grep host tools. The thread runs the calls in parallel, appends the results, and calls the model again until it answers without tool calls.

**Tool rules:**
- Paths are confined to the workspace, with symlinks resolved.
- Reading `.env*` is denied.
- A blocked call returns "Blocked by guardrail: <rule>" to the model, and the event is marked blocked.
- Tool names are sanitised, and tool parameter schemas keep only properties and required.

**In the TUI:** each call shows as a collapsed one-line tool card with its outcome, and blocked calls get a red badge.

**Blocked by:** 02

**Status:** done

- [x] Seam-1 test: the model reads a file in a temporary git workspace, then answers. The tool response event carries numbered lines.
- [x] Seam-1 tests: a path outside the workspace, a symlink that escapes it, and a `.env` read are each blocked with the guardrail error, and the turn continues.
- [x] Seam-1 test: glob and grep respect `.gitignore`.
- [x] Seam-1 test: two tool calls in one assistant message both execute, and both results are appended before the next model call.
- [x] Seam-2 test: a transcript containing a tool card renders its one-line summary.

## Notes

For ticket 04 (write/edit/bash, approvals-adjacent plumbing) and ticket 05 (approvals), and for
reconciling with ticket 08 (persistence/resume, running in parallel on the same base).

**Tool trait/registry shape (`harness-tools`):** per the design-reference spec ("dispatch is by
enumeration, not trait objects"), there is no `dyn Tool`. `HostTool` (`src/lib.rs`) is a plain
`enum { ReadFile, Glob, Grep }` with `name()`/`description()`/`parameters()`/`call(workspace,
&Value) -> ToolResult`, matched by hand in each method. `harness_tools::call_by_name(workspace,
name, arguments_json) -> ToolResult` is the one entry point a caller needs (looks the tool up by
name, parses the JSON arguments, dispatches) — callers never match on `HostTool` themselves.
`ToolResult { content: String, blocked: bool }` is the outcome; its constructors (`ok`/`error`/
`blocked`) are crate-private, so only `harness-tools`' own tool modules build one. Tool errors
(bad args, missing file) render as `{"error": message}` (matching the TS reference's
`executeToolCalls` convention); a guardrail block renders as the literal `"Blocked by guardrail:
<rule>"` (per this ticket's own text — note the broader `rust-tui-migration` spec's §7.2 shows
that string JSON-wrapped as `{"error": "Blocked by guardrail: ..."}`; this ticket's own acceptance
text does not, so the plain string is what's implemented and tested).

**Ticket 04 extends, doesn't replace:** add `HostTool::WriteFile`/`Edit`/`Bash` variants and their
modules; `harness_tools::host_tool_specs()`/`call_by_name` already fold in whatever's in
`HostTool::ALL`, so no caller in `harness-core` needs to change. `sanitize_name`/`strip_schema` in
`lib.rs` are already written for a multi-source future (ticket 10's MCP tools reuse them with
their own `used: HashSet<String>` continued from the host batch).

**Guardrail pattern matcher:** `harness-tools/src/pattern.rs`, one function
`glob_match(pattern: &str, text: &str) -> bool` (`*` = any run of characters, everything else
literal), with the ticket's one permitted seam-exception table test
(`pattern::tests::combinatorics`). `harness-tools/src/guardrail.rs` uses it for
`DENIED_READ_PATTERNS: &[&str] = &[".env*"]` today. Ticket 04's bash denylist
(`sudo *`, `rm -rf /*`, ...) and the full opencode-style allow/ask/deny permission engine (with
config-driven pattern maps) are **not built yet** — this ticket only needed a hardcoded deny set
(workspace confinement + `.env*`), no `ask`, no config table, no doom-loop. `confine(workspace,
requested) -> Result<PathBuf, String>` (canonicalises, symlinks included, returns the rule name as
`Err` on escape; a path that doesn't exist yet is returned unresolved rather than blocked, so a
tool's own "not found" error still surfaces normally — relevant to ticket 04's `write_file`/`edit`,
which create new paths).

**Event kinds added (`harness-core/src/event.rs`):** `tool.call{thread_id, tool_call_id, name,
arguments}` (a start marker, emitted for every call up front before any of them run) and
`tool.response{thread_id, tool_call_id, content, blocked}` (the outcome). Both are durable
(`self.emit`, not `self.forward`), so they're read back from `event` rows exactly like
`model.message`. **No new store tables or columns were needed** — the existing generic `event`
table (`id, turn_id, thread_id, type, payload_json, created_at`) already covers this; only new
`type` string values and payload shapes. This should make reconciling with ticket 08 (which adds
its own tables, e.g. `thread`/`setting`) low-risk — nothing here touches `harness-store`'s schema
at all.

**Turn loop shape (`harness-core/src/harness.rs`):** `TurnRunner::execute` now loops per LLM
round: build the system prompt and the (fixed) tool list once outside the loop, `client.stream(&
messages, &tools)`, and on `finish_reason` other than an empty `tool_calls` list, append the
assistant message (with `tool_calls`) and loop back after running them; an empty `tool_calls`
ends the turn as before. Tool calls within one round are emitted as `tool.call` events in order,
then run concurrently via `tokio::task::spawn_blocking` (host tools are blocking filesystem
work — this is what gives real OS-thread parallelism, not just interleaved async), then their
`tool.response` events and `Role::Tool` history messages are appended **in the original
`tool_calls` order**, not completion order, so the sequence is deterministic and provider history
stays correctly matched by `tool_call_id`. `thread_id` is still minted once per turn and reused
across every round in this loop, per ticket 09's note about building the prompt once per thread.

**`harness-llm` wire format:** `Message` gained `tool_calls: Vec<ToolCallRequest>` (skipped when
empty) and `tool_call_id: Option<String>` (skipped when `None`); `Role::Tool` added.
`ChatClient::stream` takes a second `tools: &[ToolDefinition]` parameter and adds a `"tools"`
array to the request body when non-empty (plus `"parallel_tool_calls": true` for Qwen, which
defaults that to `false`). `Completion` gained `tool_calls: Vec<ToolCallResult>`, assembled inside
`ChatStream` from streamed fragments in a `BTreeMap<usize, PartialToolCall>` keyed by `index`;
Qwen's empty-string `id` on continuation chunks is filtered out before it can overwrite the
accumulated id (`fragment.id.filter(|id| !id.is_empty())`). Existing fixtures/tests were
unaffected (missing `tool_calls` keys deserialize to `None`, same as the existing `error` field's
behaviour). New fixtures: `deepseek_read_call.sse` (single call, `__PATH__` placeholder reused
across the success/blocked tests via `.replace()`), `deepseek_glob_call.sse`,
`deepseek_grep_call.sse`, `deepseek_two_calls.sse` (two calls, interleaved by index across
chunks), `qwen_read_call.sse` (exercises the empty-continuation-id quirk). All second-round
"the model answers" replies reuse the existing `deepseek_plain.sse`/`qwen_plain.sse`.

**TUI (`harness-tui`):** `app::TranscriptItem` gained a `Tool(ToolCallItem)` variant
(`tool_call_id`, `name`, `arguments`, `outcome: Option<ToolOutcome>`); `apply_event` handles
`EventKind::ToolCall` (push a new card, `outcome: None`) and `EventKind::ToolResponse` (find the
card by `tool_call_id` via a new `tool_call_mut` helper, same pattern as `assistant_message_mut`,
and set its outcome). `ui.rs` renders one collapsed `Line` per tool card
(`🔧 name(compact args) …` while pending, `→ <preview>` once answered, or a red-styled `"blocked"`
badge via `theme::error()`); `compact_json`/`one_line` are small formatting helpers, private to
`ui.rs`. `theme` had to become `pub mod` (was private) so the seam-2 test can assert the badge's
cell colour is exactly `theme::ERROR`, not just that the text "blocked" appears.

**Test workspace is now a real git repo:** `harness-core/tests/common/mod.rs`'s
`TestHarness::open_with` now runs `git init -q` in the temp workspace before returning — needed
because `ignore::WalkBuilder` only honours `.gitignore` files when `require_git` (default `true`)
finds an actual `.git` directory. This is additive; nothing about the existing fields/behaviour of
`TestHarness` changed, so it shouldn't conflict with ticket 08's own test-infra additions.

**Deviations from the ticket text:** the tool is named `read_file` (underscore), matching the
design-reference spec's host-tools table (§7.1) and the TS reference's naming convention, not the
ticket prose's hyphenated "read-file" (which is just informal phrasing — `glob`/`grep` in the same
sentence aren't hyphenated either).

**Unticked:** none — all five boxes are met and covered by the tests listed above.
