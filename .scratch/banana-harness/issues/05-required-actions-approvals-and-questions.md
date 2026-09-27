# 05: Required actions: approvals and questions wait in place

**What to build:** required actions that keep the turn running (ADR 0002).

**Approvals:**
- A tool whose permission resolves to `ask` raises tool-approval-required and parks the thread until the turn handle's reply arrives.
- The TUI shows an approval modal: `a` allow once, `s` allow for session (kept in memory), `d` deny.
- Deny returns "User denied tool call: <reason>" to the model.

**Questions:**
- The root-only question tool (a question plus up to five options) raises tool-response-required.
- The TUI shows a question modal with the options plus free text.

**Cancel and headless runs:**
- Cancel answers every pending required action with deny.
- In a headless run, questions are auto-answered with "No user available; proceed with your best judgement", and `ask` resolves to deny.

**Blocked by:** 04

**Status:** done

- [x] Seam-1 tests with an `ask` rule: allow runs the tool, deny returns the denial error, and allow-for-session skips the prompt on the next identical call.
- [x] Seam-1 test: the question tool's answer becomes the tool result, and the turn continues within the same turn.
- [x] Seam-1 test: cancelling while a required action is pending ends the turn as cancelled without hanging.
- [x] Headless test: a question is auto-answered and the run exits with code 0.
- [x] Seam-2 tests: the approval modal and the question modal render.

## Notes

For ticket 11 (parallel, same base — expect an `event.rs`/`app.rs`/`ui.rs` merge conflict, all
mechanical), 12 (`/undo` HELP_COMMANDS), 14 and 15.

**Events (`harness-core/src/event.rs`):** two new `EventKind` variants, `ToolApprovalRequired
{action_id, thread_id, tool_call_id, name, arguments, rule}` and `ToolResponseRequired {action_id,
thread_id, question, options}`. Both need `name()`/`thread_id()` arms; ticket 11's
`McpAuthRequired` should follow the same shape.

**`TurnHandle` (`harness-core/src/harness.rs`):** gained `reply(action_id, Answer)` (spec's exact
API). `Answer { Allow, AllowForSession, Deny, Text(String) }` (`harness.rs:26`, re-exported from
`lib.rs`) is the one reply payload for both an approval and a question — a waiter matches only the
variants it expects and falls back sensibly on the other (a question waiter gets `Deny` back if
cancelled, an approval waiter gets `Deny` if handed a stray `Text`). `PendingActions =
Arc<Mutex<HashMap<String, oneshot::Sender<Answer>>>>` (`:38`) is shared between `TurnHandle` and
`TurnRunner`, created fresh per turn in `start_turn`. `reply` removes-then-sends, so a reply to an
already-resolved id (double-answer, or after cancel) is a no-op.

**Waiting in place (ADR 0002):** `TurnRunner::wait_for_answer(action_id, headless_default)`
(`harness.rs:~715`) is the one parking point: a headless harness (`Inner.headless`, set from the
new `HarnessOptions.headless`) returns `headless_default` immediately without ever touching the
pending map, so it can never hang; otherwise it registers a oneshot and
`tokio::select!`s (biased) on `self.cancel.cancelled()` vs. the oneshot — cancelling resolves
*every* currently-parked waiter to `Deny` independently (each one is its own `select!`), which is
what makes "cancel answers every pending required action with deny" true without a separate
sweep of the pending map. `request_approval`/`request_question` build the event, emit it, then
call this. The event is still emitted in a headless run (so JSON-lines observers see it); only
the wait is skipped.

**Approvals plumbing:** the permission engine returns a `Verdict` now, not a bare `String`
(`harness-tools/src/guardrail.rs`): `Verdict::Deny(String)` (unchanged wording) vs. `Verdict::Ask
(AskRequest{key, pattern, rule})`. `check`/`check_command`/`path` take a new `overrides:
&AskOverrides` parameter (`AskOverrides` is a `Mutex<HashSet<(key, pattern)>>`, `pub fn
new/allow/`, private `is_allowed`); an already-allowed `(key, pattern)` resolves `Ask` straight to
`Ok(())`, so "allow for session" needs no retry-with-a-flag trick — just record the pair and
re-run the same check. `check_mcp` is **unchanged** (still treats `ask` as deny): MCP approvals
aren't wired, since no acceptance criterion needs them; the `mcp` permission key still works, it
just can't pause. `ToolContext` gained `ask_overrides: &AskOverrides`; every host tool's
`ctx.permission.check(...)`/`check_command(...)` call site (`bash.rs`, `glob_tool.rs`,
`grep_tool.rs`, `ctx.path` in `lib.rs`) now goes through the new `crate::map_verdict` free fn,
which turns `Verdict::Deny` into the existing `ToolResult::blocked` and `Verdict::Ask` into a new
`ToolResult::ask(AskRequest)` (`needs_approval: Some(Box<AskRequest>)` — boxed only to keep
`ToolResult` under clippy's `result_large_err` threshold; nothing else about the field changed).
`ToolResult::denied(reason)` is the "User denied tool call: `<reason>`" result (`blocked: true`,
matching the existing red-badge convention for blocked/denied calls).

**The retry (`harness-core/src/tools.rs` + `harness.rs`):** `TurnTools` now holds
`ask_overrides: Arc<AskOverrides>` (constructed once in `Inner`, shared across every turn of the
harness — this is the "session" in "allow for session"; it does **not** live on the per-turn
`TurnTools`, which is rebuilt every turn). `TurnTools::call` gained a second parameter,
`once_override: Option<(String, String)>`: `None` checks normally (which is also what a *second*
call after `allow_for_session` sees, since the override is now recorded); `Some((key, pattern))`
builds a throwaway one-entry `AskOverrides` for just that single call, so "allow once" never
touches the shared session set. `harness.rs`'s new `TurnRunner::call_host_tool` is the actual
orchestration: call once (`None`) → if `needs_approval` is set, `request_approval` → on the
answer, either return `ToolResult::denied`, retry with `Some((key, pattern))` (once), or call
`tools.allow_for_session(key, pattern)` then retry with `None` (session). `run_tool_calls`'s host
branch now spawns `runner.call_host_tool(...)` (a `TurnRunner` clone) instead of calling
`tools.call` directly.

**Question tool (`harness-core/src/builtins.rs`):** `Builtin::AskQuestion` (`ask_question`),
root-only, always enabled (no config toggle — the ticket doesn't ask for one).
`QuestionRequest{question, options}` caps `options` at `MAX_QUESTION_OPTIONS = 5`.
`run_tool_calls` collects `(index, QuestionRequest)` the same way it collects subagent requests,
then answers them **sequentially after** the subagent batch (not in parallel with host calls):
the TUI has one overlay slot, so only one question can ever be on screen at a time, and
sequential awaiting is the honest model of that. A question's answer only ever becomes
`ToolResult::ok(text)` — there's no "denied" question, per the ticket (only approvals deny).

**Headless (`banana-harness/src/main.rs`):** `HarnessOptions.headless: true` in `headless()`;
`harness-tui/src/lib.rs`'s `open_harness` passes `false`. No other change was needed in `main.rs` —
the required-action events just flow through the existing `--json`/plain-text branches like any
other event (plain text's `_ => {}` ignores them, matching how it already ignores most kinds).

**TUI (`harness-tui`):** `Overlay` (already a single-slot `Option<Overlay>` on `MainState` since
ticket 13) gained `Approval{action_id, name, arguments, rule}` and `Question{action_id, question,
options, free_text}`. `apply_event` intercepts both new `EventKind`s at the *top level* (not
through the per-thread `apply_thread_event` dispatch), because an approval can come from a
**subagent** thread too (only the question tool is root-only) and opening a modal is a
`MainState`-level concern regardless of which thread asked — routing it through the thread-scoped
dispatcher would have needed passing `&mut MainState` through a function that only ever gets
`&mut Vec<TranscriptItem>` today. `apply_thread_event`'s match still needs no-op arms for both
variants (Rust's exhaustiveness applies to the whole `EventKind`, even though these two never
reach that function at runtime). On `TurnDone`, a still-open `Approval`/`Question` overlay is
cleared — the only way that can happen is cancel racing ahead of a reply (every waiter resolves to
`Deny` on cancel independently; nothing else closes the overlay).

Keys (`interpret_overlay_key` in `lib.rs`): Approval is `a`/`s`/`d` exactly as the ticket specifies,
each producing a new `Action::Reply{action_id, answer}` that `handle_terminal_event` turns into
`turn.reply(...)` plus `close_overlay()`. Question is digits `1`-`9` (submits that option
immediately) or free text plus Enter; Backspace edits the free-text buffer. **Esc on either modal
is `Action::Cancel`, not a bare close** — the turn is waiting on this action, and dismissing the
overlay without answering would leave it parked forever; the overlay then clears itself once
`turn.done` arrives (see above). This is a deliberate deviation from the generic "Esc closes an
overlay" rule the other overlays follow, called out in `Overlay`'s doc comment.

**Tests:** `harness-core/tests/approvals.rs` (seam 1) — the 4-round ask/allow/deny/allow-for-session
sequence (asserting the 4th identical call raises *no* approval event at all), the question
round-trip, cancel-while-pending, and the headless auto-answer. `harness-core/tests/
workspace_changes.rs`'s pre-existing `overlay_bash_patterns_keep_their_written_order` used to
assert `ask` behaved as an outright block; it now drives the real approval-required/deny round
trip instead, so the test still proves the overlay resolved that pattern to `ask` specifically
(not allow/deny) rather than losing that coverage. `harness-tui/tests/required_actions.rs`
(seam 2) — both modals render, and the turn-done-clears-a-stale-overlay case.
`harness-core/tests/mcp.rs` and `tool_calls.rs` had two pre-existing hardcoded tool-name-list
assertions that needed `"ask_question"` added (unrelated to their own behavior; the new builtin
is just always in the list now).

**Deviations / simplifications:**
- MCP tool "ask" rules are still treated as deny (`check_mcp` unchanged) — no acceptance criterion
  needs it, and wiring it would mean threading approvals through `McpRegistry::call` too. Left for
  whoever next touches MCP permissions.
- The question modal has no up/down option navigation, only direct digit-select or free text —
  simpler than the sessions picker's cursor, and the ticket only asks for "options plus free
  text," not cursor navigation.
