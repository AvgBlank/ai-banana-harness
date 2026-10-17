# 21: Approvals, questions and the provider picker

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** M (one agent session)

**What to build:** Required actions end to end, and the provider selector with the destination shown.

**Blocked by:** 20

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/renderer/state/*` (pending actions), `components/dialogs/ApprovalDialog.tsx`, `components/home/ModelPicker.tsx`, question UI
- `gui/tests/unit/renderer/*`, `gui/tests/e2e/scenarios.spec.ts`

**Acceptance criteria:**
- [ ] Approvals queue oldest first and show "1 of N"; each dialog shows the tool, command, working directory, matched rule and reason, with Deny focused by default; replying removes the action optimistically and a sidecar `unknown_action` just drops it.
- [ ] `ask_question` renders its question UI and the answer from `tool.response`; a pending action is removed by reply, by `turn.done`, and by `workspace.list`'s `pending_action_ids`; none survive a finished turn; cancelling closes open dialogs.
- [ ] The provider picker lists the options from `provider.list` with model and the authority the key is sent to, marks the active one, switches the session, and shows the one-model-per-provider note.
- [ ] e2e `edit-and-approve` for allow once, allow for session, deny and text answers, and `question-and-answer`; the stub's request log shows the same conversation as the in-process seams.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 07 sections 3 and 5; ticket 04 screens 5 and 6.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
