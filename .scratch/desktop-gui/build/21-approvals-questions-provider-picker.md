# 21: Approvals, questions and the provider picker

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** M (one agent session)

**What to build:** Required actions end to end, and the provider selector with the destination shown.

**Blocked by:** 20

**Status:** done

**Files and components likely to change:**
- `gui/src/renderer/state/*` (pending actions), `components/dialogs/ApprovalDialog.tsx`, `components/home/ModelPicker.tsx`, question UI
- `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts`

**Acceptance criteria:**
- [x] Approvals queue oldest first and show "1 of N"; each dialog shows the tool, command, working directory, matched rule and reason, with Deny focused by default; replying removes the action optimistically and a sidecar `unknown_action` just drops it.
- [x] `ask_question` renders its question UI and the answer from `tool.response`; a pending action is removed by reply, by `turn.done`, and by `workspace.list`'s `pending_action_ids`; none survive a finished turn; cancelling closes open dialogs.
- [x] The provider picker lists the options from `provider.list` with model and the authority the key is sent to, marks the active one, switches the session, and shows the one-model-per-provider note.
- [x] e2e `edit-and-approve` for allow once, allow for session, deny and text answers, and `question-and-answer`; the stub's request log shows the same conversation as the in-process seams.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 07 sections 3 and 5; ticket 04 screens 5 and 6.

**Completion record:**
- State: `TurnView.pending` holds approvals and questions oldest first. `tool.approval_required` marks the card `awaiting_approval` and queues the action; `tool.response_required` queues a question; a `tool.response` removes the approval for its call (and a question's, for `ask_question`); `turn.done` clears the list; a stored turn that never finished clears it too. In `transcripts.ts`, `reply` removes the action at once and then sends `action.reply` (a refusal such as `unknown_action` changes nothing), Stop clears the session's pending actions at once, and `reconcile` drops the actions that were waiting before a `workspace.list` request and are missing from its `pending_action_ids`; it runs after a resume and when the sidecar reports ready. `Client` gained `replyAction`, `pendingActionIds` and `switchProvider`.
- UI: `ApprovalDialog` (tool, command or path, workspace, matched rule, why, "1 of N", Deny focused, Allow once, Allow for this session), `QuestionDialog` (options as buttons plus a free-text answer, sent as `{text}`), a ported `ui/dialog` that Escape and an outside click cannot dismiss, an "awaiting approval" tool card with its rule, and an `ask_question` card that shows the answer from `tool.response`. In a session the provider picker lists `provider.list` with model and the scheme and authority the key goes to, marks the active one, switches the session with `session.switch_provider` (a refusal leaves it as it was) and keeps the one-model-per-provider note. `@radix-ui/react-dialog` 1.3.1 was added as an exact devDependency.
- Checked: `make gui-lint`; `make gui-test` (235 tests: reducer queueing and removal rules, dialogs, ordering, counts, replies, refusals, Escape, Stop, turn end, reconcile after restart, questions, provider switching and refusal); `make gui-e2e` (26 passed; `edit-and-approve` allow once then deny, allow for this session skipping the second prompt, `question-and-answer` with a typed answer, a turn cancelled while it waits, and `bash-and-test` now approved through the dialog; the stub's request log is checked with the scenario's own `expect`). Mutations caught: pending surviving `turn.done`, reconcile off, and the optimistic removal.
- Deviations: (1) the dialogs show for the session on screen; a session elsewhere keeps waiting with no badge yet. (2) "Why you're asked" is a fixed sentence: the event carries the rule text only. (3) Deny is not bound to Escape on purpose. (4) The unit test for Stop clicks the button while the modal is open, because jsdom has no inert pages; the e2e test cancels through the protocol, since a real modal blocks the page. (5) Text answers apply to questions; approvals have no text reply in the dialog. (6) The sidecar's run-time guarantee that the first request after an allow-for-session skips the prompt is checked only by the e2e test above.
- Not covered: Linux; pending actions of sessions not on screen.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
