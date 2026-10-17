# 24: Resilience: reload, crash, quit and link confirmation

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** L (one agent session)

**What to build:** Everything that keeps the app honest when things go wrong (ticket 06 sections 1 and 5, ticket 07 section 4). Split point if over budget: live buffer and reload (24a) then crash, quit and links (24b).

**Blocked by:** 21

**Status:** open (not started)

**Files and components likely to change:**
- `gui/src/main/{live/buffer.ts,quit.ts}`, `main/ipc/*` (`live.attach` returns `{frames, active_turn_id}`), `main/external/links.ts`
- `gui/src/renderer/components/{banners/CrashBanner.tsx,dialogs/QuitDialog.tsx,dialogs/LinkDialog.tsx}`, renderer reconnect logic
- `gui/tests/unit/*`, `gui/tests/e2e/resilience.spec.ts`

**Acceptance criteria:**
- [ ] Main keeps a bounded (about 5 MiB) per-workspace buffer of the current or most recent turn's frames until the next `turn.start`, `workspace.close` or a crash; `live.attach` returns the frames and `active_turn_id` in one synchronous step; the renderer attaches to every workspace's active turn at start and before `turn.start`.
- [ ] Reload: after resume and attach the transcript, partial message and pending approval are restored with no duplicate event ids in the DOM; an attach that lands just after `turn.done` still ends `done`; a buffer overflow is repaired by the final message.
- [ ] A sidecar crash marks running turns interrupted, clears pending actions, disables the composer, shows the banner ("Restarting... attempt n of 3", then Restart with the redacted stderr tail), and after `ready` resumes the visible session; the interrupted turn comes from `state: null`; nothing is re-sent.
- [ ] Quit with a running turn asks to confirm; confirming sends `shutdown`, SIGTERM and SIGKILL within the limits; killing main leaves no orphan sidecar.
- [ ] External links are confirmed with the full URL and a per-session "don't ask again for this host"; MCP OAuth URLs go through the same path.
- [ ] e2e: `slow-stream-gated` reload mid-stream and mid-approval; sidecar killed mid-turn; quit while running; a second launch focuses the first window.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 06 sections 1 and 5; ticket 07 section 4.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
