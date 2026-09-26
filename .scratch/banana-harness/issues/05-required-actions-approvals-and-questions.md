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

**Status:** ready-for-agent

- [ ] Seam-1 tests with an `ask` rule: allow runs the tool, deny returns the denial error, and allow-for-session skips the prompt on the next identical call.
- [ ] Seam-1 test: the question tool's answer becomes the tool result, and the turn continues within the same turn.
- [ ] Seam-1 test: cancelling while a required action is pending ends the turn as cancelled without hanging.
- [ ] Headless test: a question is auto-answered and the run exits with code 0.
- [ ] Seam-2 tests: the approval modal and the question modal render.
