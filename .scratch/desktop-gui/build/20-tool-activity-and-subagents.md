# 20: Tool cards, subagent groups and turn summaries

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** M (one agent session)

**What to build:** Complete the reducer and the transcript for everything the agent does: tool cards, blocked calls, subagent groups, compaction notices, interrupted state and the per-turn summary.

**Blocked by:** 18

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/renderer/state/{reduceTurn.ts,diff.ts}`
- `gui/src/renderer/components/session/{ToolCard.tsx,SubagentGroup.tsx,TurnSummary.tsx}`
- `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts`

**Acceptance criteria:**
- [ ] `tool.call` and `tool.response` update a card in place; blocked calls show "Blocked by guardrail <rule>"; `bash` shows an `exit N` chip parsed from the documented first line; invalid argument JSON keeps the card with raw text; output over 200 lines or 64 KiB collapses with "Show all".
- [ ] The diff parser skips everything before the first `@@` (the `--- before` / `+++ after` headers), handles `\ No newline at end of file`, and counts lines correctly; `edit` and `write_file` cards show `+N -M` and an inline diff.
- [ ] Subagent threads nest in their `create_sub_agent` card, matched by title (oldest unresolved on no match); the group status rolls up; no child appears in the root list.
- [ ] `context.compacted` shows a notice; a turn with no `turn.done` and no active turn is `interrupted` with its running tools marked; the summary card derives "Edited N files +A -R" and the duration from the events (same live and replayed).
- [ ] Replay-equivalence tests for `bash-and-test` and `subagents`; e2e `bash-and-test` and `subagents` through the real sidecar.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 07 sections 3 and 5.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
