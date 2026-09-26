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

**Status:** ready-for-agent

- [ ] Seam-1 test: when usage crosses the threshold, the next model request contains only the summary plus the continuation, and a context-compacted event is emitted.
- [ ] Seam-1 tests: a single huge tool result is spilled to a file whose content matches the original, and the model receives a path plus the preview. Mixed-size batches follow the 6,000/10,000 rules. A failure is truncated.
- [ ] Seam-1 test: after a spill, `git status` in the workspace is clean except for the agent's intended changes.
- [ ] Seam-2 test: the compaction notice renders.
