# 20: Tool cards, subagent groups and turn summaries

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** M (one agent session)

**What to build:** Complete the reducer and the transcript for everything the agent does: tool cards, blocked calls, subagent groups, compaction notices, interrupted state and the per-turn summary.

**Blocked by:** 18

**Status:** done

**Files and components likely to change:**
- `gui/src/renderer/state/{reduceTurn.ts,diff.ts}`
- `gui/src/renderer/components/session/{ToolCard.tsx,SubagentGroup.tsx,TurnSummary.tsx}`
- `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts`

**Acceptance criteria:**
- [x] `tool.call` and `tool.response` update a card in place; blocked calls show "Blocked by guardrail <rule>"; `bash` shows an `exit N` chip parsed from the documented first line; invalid argument JSON keeps the card with raw text; output over 200 lines or 64 KiB collapses with "Show all".
- [x] The diff parser skips everything before the first `@@` (the `--- before` / `+++ after` headers), handles `\ No newline at end of file`, and counts lines correctly; `edit` and `write_file` cards show `+N -M` and an inline diff.
- [x] Subagent threads nest in their `create_sub_agent` card, matched by title (oldest unresolved on no match); the group status rolls up; no child appears in the root list.
- [x] `context.compacted` shows a notice; a turn with no `turn.done` and no active turn is `interrupted` with its running tools marked; the summary card derives "Edited N files +A -R" and the duration from the events (same live and replayed).
- [x] Replay-equivalence tests for `bash-and-test` and `subagents`; e2e `bash-and-test` and `subagents` through the real sidecar.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 07 sections 3 and 5.

**Completion record:**
- `state/reduceTurn.ts` now covers `tool.call` and `tool.response` (cards update in place; blocked calls carry the guardrail rule; bash output loses its `exit code: N` line and keeps the code; invalid argument JSON keeps the card with raw text), `context.compacted` notices, `thread.created` and `thread.done`, and derives the summary (`summarize`: files counted once across edits, lines summed, `edit` and `write_file` only, subagent edits included) and the duration from ULID timestamps (`ulidMs`; no clock). Subagent threads live in `TurnView.threads` and attach to the oldest unclaimed `create_sub_agent` card whose `name` equals the title, else the oldest unclaimed one; `groupStatus` rolls up error over interrupted over running over done. A stored turn with no state and no `turn.done` is interrupted, with its running tools and threads marked. New: `state/diff.ts` (skips everything before the first `@@`, treats `\ No newline at end of file` as an unnumbered note, counts lines) and `state/output.ts` (200 lines or 64 KiB, cut on a character boundary).
- UI: `ToolCard` (summary line, `+N −M` chip, `exit N` chip, spinner and state icons, inline diff, "Show all" for clipped output), `SubagentGroup` with a recursive `ItemList` (collapsible, status badge, child errors), `TurnSummary` ("Edited N files +A −R", duration, "View changes"), `RichText`, and the shared `DiffView` now on the new parser. `SessionView` renders turns through them.
- Checked: `make gui-lint`; `make gui-test` (219 tests: diff and output edge cases, reducer behaviour for cards, blocked, subagent matching and roll-up, interruption, summary and duration, replay equivalence for the bridge's plain-answer, edit-and-approve, single-subagent and compaction golden frames and a bash-and-test shaped turn, and the rendered transcript); `make gui-e2e` (22 passed; new `bash-and-test` and `subagents` specs use the scenarios' own replies through the real sidecar and check the cards again after a reload and resume). Mutations caught: subagent title matching, the bash exit parsing, the header skip and, after adding a test, counting a diff on a non-edit tool.
- Deviations: (1) `bash-and-test` needs an approval and the approval dialog is a later ticket, so the e2e test plays the user's part by answering `tool.approval_required` with Allow over `action.reply`. (2) The summary card has "View changes" but no Undo button (undo belongs to a later ticket). (3) A running turn that loses its sidecar is marked interrupted only on replay, not live. (4) Question cards (`ask_question`) and awaiting-approval card states are not rendered yet. (5) Long transcripts are not virtualised; no `content-visibility` styling was added. (6) The prototype's `+N −M` for a card comes from the parsed diff, not a separate event.
- Not covered: Linux; `mcp.auth_required` rendering.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
