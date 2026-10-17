# 24: Resilience: reload, crash, quit and link confirmation

**Lane / milestone:** G · M6 Full v1 behaviour   **Size:** L (one agent session)

**What to build:** Everything that keeps the app honest when things go wrong (ticket 06 sections 1 and 5, ticket 07 section 4). Split point if over budget: live buffer and reload (24a) then crash, quit and links (24b).

**Blocked by:** 21

**Status:** done

**Files and components likely to change:**
- `gui/src/main/{live/buffer.ts,quit.ts}`, `main/ipc/*` (`live.attach` returns `{frames, active_turn_id}`), `main/external/links.ts`
- `gui/src/renderer/components/{banners/CrashBanner.tsx,dialogs/QuitDialog.tsx,dialogs/LinkDialog.tsx}`, renderer reconnect logic
- `gui/tests/unit/*`, `gui/tests/e2e/resilience.spec.ts`

**Acceptance criteria:**
- [x] Main keeps a bounded (about 5 MiB) per-workspace buffer of the current or most recent turn's frames until the next `turn.start`, `workspace.close` or a crash; `live.attach` returns the frames and `active_turn_id` in one synchronous step; the renderer attaches to every workspace's active turn at start and before `turn.start`.
- [x] Reload: after resume and attach the transcript, partial message and pending approval are restored with no duplicate event ids in the DOM; an attach that lands just after `turn.done` still ends `done`; a buffer overflow is repaired by the final message.
- [x] A sidecar crash marks running turns interrupted, clears pending actions, disables the composer, shows the banner ("Restarting... attempt n of 3", then Restart with the redacted stderr tail), and after `ready` resumes the visible session; the interrupted turn comes from `state: null`; nothing is re-sent.
- [x] Quit with a running turn asks to confirm; confirming sends `shutdown`, SIGTERM and SIGKILL within the limits; killing main leaves no orphan sidecar.
- [x] External links are confirmed with the full URL and a per-session "don't ask again for this host"; MCP OAuth URLs go through the same path.
- [x] e2e: `slow-stream-gated` reload mid-stream and mid-approval; sidecar killed mid-turn; quit while running; a second launch focuses the first window.

**Verification** (commands to run; none has been run by the planning):

```
make gui-lint
make gui-test
make gui-e2e
```

**Documentation:** None.

**Notes:** Specification: ticket 06 sections 1 and 5; ticket 07 section 4.

**Completion record:**
- Main: `live/buffer.ts` keeps each workspace's current or latest turn (a new `turn.created` replaces it, `workspace.close` and any non-running sidecar state empty it, about 5 MiB with the oldest frames dropped first) and knows whether a turn is running. `live.attach` returns `{frames, activeTurnId}` for a session in one synchronous step, and `sidecar.state` returns the supervisor's state, so a reloaded window does not wait for a push that already happened. `quit.ts` (`QuitGuard`) asks "A turn is running. Quit and cancel it?" in a native dialog when a turn runs, then runs the supervisor's shutdown (`shutdown`, stdin close, SIGTERM, SIGKILL); a second quit takes the kill path. A renderer crash reloads the window and leaves the sidecar alone. `window.open` goes through the confirmed external-link path instead of being dropped, and an `mcp.auth_required` event's address is opened through the same path (validated, full URL shown, "don't ask again for this host" for the run). A failure after the restart budget now carries the redacted stderr tail. `SidecarState` and `FailureReason` moved to `shared/api.ts` as the one definition.
- Renderer: `load` resumes and then attaches; a stored turn without an end is interrupted unless it is the sidecar's running turn, buffered frames are deduplicated by event id, and an attach that lands after `turn.done` ends done. At start and whenever the sidecar returns to ready the renderer attaches to every workspace's running turn. A crash or failure interrupts every running turn, clears pending actions, closes the dialogs and disables the composer; `CrashBanner` shows "Restarting... attempt n of 3", then Restart with the stderr tail on request; after ready the visible session is resumed again and nothing is re-sent.
- Checked: `make gui-lint`; `make gui-test`; `make gui-build`; `make gui-e2e` (42 passed; `resilience.spec.ts` has 8: reload mid-stream on a gated stream, reload mid-approval, a sidecar killed mid-turn, quit while running (cancel, then confirm and the sidecar gone), killing the app leaves no orphan sidecar, a second launch focuses the first window and starts no second sidecar, a crashed renderer is reloaded, and a window.open link asks with the full address and remembers a trusted host); `make lint`; `make test`; `make gui-leak-scan`. Mutations caught: the live-turn exception for interrupted turns, replacing the buffer at a new turn, and the quit confirmation.
- Deviations: (1) Main pushes every frame to the window as before and the buffer is only a snapshot, so attaching before `turn.start` would add nothing: the renderer subscribes first and attaches after, and event ids remove the overlap. (2) The quit and link confirmations are native dialogs owned by main, so no renderer dialog can stand in for them; there is no `QuitDialog.tsx` or `LinkDialog.tsx`. (3) The buffer is replaced at `turn.created`, which follows `turn.start`. (4) A crashed renderer is reloaded without a limit on repeated crashes (a `ponytail:` note). (5) A reload after the Playwright page handle's target crashed is checked from main, since the handle does not follow a crashed target.
- Not covered: Linux and packaging; the `failed` state after three crashes is covered by unit tests only; a sidecar that hangs without exiting (no ping in the protocol).

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.
