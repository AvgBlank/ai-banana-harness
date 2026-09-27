# 06: Long runs: compaction and large tool output

**What to build:** so that long tasks can finish.

**Context usage:** taken from the provider's usage after each LLM call; for every other append, estimated as characters/4.

**Compaction:**
- Runs before every LLM call, in subagents too.
- Threshold: the smaller of 80% of context length and context length minus max output, falling back to 50,000. It fires when usage + 931 reaches the threshold.
- It summarises the whole context with TrueForge's 8-section prompt, ported verbatim (see the design reference), replaces the context with the summary plus a continuation internal message, and emits context-compacted.

**Large tool output:**
- A batch under 6,000 tokens passes through untouched.
- Otherwise every result of 6,000 tokens or more is blocked, then the largest others until the total is under 10,000.
- A blocked result's content is written to the self-git-ignoring tool-output folder in the workspace and replaced with its path, guidance and a 100-character head/tail preview.
- Failures are truncated to 500 characters.

**In the TUI:** a "context compacted" notice, and a context-usage bar in the status line (the full sidebar comes in 13).

**Blocked by:** 03

**Status:** done

- [x] Seam-1 test: when usage crosses the threshold, the next model request contains only the summary plus the continuation, and a context-compacted event is emitted.
- [x] Seam-1 tests: a single huge tool result is spilled to a file whose content matches the original, and the model receives a path plus the preview. Mixed-size batches follow the 6,000/10,000 rules. A failure is truncated.
- [x] Seam-1 test: after a spill, `git status` in the workspace is clean except for the agent's intended changes.
- [x] Seam-2 test: the compaction notice renders.

## Notes

For tickets 07, 13 and later, and for reconciling with 04/07/10/13 (parallel, same base).

**Compaction (`crates/harness-core/src/compaction.rs`):** pure pieces only: `threshold`,
`is_due`, `summary_request`, `compacted_context`, `compacted_usage`, `estimate_tokens`, plus the
private `transcript` serialiser and the `PROMPT`/`CONTINUATION` texts. The constants are exact:
`PROMPT_TOKENS = 931`, `CONTINUATION_TOKENS = 42`, `FALLBACK_THRESHOLD = 50_000`, and 80% computed
as `ctx * 4 / 5` (the integer floor). The loop calls it in `harness.rs`
(`TurnRunner::execute`, around `compaction::is_due` at :287), immediately before
`client.stream`. The summary is a "non-streaming" call through `TurnRunner::complete` (:469), which
drains `client.stream(&request, &[])` without forwarding deltas and honours cancel. No new
`harness-llm` API was needed. Compaction then truncates `messages` to the system prompt, appends
`[assistant: summary, user: <bh-internal>continuation</bh-internal>]`, emits the durable
`context.compacted{thread_id, usage}` (`event.rs:58`, `self.emit`) and snapshots the thread.

**The prompt is verbatim.** Contrary to the brief's expectation, the TS source *is* in this
worktree (`packages/trueforge-core/src/core/capabilities/builtins/ContextCompaction.ts:45-133`).
`PROMPT` was copied from it and checked byte-for-byte with a script, including its two trailing
spaces, its duplicated "6." and the example's odd section numbering. The continuation text is
copied too, with `<tfy-internal>` renamed to `<bh-internal>`. Transcript markers match TS:
`<i><user>:`, `<i><assistant>:`, `<tool-call id=… name=…>:`, `<i><tool-response id=…>:`. Internal
messages and the system prompt are skipped, and `i` is the index in the context without the
system prompt. There are no thinking blocks, because thinking is disabled.

**Context usage (the "current context usage" TS tracks):** a running `Usage` in `execute`. At turn
start it is the thread's stored usage plus chars/4 of the new input, or chars/4 of everything when
there's no stored usage. After each LLM call the provider's `usage` replaces it (chars/4 of the
assistant message if the provider sent none). After each tool step it adds chars/4 of the tool
messages, measured after capping. `thread.context_usage_json` now stores **this running value**,
not the last completion's usage. That is what makes resume compact at the right time. Nothing read
the old meaning. `turn.usage_json` is unchanged: it is still the last completion's usage.

**Threshold config:** `ProviderConfig` gained optional `context_length` and `max_output`
(`config.rs`). Both are unset in `default.toml`, because the real windows of `deepseek-flash` and
`qwen3.8-flash` aren't known here. That means the evaluation config uses the 50,000 fallback. A
comment in `default.toml` documents the keys. `max_output` is only the threshold's reservation: it
isn't sent to the provider, since no `max_tokens` is sent today. `Harness::compaction_threshold(provider)`
(`harness.rs:128`) exposes the result for the TUI bar.

**Large tool output (`crates/harness-tools/src/large_output.rs`, `pub fn cap_large_outputs(workspace,
&mut [(tool_name, ToolResult)])`):** it lives in `harness-tools` beside the other workspace
filesystem code. `harness-core` calls it once per tool step (`harness.rs:417`, inside
`spawn_blocking`), after every result is collected and before any `tool.response` is emitted or
appended. The model and the event therefore always see the same capped text. Tokens are `chars / 4`
as `f64`, like TS's `s.length / 4`. The algorithm follows TS `process()`. Oversized results are
blocked in batch order. The rest are sorted largest first and blocked while the running total is
still at or above 10,000. The total drops by `original − replacement` each time. Spill files are
`.banana-harness/tool-output/{tool}_{batch index}_{epoch ms}.txt`, where the tool name goes through
the crate's `sanitize_name` and is cut to 40 characters. The model gets the **workspace-relative**
path, which `read_file` and `grep` accept directly. The message follows TS `createFileDumpMessage`,
with the guidance rewritten for `read_file`/`grep`. If the write fails, the result falls back to
TS's no-sandbox guidance plus the preview, so the step never fails. The spill directory goes through
`guardrail::confine` after it is created.

**`ToolResult.failed` (new field):** set by `ToolResult::error` and `ToolResult::blocked`, and false
for `ok`. It is how "failure" is categorised (TS `result.failure`). **Reconciling with ticket 04:**
new tools that build results through the existing constructors need no change. A struct literal
would now need `failed`.

**TUI:** `EventKind::ContextCompacted` pushes the existing `TranscriptItem::System("⟳ context
compacted")`, which renders as a muted `· ⟳ context compacted` line. It also sets
`MainState.context_tokens`, which each final `model.message`'s usage updates too.
`MainState.context_limit: Option<u64>` is set from `Harness::compaction_threshold` on startup,
`/provider` and `/sessions` resume. `ui.rs::draw_status` splits off a right-aligned 20-column
`ctx ■■■□□□□□□□ 34%` bar (`context_bar`, :243), where 100% means "compaction is due now". It is
hidden while the limit is `None`. **Ticket 13** owns the full sidebar and should move or reuse
`context_bar` rather than add a second one.

**Tests:** seam 1 is `crates/harness-core/tests/long_runs.rs`, which builds its own SSE with small
`chunk`/`answer_sse`/`calls_sse` builders and doesn't use `common/mod.rs`, to keep merges with
siblings simple. It covers: compaction mid-turn (the summary request's shape, a next request of only
system + summary + continuation, the event, and its persistence), the min(80%, ctx − max_output)
threshold, a single spill (the file equals the original, and the model gets the path plus the
preview), `git status --porcelain --untracked-files=all` equal to `?? big.txt`, the 6,000/10,000
mixed batch (with a mutation check: it fails when 10,000 is changed to 10,200), an untouched
moderate batch, and failure truncation. The failure is a 30,000-character unknown-tool name. Seam 2
is `crates/harness-tui/tests/compaction.rs`, which checks the notice and the bar before and after.

**Deviations:**
- **Compacted usage uses the summary's *output* tokens + 42**, not TS's literal
  `response.usage.input_tokens + 42` (`ContextCompaction.ts:239`). TS's value is the size of the
  *old* transcript. That would leave usage at or above the threshold right after compacting, and
  would show about 100% on the bar, until the next LLM call corrects it; TS's own NOTE admits "this
  is not really correct". The summary becomes the new context's input, so its completion tokens are
  the honest figure. The +42 constant is ported exactly. Without provider usage it falls back to
  chars/4 of the summary.
- **The `.gitignore` of `*` lives in `.banana-harness/tool-output/`**, not in `.banana-harness/`.
  This matches the ticket's "self-git-ignoring tool-output folder". `.banana-harness/` also holds
  the user's committed `config.toml` and `skills/`, which a `*` one level up would hide from
  `git status` and from the `.gitignore`-honouring glob/grep tools. **Reconciling with ticket 04:**
  its guardrail note about `.banana-harness/tool-output` always being allowed still applies unchanged.
- The large-output guidance leaves out TS's subagent step ("delegate to a sub-agent … pass the file
  path"). **Ticket 07** should add it to both messages in `large_output.rs::block` when subagents
  are enabled.
- **Subagents (ticket 07):** compaction and capping are called from the one thread loop that exists
  today. When 07 splits out a per-thread LLM step, it needs a per-thread `context_usage` and should
  call the same `compaction::is_due`/`TurnRunner::complete` block and `cap_large_outputs`. The block
  at :287 is self-contained, so it can move into a thread-step function as is.
- MCP results (**ticket 10**) and bash output (**ticket 04**, spec story 46) go through
  `cap_large_outputs` automatically, as long as they're in the same tool-step batch.
