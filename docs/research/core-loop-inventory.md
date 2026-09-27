# Core loop inventory for the Banana Harness port

Ticket: `.scratch/rust-tui-migration/issues/03-core-loop-inventory.md`. The only source is this repo's code and tests at `7ab8b615`.

Paths are relative to `packages/trueforge-core/src/` unless they start with `packages/`. Test paths are relative to `packages/trueforge-core/tests/`. LOC counts are raw TypeScript lines, including comments and OpenAPI `.describe()` text. Expect the Rust code to be smaller.

## TL;DR

- **The real logic is small.** It is a derived state machine over a context array (`AgentThread`), a leaf-scheduler for sub-agent threads (`AgentThreadOrchestrator`), plus four capability hooks: preSend, preLLM, toolResponse and postToolCall. Most of the rest is hosted glue: stores, tenants, sandbox, tracing, OpenAPI, SSE resume.
- **Port as-is:** the loop state machine, the send validation, `OpenToolCallCloser`, compaction thresholds and prompt, the large-tool-response algorithm, selector semantics, deferred-tool meta-tools, the sub-agent routing, the skills prompt text and the `InstructionBuilder` rendering.
- **Drop:** `SessionHandle` forking, ancestors and executor freezing; Redis event subscriptions; passthrough events; capability-state KV (no open-source user); `turn.update`/`paused` (never emitted); `user.tool_approval_policy` and `user.mcp_auth_continue` (schema-only stubs); `sandbox.created`; Code Mode wiring; SSRF guard; tracing spans.
- **Missing in TS but needed by the map:** there is no stdio MCP transport (only streamable-http and SSE), no local skill loader (skills come from a DB/registry and are mounted into a sandbox), and no host-filesystem large-response dump (the dump requires a sandbox).
- **Rough total:** about 6.5k TS LOC is in scope. After dropping glue, about 3.5–4.5k lines of Rust is realistic. The largest single piece is the thread loop (L).

---

## 1. Thread loop: `AgentThread` + `AgentThreadOrchestrator`

**Files:** `core/runtime/AgentThread.ts` (1453), `core/runtime/AgentThreadOrchestrator.ts` (528), `core/runtime/AgentThread.types.ts` (179), `core/runtime/contextUtils.ts` (176), `core/runtime/OpenToolCallCloser.ts` (97), `core/runtime/contextUsage.ts`, `core/runtime/metrics.ts`, `core/util/promiseUtils.ts:29` (`mergeAsyncGenerators`), `core/capabilities/AgentCapability.ts`, `core/capabilities/AgentContextProcessor.ts`.

### Essential contract: `AgentThread`

- **State is derived from context, never stored** (`AgentThread.ts:121-136`).
  - Open tool calls are computed by walking the context tail-first to the last assistant message and subtracting any `tool` replies (`:146-172`).
  - No open calls → `llm-call-required`.
  - An open call with `is_approval_required` and no recorded decision, or with `is_client_side` → `user-input-required`.
  - Otherwise → `tool-response-required`.
  - Transitions are asserted (`:106-119`).
- **`execute()` loop** (`:1313-1415`):
  1. Replay any `preComputedCompletion` (a sub-agent that finished in an earlier turn).
  2. Run preSend if it has not run yet.
  3. Init tools.
  4. Loop on the derived state:
     - `llm-call-required`: check abort, then the iteration limit, then `stepLLMCall`.
     - `tool-response-required`: check abort, then `stepToolResponse`.
     - `user-input-required`: emit the required events and exit. It deliberately does not check abort.
  - Every error becomes an error `AGENT_DONE` event. `execute()` never throws (`:1407-1411`).
  - The iteration limit defaults to 25 inside the thread (`:100`), but `AgentSpec` defaults it to 100 (max 1024) (`agent-session/schemas/agentSpec.ts:18,212-220`). It counts LLM calls per turn, because thread metrics are per instance and threads are rebuilt every turn.
- **`stepLLMCall`** (`:1014-1156`):
  1. Run the preLLM processors (compaction).
  2. Build the request (`transformToLLMRequest` `:791-838`): system instruction, then `definition.messages` (initial user messages), then the context with `tool_info` stripped and approval rows filtered out, then the ephemeral processors, `response_format`, `modelParams` and `tools`.
  3. Emit a `model.message` start marker (id only), then `model.message.delta`s, with `tool_info` resolved per delta (`:370-426`).
  4. If aborted, emit no final message (`:1073`).
  5. Enrich the tool calls a second time with `resolveUnderlyingTool: true`, for the context copy. This is how deferred `call_tool` picks up the underlying tool's approval flag (`:428-464`).
  6. Strip `reasoning_content` from the context copy. It is display-only (`:229-235`).
  7. Append to context. `finish_reason === 'length'` becomes an error done ("max_tokens breached"). No tool calls becomes done (`:1136-1153`).
- **`stepToolResponse`** (`:1158-1271`):
  1. Execute the open calls through `executeToolCalls` (§8).
  2. Run the ToolResponse processors (large-tool-response). They may rewrite content but must not add or drop ids (`:1224-1231`).
  3. Emit `tool.response` events, then append the tool messages to context.
  4. If any MCP call needs auth → exit.
  5. Run the postToolCall processors.
  6. If any sub-agent creations were returned, yield them and **exit**, so the orchestrator schedules the children.
- **`send()`** (`:602-671`, validation `:325-368`):
  - Runs preSend first. `OpenToolCallCloser` answers dangling ordinary tool calls on the last assistant message with `{"error":"Tool call was not executed. Please retry this tool call."}`, but skips approval, client-side and thread-creation calls (`OpenToolCallCloser.ts:26-67`).
  - A batch must be homogeneous. A user message is rejected while approvals or questions are pending.
  - Tool responses must match open ids.
  - A resume batch must resolve **all** pending approvals and client-side calls.
  - Approvals are stored in context as `{type:'user.tool_approval', tool_call_id, approval}` rows (never sent to the LLM). Client tool responses become `role:'tool'` messages.
- **Context-usage bookkeeping** (`:688-719`, `contextUsage.ts`):
  - After an LLM call, `current_context_usage = {prompt_tokens: usage.input_tokens, completion_tokens: usage.output_tokens}`.
  - Every other append adds a chars/4 estimate (`core/llm/usage.ts:3`, `contextUtils.ts:144-176`).
  - Compaction reads this value, so the provider must report usage when streaming.
- **Internal system messages:** these are user messages wrapped in `<tfy-internal>…</tfy-internal>`, and `INTERNAL_SYSTEM_PROMPT` tells the model never to produce them (`contextUtils.ts:28-53`). Compaction and file uploads use this. Rename the tag in the port.

### Essential contract: `AgentThreadOrchestrator`

- The threads live in `Map<id, AgentThread>`. **Only leaves run**, meaning threads that no other thread names as parent (`:79-95`).
- Leaves run in batches of `MAX_PARALLEL_SUB_AGENTS = 5` (`:37,453`). Events within a batch are interleaved with `mergeAsyncGenerators`, and each batch drains before the next one starts.
- The outer `while` loop keeps running until the map is empty or a stop happens (`:446-497`).
- Chunk routing (`:329-420`):
  - `tool.approval_required` or `tool.response_required` → stop the turn.
  - Root `AGENT_DONE` → stop the turn.
  - Child `AGENT_DONE`:
    1. If the parent's tool call is still open, emit a `tool.response` event with `content: ''` for the parent, and `send` the child's `send_to_parent` tool message into the parent's context.
    2. Emit the done event, fold the child's metrics into the finished bucket, and delete the child.
  - `AGENT_CREATE_SUBAGENT`: build a child thread through the injected factory, emit `thread.created`, and insert it into the map.
  - MCP auth events from parallel threads are merged per server id (`:97-116`).
- **`send()`** (`:264-316`):
  - Approval and tool-response items are grouped by `thread_id`, so a child can be paused.
  - User messages are rejected while more than one thread exists ("send empty input for previous conversation to complete").
  - Every thread's batch is validated before any thread is mutated.
- **Result:** `{output: root model.message | null, required_actions[], root_agent_error}` (`:522-526`). `TurnHandle` maps this to the terminal turn state (§2).

### Tests that pin it

- `orchestration/orchestration.test.ts`
- `orchestration/orchestrationWithTools.test.ts` (sub-agent round trip)
- `orchestration/orchestrationApproval.test.ts` (allow and deny)
- `core/runtime/openToolCallCloser.test.ts`
- `core/runtime/unknownToolLogging.test.ts`
- `core/runtime/modelMessageReasoningContent.test.ts`
- `core/runtime/sendBatchTypes.compile.test.ts`

No test covers the iteration limit, the `length` finish reason, the batch-of-5 scheduling or the invalid-transition asserts. Those need new Rust tests or golden transcripts.

### Hosted concerns to drop

- `Sandbox`:
  - prompt section `:587-589`
  - file uploads `UserInputMessage.ts:114-201`
  - `sandbox.created` events
  - skills token estimate `:851`
- `AgentTracing` spans: orchestrator `:118-226`, `withInitSpan` `:1335`.
- The `tfyManagedServerNames` part of the input-token breakdown (`:840-887`). Keep a simple breakdown or drop it.
- Capability-state KV (`:555-569,951-965`, `AgentCapability.ts:28-33`). In open source only the `tfy.plan` test fixture uses it (`agent-session/capabilityState.test.ts`).
- Passthrough events (`core/events/PassthroughEvents.ts`). The registry is empty in open source.
- File and image user parts (the map says text-only).

No Redis, HTTP or tenant code is used directly. The comment in `AgentContextProcessor.ts:37` that mentions Redis refers to the persisted context.

### Port size

**L.** About 2.3k TS LOC, roughly 1.2–1.5k lines of Rust (a tokio task per thread plus an mpsc fan-in in place of generator merging).

---

## 2. Turn and event schema

**Files:**
- `core/events/schema.ts` (477): harness events and inputs
- `agent-session/schemas/events.ts` (126): turn lifecycle and the persisted union
- `agent-session/schemas/turn.ts` (212): turn states and inputs
- `core/llm/LLMTypes.ts` (205): message, tool-call and `tool_info` shapes
- `agent-session/TurnHandle.ts` (560)
- `agent-session/SessionHandle.ts` (546)
- `agent-session/models/TurnRecord.ts`

### Essential contract

- **Event ids are monotonic lowercase ULIDs**, and they are the durable order key (`schema.ts:22-26`).
- **Turn states:** `running` | `done{output, required_actions[], metrics}` | `cancelled{reason}` | `error{message}` (`turn.ts:25-136`).
  - A paused turn is represented as **`done` with non-empty `required_actions`**.
  - `paused` and `turn.update` exist in the schema, but nothing in core emits them. They are only read by the store queries.
  - Cancel reasons: `client-cancelled`, `server-execution-timeout`, `cancelled-for-next-turn`, `abandoned` (`turn.ts:14-23`). Locally only `client-cancelled` (Ctrl-C) and perhaps `cancelled-for-next-turn` still mean anything.
- **Turn inputs:** `user.message` (string or parts), `user.tool_approval{thread_id, tool_call_id, approval: allow | deny{reason?}}`, `user.tool_response{thread_id, tool_call_id, content}`. They must not be mixed (`turn.ts:138-196`).
- **`TurnHandle.stream()`** (`TurnHandle.ts:197-379`):
  - It is single-use and persists every event before yielding it.
  - It emits `turn.created`, then the execution events, then `turn.done`.
  - It is the sole writer of the terminal state. The precedence is: signal aborted → cancelled; caught error → error; root agent error → error; consumer abandoned the stream → cancelled; otherwise done.
  - Persistence per event type is in `persistExecutionEvent` (`:403-558`):
    - Deltas and the `model.message` start marker are streamed but never persisted.
    - Durable assistant content arrives through `AGENT_CONTEXT_APPEND.output`.
    - `agent.context.overwrite` replaces the thread context and is **not streamed** to the client.
- **Cross-turn state = per-thread snapshot** `{context, current_context_usage, parent, agent_info, completion, capability_state}` (`AgentThread.types.ts:150-159`), plus the MCP session ids.
  - Each turn, threads are rebuilt from the previous snapshot (`SessionHandle.ts:175-339,454-492`).
  - Input is sent and validated **before** the turn is committed, so a failed send persists nothing (`:261-263`).

### Tests that pin it

- `agent-session/turnStream.test.ts` (terminal states, abort reasons, metrics isolation, single-use)
- `agent-session/sessions.test.ts` (send failure persists nothing)
- `core/events/eventId.test.ts`
- `agent-session/schemas/session.test.ts`

### Hosted concerns to drop

- `ISessionStore` and its Kysely Postgres/SQLite implementations (`agent-session/store/ISessionStore.ts`, `packages/trueforge/src/db/*`).
- `TurnNotRunningError` freeze races and `active_executor_id` (multi-executor).
- `freezeAndGetTurn`, `ancestor_ids`/`MAX_TURN_ANCESTORS`, and forking on arbitrary `previous_turn_id` (`SessionHandle.ts:35,192-198,346-353`).
- `tenant_id`, `custom`, and the session-title write.
- `insertTurnInboundEvents` (a commented-out stub, `packages/trueforge/src/apis/turns.ts:958-963`).
- SSE sequence numbers and Redis resume (`packages/trueforge/src/runtime/event-subscription/*`, `turns.ts:329-367`).
- `activeTurns` and the max-execution timer (`packages/trueforge/src/runtime/activeTurns.ts`, `turns.ts:445-450`).
- OpenAPI `.describe` decoration.

The SQLite state should keep only: sessions, a linear list of turns, per-thread snapshots, and the event log (ticket 10).

### Port size

**M.** About 1.6k TS LOC. Rust: serde enums of about 400 lines, plus a turn runner of about 300 lines.

---

## 3. Context compaction

**Files:** `core/capabilities/builtins/ContextCompaction.ts` (269). Wired in `agent-session/builtinsFromSpec.ts:33-40` for both root and child threads.

### Essential contract

- **Threshold** (`:13-41`), in order of precedence:
  1. An explicit `config.context_management.compaction.trigger.value`.
  2. Otherwise `floor(contextLength × 0.8)`, lowered to `contextLength − max_completion_tokens` (falling back to `max_tokens`) when that is positive and smaller. An unusable output reservation keeps the 80% value.
  3. Otherwise **50 000**.
- **Trigger** (`:208-214`): a preLLM hook, so it runs before every LLM step, including mid-turn and in sub-agents. It fires when `prompt_tokens + completion_tokens + 931 ≥ threshold`. The 931 is the hard-coded token count of the summary prompt (`:44`).
- **Algorithm** (`:154-197,215-243`):
  1. Serialise the whole context as one string with `<i><user>:`, `<i><tool-response id=…>:`, `<i><assistant-thinking>:`, `<i><assistant>:` and `<tool-call id name>:` markers. Internal messages are skipped. Non-text parts become "[user attached a … no longer available]".
  2. Make a **non-streaming** call to the same model with `[user: transcript, user: PROMPT]`. The prompt (`:45-133`) is a Claude-Code-style 8-section summary.
  3. Replace the entire context with `[assistant: summary, user: <internal>continuation: "must not repeat or re-run any work…"]`.
  4. Set `current_context_usage.prompt_tokens = summary input_tokens + 42` (`:140,239`).
  5. Emit `agent.context.overwrite{reason:'compaction', usage}` and increment `total_summarizations`.
- **Known gaps to keep or fix consciously:**
  - No recent messages are kept verbatim.
  - Nothing guards against the transcript itself overflowing the context.
  - The token estimate ignores the tool definitions (acknowledged at `:235-238`).

### Tests that pin it

- `core/capabilities/builtins/ContextCompaction.test.ts` (threshold resolution only)
- `agent-session/builtinsFromSpec.test.ts:49-66` (enable/disable)

Nothing tests the summarisation flow. Write a golden transcript for it.

### Hosted concerns

None. It needs only `ILLM.createNonStream`.

### Port size

**S.** About 270 TS LOC, about 110 of which is the prompt. Roughly 150 lines of Rust.

---

## 4. Large tool response handling

**Files:** `core/capabilities/builtins/LargeToolResponse.ts` (238), `core/capabilities/ToolResponseProcessor.ts`. Wired in `builtinsFromSpec.ts:56-63`.

### Essential contract

It runs once per tool step over the whole batch of results (`:180-213`). Tokens are estimated as chars/4.

1. If the batch total is under **6000** (`individualTokenThreshold`), do nothing. It is an error for the individual threshold to exceed the total.
2. Block every result that is at least **6000** tokens on its own.
3. Sort the remaining results by size, largest first, and block them until the running total drops below **10 000**.
4. How a result is blocked depends on its category (`:118-144`):
   - **failure:** truncate to 500 chars.
   - **sandbox tool:** "write to a file and use grep/sed", plus a preview of the first and last **100** chars.
   - **MCP tool:** guidance (reduce the params, use the sandbox, delegate to a sub-agent) plus the preview. **If a sandbox exists**, the full content is written to `{tool}_{i}_{epoch}.txt` in the sandbox and the message is replaced with the file path, guidance and preview (`:146-178`).
   - `isStructuredContent` is cleared in every case.

### Port note

Without a sandbox, the TS MCP path is **lossy**: only the preview survives. With host tools, the port should always dump to a workspace-local scratch directory and point the agent at `read_file`/`grep`. The "sandbox tool" category maps to the host `bash`/`read_file` tools.

### Tests that pin it

- `core/capabilities/builtins/sandboxObjectIdentity.test.ts` (categorisation only)

The threshold and ordering algorithm has no tests. Port it together with new table tests.

### Hosted concerns

`Sandbox.writeArtifact` and `sandbox.created`. Replace them with a host filesystem write.

### Port size

**S.** About 240 TS LOC, roughly 150 lines of Rust.

---

## 5. Deferred tools and tool selectors

**Files:**
- `core/runtime/DeferredTool.ts` (351)
- `core/mcp/ToolSelectorPolicy.ts` (116)
- `core/mcp/toolSelectors.ts` (124)
- `core/mcp/ToolSet.ts` (139)
- `core/mcp/convertMCPServers.ts` (95)
- `core/mcp/toolNames.ts` (36)
- `agent-session/schemas/agentSpec.ts:51-110` (config)

### Selectors

Defined in `toolSelectors.ts`, applied in `ToolSelectorPolicy.ts`.

- **Tags:**
  - `@all`
  - `@read-only` (`readOnlyHint===true`)
  - `@write` (`readOnlyHint===false && !destructiveHint`)
  - `@destructive` (`destructiveHint===true`)
- **Where each tag is allowed:** `enable`/`disable`/`preload` accept `@all` and `@read-only`. `require_approval_for_tools` accepts `@all`, `@write` and `@destructive`. Literal tool names are allowed everywhere (`:11-28`).
- **Defaults:** enable `[@all]`, disable `[]`, preload `[]`, approval `[@destructive]`, server `preload:false`.
- **Rules:**
  - A tool is allowed when it matches enable and does not match disable (`:98-112`).
  - An unannotated tool never matches `@write`/`@destructive`/`@read-only`.
  - A literal enable name that is missing from the server is a hard error, 422 (`ToolSet.ts:55-61`).
  - Calling a disallowed tool is a 403 error (`:110-115`).
  - A tool is preloaded if the server has `preload` or the tool matches `preload_tools` (`ToolSelectorPolicy.ts:69-76`).
  - An explicit empty approval list means no approvals at all (`:108-115`).

### Tool registration

`convertMCPServers.ts:23-95`:

1. User servers are skipped unless they have `preload` or at least one preloaded tool. This avoids connections and OAuth prompts for servers that are never used.
2. System toolsets go first, then user servers. Each group is sorted by name, and tools are sorted by name within a server.
3. Only `preload` tools are registered as LLM functions.
4. Names are sanitised to `[a-zA-Z0-9_-]`, truncated to 64 chars, and given a collision suffix from 1 to 50 (`toolNames.ts`).
5. Descriptions get the prefix `mcp server: {name}\n`.
6. **The parameters keep only `properties` and `required`.** Other JSON-schema keys such as `$defs` and `additionalProperties` are dropped. This is a fidelity quirk.

### Deferred tools

`DeferredTool.ts`:

- It is a system toolset named `deferred-tools`, built over **all** user MCP toolsets whenever any exist (`AgentThread.ts:571-576`).
- It exposes `list_tools(mcp_server)` (names only), `get_tool_info` (description, inputSchema, outputSchema), `get_tool_output_schema`, and `call_tool(mcp_server, tool_name, input)`.
- The system prompt lists the non-preload servers with their descriptions truncated to 200 chars, and says to discover tools before calling them (`:82-105`).
- `call_tool` forwards to the underlying `ToolSet.callTool` along with any approval decision (`:302-350`).
- `toolCallInfo`, when resolving the underlying tool, returns the underlying tool's info with `is_deferred:true`. That is how approval gating reaches deferred calls (`:111-144`).
- **Drop `get_tool_output_schema` in the port.** It exists only for Code Mode.

### Tests that pin it

- `core/runtime/deferredLoading.test.ts` (instruction membership)
- `core/mcp/remoteMcpServer.test.ts:191-298` (selectors, 422/403, approval defaults, empty and literal approval lists, deny short-circuit)
- `core/mcp/mcpInitializeAggregation.test.ts`
- `core/helperOwnership.test.ts`

`call_tool` routing and name-collision suffixes have no direct tests.

### Hosted concerns

- `getAllowedToolNamesForSandbox` (Code Mode allow-list, `ToolSelectorPolicy.ts:94-106`). Drop it.
- The OAuth `authRequired` short-circuits. Keep them only if remote OAuth stays in scope.

### Port size

**M.** About 860 TS LOC, roughly 500 lines of Rust.

---

## 6. Dynamic sub-agents

**Files:**
- `core/capabilities/builtins/DynamicSubAgents.ts` (165)
- orchestrator routing (§1)
- `SessionHandle.ts:494-545` (child factory)
- `agent-session/TurnResourceResolver.ts:173-235`
- `builtinsFromSpec.ts:46-54`

### Essential contract

- **The tool.** A root-only `create_sub_agent{name, input[, model enum]}` tool. Its `toolCallInfo` has `is_thread_creation:true`, which keeps the call open and exempt from `OpenToolCallCloser`. The handler returns `{createSubAgent: AgentInfo}` and does no work (`:83-145`).
- **Instruction text:** `:31-52`. It includes the sandbox paragraph, which should be dropped or reworded. The tool description is at `:60-81`. The optional `model` enum comes from `ModelSetConfig`, which nothing in open source wires up (`builtinsFromSpec.ts:48-53` passes none).
- **What a child gets:**
  - The system prompt is `SUB_AGENT_IDENTITY` (`:14-15`), with no user instructions and no `response_format`.
  - `messages = [user: request.input]`, and it shares the parent's `toolSets` instances.
  - It gets the builtins **with `isChild=true`**: no `ask_user_question`, no `create_sub_agent` (so depth is 1), no OpenUI. Compaction and large-tool-response stay on (`builtinsFromSpec.ts:42-67`, `SessionHandle.ts:501-543`).
  - The model override is `agent_info.model ?? spec.model.name` (`TurnResourceResolver.ts:213`).
- **Completion:**
  - A child's final text becomes the parent's tool message.
  - `finish_reason:'length'` becomes an error with the prefix "max_tokens breached" (`AgentThread.ts:1105-1126`).
  - A child that finished but whose delivery to the parent was interrupted is replayed next turn through `preComputedCompletion` (`:1324-1327`).
  - When a parent issues several `create_sub_agent` calls, it waits until every child is done, which is when it becomes a leaf again. Up to 5 children run at once.

### Tests that pin it

- `orchestration/orchestrationWithTools.test.ts`
- `core/runtime/openToolCallCloser.test.ts:65-82`

### Hosted concerns

Tracing sub-spans (`AgentThreadOrchestrator.ts:190-226`) and sandbox sharing. The metrics bucket is optional.

### Port size

**M.** About 165 TS LOC of tool code plus about 150 of orchestrator and factory logic, roughly 300 lines of Rust. Most of the difficulty is in the orchestrator (§1).

---

## 7. Skills

**Files:** `core/sandbox/skills/constants.ts` (28), `core/sandbox/skills/SkillMounter.ts` (135), `core/sandbox/skills/ISkillMounter.ts`, `core/sandbox/Sandbox.ts:278-290,359-361`, and the schema at `agentSpec.ts:233-254`.

### Essential contract (the prompt only)

- A `<skills>` section sits inside `<sandbox>`. It opens with `SKILLS_PREAMBLE`, seven rules about `path`, `SKILL.md` and inline-vs-read (`constants.ts:8-16`).
- Then comes one `<skill>` block per skill (`constants.ts:19-28`):
  - A preloaded skill renders as `path: …\nSKILL.md content:\n…`.
  - Any other skill renders as `path: …\nname: …\ndescription: …`.
- Skill names match `^[A-Za-z0-9._\-/:]+$`, must not be `.` or `..`, and must not start with `.tfy-`. At most 50 skills; `preload` defaults to false (`agentSpec.ts:236-254`).
- The token estimate for skills (`Sandbox.ts:278-280`) only feeds the usage breakdown.

### Hosted: drop entirely

- `getSandboxInit`, `.tfy-requested-skills.json` and the `skill_downloader.py` sparse-clone and tarball download (`SkillMounter.ts:38-118`).
- Registry FQNs and presigned URLs (`packages/trueforge/src/truefoundry/TrueFoundrySkillStore.ts`).
- The DB skill store (`packages/trueforge/src/db/skillStore.ts`, `gitSkillMounts.ts`).
- The validation that "skills require a sandbox provider" (`packages/trueforge/src/runtime/sessionResources.ts:359-375`).

Descriptions currently come from DB rows, not from `SKILL.md`.

### New work for the port

A local loader that scans the configured directories, reads each `SKILL.md` frontmatter for `name` and `description`, and emits the same prompt blocks with absolute host paths. Move the section out of `<sandbox>`.

### Tests that pin it

- `core/sandbox/skills/skillMounter.test.ts:65-69` (prompt blocks)
- `agent-session/agentSpecSkills.test.ts` (name rules)

### Port size

**S.** About 60 TS LOC is worth keeping, plus about 150 lines of new Rust loader code.

---

## 8. MCP tool execution (`executeToolCalls`, `ToolSet`, `RemoteMCP`)

**Files:**
- `core/mcp/executeToolCalls.ts` (196)
- `core/mcp/IMCPServer.ts` (130)
- `core/mcp/ToolSet.ts` (139)
- `core/mcp/LocalToolMCP.ts` (139)
- `core/mcp/ClientSideTool.ts` (39)
- `core/mcp/RemoteMCP.ts` (356)
- `core/mcp/remoteMcpClient.ts` (240)
- `core/mcp/pagination.ts` (34)
- `agent-session/TurnResourceResolver.ts:181-209`

### `executeToolCalls` (`:40-196`)

- All calls run in parallel.
- Arguments are parsed with `JSON.parse(args || '{}')`. A throw becomes `{"error": msg}` with `failure:true`. An unknown tool name becomes `"Tool X not found in tool mapping"`.
- Each response is classified:
  - `createSubAgent` → a thread-creation event, with **no tool message**.
  - `authRequired` → a placeholder tool message ("Waiting for user to authenticate…").
  - `approvalRequired` / `clientSideToolRequired` → collected.
  - A result is rendered as follows:
    - an error result becomes `{"error": content}`
    - `structuredContent` is JSON-encoded
    - text parts are joined with `\n`
    - any other content is JSON-encoded

### `IToolSet` contract

`listTools` / `callTool(params, decision?)` / `toolCallInfo(params, resolveUnderlying?)` plus `preload` and `hasPreloadedTools`.

- `LocalToolMCP` is the base class for in-process tools. It validates input with zod, is never approval-gated, and always preloads (`LocalToolMCP.ts:59-139`). The host tools `read_file`, `bash` and so on should be implemented this way.
- `ToolSet` layers the selector policy (§5) over a policy-free `ToolSource`.

### `RemoteMCP`

- Tries streamable-http first, then SSE. A known transport is used as a hint, and the client falls back to probing if the hint fails (`remoteMcpClient.ts:177-240`).
- Connects single-flight and caches the tool list.
- Pages through tools with a repeated-cursor guard.
- On a 404/"session" error it retries once. The shared socket is detached, not closed, while sibling calls are still in flight (`RemoteMCP.ts:110-210`).
- A 401 fails immediately.
- Carries the MCP session id across turns (`TurnResourceResolver.ts:192`).
- Response bodies are capped at 50 MiB (`remoteMcpClient.ts:34-63`).
- Headers can be static or come from a resolver that signals `authRequired` (OAuth).

### Tests that pin it

- `core/mcp/remoteMcpServer.test.ts`
- `core/mcp/connectRemoteMcp.test.ts`
- `core/mcp/mcpResponseSizeLimit.test.ts`
- `core/mcp/mcpInitializeAggregation.test.ts`
- `core/mcp/LocalToolMCP.test.ts`
- `orchestration/orchestrationWithTools.test.ts`

### Hosted concerns to drop

- The SSRF-guarded fetch (`core/util/ssrfGuard.ts`, 314 LOC). A local user configures their own servers.
- OpenTelemetry header propagation.
- Tracing spans.
- The TrueFoundry gateway metadata headers (`packages/trueforge/src/runtime/sessionResources.ts:34-133`).
- The per-tenant MCP store and OAuth token store (`packages/trueforge/src/db/McpServerWithAuthStore.ts`).

### Gap

**There is no stdio transport in TS.** The map requires stdio MCP, so that is net-new work. `rmcp` covers both transports. Local OAuth is listed under "Not yet specified" in the map.

### Port size

**M.** About 940 TS LOC for execution and the toolset abstractions, plus about 630 for remote. With `rmcp`, roughly 600–800 lines of Rust.

---

## 9. Tool approval policy

**Files:** the logic is spread across `ToolSet.ts:69-126`, `AgentThread.ts:121-136,298-368,1273-1298`, `contextUtils.ts:83-91`, and `schema.ts:68-99` (decision schema).

### Essential contract

1. When the model emits a tool call, `tool_info.is_approval_required` is resolved from the selector (§5) and the tool annotations. For deferred calls the underlying tool is used.
2. The thread derives `user-input-required` and emits `tool.approval_required{tool_calls:[{id, source_event_id}]}`. The turn ends as `done` with `required_actions`.
3. The next turn's input carries `user.tool_approval{thread_id, tool_call_id, approval}`. It must cover every pending id, and it is stored in context as a decision row.
4. The state becomes `tool-response-required`, and `callTool` receives the decision:
   - **allow** → the tool runs.
   - **deny** → the tool result is `{"error":"User denied tool call: <reason | no reason provided>"}` and the tool never runs.
   - A missing decision on a gated tool returns `approvalRequired`, which the loop treats as unreachable (`AgentThread.ts:1190-1192`).
5. Local and system tools are never gated.

### Not implemented

`user.tool_approval_policy` (`allow_session` with an expiry, `schema.ts:101-126`) is schema-only. The inbound-event endpoint just mints ids (`packages/trueforge/src/apis/turns.ts:926-964`). If the TUI wants "always allow", that is new work (ticket 04).

### Tests that pin it

- `orchestration/orchestrationApproval.test.ts`
- `core/mcp/remoteMcpServer.test.ts:242-298`

### Port note

The map says evaluation mode has no per-step approval, so the mechanism must be switchable off, for example with an approval list of `[]`. Guardrails for the host tools belong to ticket 04, not to this selector system.

### Port size

**S.** About 100 lines of Rust on top of §1 and §5.

---

## 10. Instruction building (`InstructionBuilder`)

**Files:** `core/InstructionBuilder.ts` (127), assembled in `AgentThread.ts:581-600`.

### Essential contract

- A tree of XML sections. `addSection(tag, text, cdata?)` trims its input and skips empty text. `beginSection` nests. `build()` joins children with `\n\n`, drops empty sections, and wraps escaped content in `<![CDATA[…]]>` (`:54-127`).
- The assembled order is:
  - `<system-prompt>`
    - `<agent-identity>` (root or sub-agent identity)
    - `<agent-capabilities>`
      - `<internal-messages>` (the tag rule)
      - `[<sandbox>…]`
      - `[<deferred-tools-instructions>]`
      - one section per capability builder, for example `<sub-agents>`
    - `<user-instructions>` in CDATA, **root only**
- `ROOT_AGENT_IDENTITY` is at `:51-52`.
- The system prompt is built once per thread per turn, in the constructor.

### Tests that pin it

There is no direct test. It is covered indirectly by `core/runtime/deferredLoading.test.ts`, `core/sandbox/Sandbox.paths.test.ts` and `core/capabilities/builtins/OpenUI.test.ts`. Write a golden prompt snapshot in Rust.

### Hosted concerns

None. The sandbox, OpenUI and web-search sections simply disappear.

### Port size

**S.** About 80 lines of Rust.

---

## 11. AskUserQuestion

**Files:** `core/capabilities/builtins/AskUserQuestion.ts` (74), `core/mcp/ClientSideTool.ts` (39). Root only (`builtinsFromSpec.ts:42-44`).

### Essential contract

- A tool `ask_user_question{question, options: string[0..5]}`. The long description (`:31-56`) spells out the rules: mutually exclusive options, no "Other" option (the client always renders free text), and a " (Recommended)" suffix on the first option.
- `ClientSideTool.toolCallInfo` sets `is_client_side:true` (`ClientSideTool.ts:26-38`). The thread goes to `user-input-required` **before any execution**, emits `tool.response_required`, and the turn ends.
- The resume is `user.tool_response{thread_id, tool_call_id, content}`, which becomes the tool message.
- If `callTool` is ever reached, it is a guard error.
- The instruction text mentions OpenUI (`:36`). Drop that line.

### Tests that pin it

The core tests only cover the spec default (`agent-session/agentSpecDefaults.test.ts`). The UI side is covered in `packages/assistant-ui-runtime/test/requiredActionInputs.test.ts` and `packages/trueforge-ui/test/utils/toolCallParsing.test.ts`, which are dropped.

### Port note

Headless and evaluation runs need a policy: disable the tool, or auto-answer it. Otherwise a run stalls at `required_actions`.

### Port size

**S.** About 60 lines of Rust, plus a TUI prompt widget.

---

## Also required, outside this ticket's list

`core/llm/VercelAILLM.ts` (1498) implements `ILLM` (`core/llm/ILLM.ts`): streaming deltas, reasoning, usage normalisation, and the final assembled message. The loop depends on four behaviours from it:

- It reports `usage` in the stream. Compaction depends on this.
- It reports `finish_reason`, and `length` in particular.
- It assembles `tool_calls` from the deltas.
- It exposes `reasoning_content` for display. Context replay drops it.

The DeepSeek and Qwen specifics belong to tickets 01 and 07. `CurrentDateTime` (`core/capabilities/builtins/CurrentDateTime.ts`, 49) is a trivial always-on tool, S.

---

## Minimal event set for the TUI

**Outbound, from harness to TUI.** Keep the TS field names so golden transcripts carry over.

| Event | Why |
|---|---|
| `turn.created` / `turn.done{state}` | Turn start and end. `done.required_actions` drives the approval and question prompts. `cancelled` is used for Ctrl-C. `error` carries a message. |
| `model.message` (start marker, then the final message with `content`, `tool_calls[].tool_info`, `reasoning_content`, `finish_reason`, `usage`) | Assistant bubble, token counter |
| `model.message.delta` (`content`, `reasoning_content`, `tool_calls` fragments with `tool_info`) | Live streaming |
| `tool.response{tool_call_id, content}` | Tool result pane. The content is `''` for sub-agent results routed to the parent. |
| `thread.created{parent, agent_info, title}` / `thread.done{state}` | Sub-agent tree |
| `tool.approval_required` / `tool.response_required{tool_calls:[{id, source_event_id}]}` | Approval prompt, `ask_user_question` prompt |
| `agent.context.overwrite{reason:'compaction'}` | **Currently not streamed** (`TurnHandle.ts:443-446`). The TUI will want a "context compacted" notice, so stream a slim form of it. |
| `mcp.initialize` (optional), `mcp.auth_required` (only if remote OAuth stays) | Status line and the auth prompt |

**Inbound, from TUI to harness:** `user.message` (string only), `user.tool_approval`, `user.tool_response`.

**Drop:** `sandbox.created`, `turn.update`/`paused` (never emitted), `user.tool_approval_policy`, `user.mcp_auth_continue`, passthrough events, the internal `CAPABILITY_STATE` event, and the SSE sequence and resume envelope.

**Internal only, never shown in the TUI:** `internal.agent.context.append` (the persistence carrier), `internal.agent.create_subagent`, `internal.agent.done`.
