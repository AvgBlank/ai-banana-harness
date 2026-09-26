# Inventory of core-loop behaviour to port

Type: research
Status: resolved
Blocked by:

## Question

In `packages/trueforge-core` and the parts of `packages/trueforge` that drive it, which behaviours of the retained features are real logic the port must reproduce, and which are hosted-only glue to drop?

Retained features:
- the thread loop / `AgentThreadOrchestrator`
- turn and event schema
- context compaction
- large-tool-response handling
- deferred tools and tool selectors
- dynamic subagents
- skills
- tool approval policy
- MCP tool execution
- instruction building

For each, give:
- the files involved
- its essential algorithm or contract, stated briefly
- the tests that pin it down
- the dependencies on hosted concerns (Redis, tenants, sandboxes, HTTP)
- a rough size for the port

## Research

Findings: branch `research/core-loop-inventory`, file `docs/research/core-loop-inventory.md` (commit 5f27ed84).

## Answer

About 6.5k lines of in-scope TS reduce to roughly 3.5–4.5k lines of Rust. The thread loop is the only large piece; everything else is small or medium. The details are in `docs/research/core-loop-inventory.md` on the `research/core-loop-inventory` branch.

**What must be ported:**
- **Context-derived thread state:** the open tool calls on the last assistant message decide whether the loop calls the LLM, runs tools, or waits for the user.
- **Scheduling:** the orchestrator runs only leaf threads, 5 subagents at a time.
- **Four capability hooks** carry the rest.
- **Compaction:** fires when usage + 931 ≥ threshold. The threshold is an explicit trigger, else min(80% of context length, context length − max output), else 50,000. It replaces the context with a summary plus a continuation message.
- **Large tool responses:**
  - a batch under 6,000 tokens (chars/4) passes through untouched
  - otherwise every result of 6,000+ tokens is blocked, then the largest others until the total is under 10,000
  - previews keep the first and last 100 chars
  - failures are cut to 500 chars
- **Selector defaults:** `@all` enabled, approval `@destructive`, servers deferred.
- **Iteration limits:** 100 in the spec, 25 inside the thread.

**Drop:**
- session forking, ancestors and executor freezing
- Redis/SSE resume and passthrough events
- capability-state storage
- `turn.update`/paused, which is never emitted
- `sandbox.created`
- Code Mode, the SSRF guard and tracing

**New work that TS never had:**
- a stdio MCP transport
- a local `SKILL.md` loader (TS reads skill descriptions from the database)
- spilling large tool responses to a workspace folder, since the sandbox that held them is gone
- a streamed compaction event for the TUI
- a real "allow for session" approval policy (TS has only a schema stub)

**Untested in TS, so it needs new tests or golden transcripts** (input to the Test and parity strategy ticket): the large-response algorithm, the compaction flow, the iteration limit, the `length` finish reason and the 5-at-a-time scheduling.
