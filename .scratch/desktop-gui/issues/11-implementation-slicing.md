# Slice the build into implementation tickets

Type: grilling
Status: resolved
Blocked by:

## Question

Every decision the build depends on is now made (tickets 00 to 10). Turn them into the last part of the destination: a `spec.md` in this folder with dependency-ordered implementation tickets, each sized for one agent session and each with acceptance criteria an agent can run itself (as `.scratch/rust-tui-migration/spec.md` does for the Rust port). Cover, in this order of dependency:

- the Rust side: [core additions](10-core-api-additions.md) (serde, `transcript`, `workspace_changes`/`turn_changes` with their `git` and `checkpoint` flags, `rename_session`, `providers()` with authority and scheme, `last_provider`, `turn.undone`, shared config paths, the public redactor), `harness-testkit` (fake provider move, scenarios, `TestEnv`, `fake-provider` binary), `harness-bridge` and `serve --stdio`, and the Makefile and workspace changes;
- the docs: apply the [ticket 09 drafts](09-adr-and-vocabulary.md) in the same commit as `harness-bridge`;
- the desktop app: `gui/` scaffold and toolchain, ported frozen tokens and components with the accessibility fixes, the Electron main process (supervisor, key resolver, trust, preferences, IPC), the preload surface, the renderer state and reducer, the screens in the order of the [prototype](04-banana-design-system.md), the Changes panel and undo flow, packaging, and CI;
- the test layers from tickets 03, 05, 06, 07 and 08 attached to the ticket that introduces each behaviour.

Also settle what is still marked "not yet specified" on the map only if it blocks a ticket; otherwise leave it there. Planning only: no implementation.

## Answer

Resolved 2026-10-17. The plan is [`spec.md`](../spec.md) with **27 implementation tickets** in [`build/`](../build/), in dependency order, each with scope, likely files, acceptance criteria (152 checkboxes in all) and verification commands. **Nothing was built, installed or applied by this ticket**; the commands are specified, not run. Everything below was checked by script over the generated files: tickets numbered 1 to 27 with no gaps, no blocker that points forward or is missing, every ticket transitively blocked on ticket 01, every link in the spec and the tickets resolving, every ticket carrying all required sections, and the staged-documentation table agreeing with the nine tickets that apply documentation.

**Shape.** Lanes: documentation (01), Rust core (02 to 06), test kit (07, 08), bridge (09 to 12), GUI (13 to 18 and 20 to 25), CI (19), packaging (26), final acceptance (27). The GUI is sliced vertically: ticket 18 is a tracer bullet (composer, real sidecar, fake provider, streamed answer) and every later GUI ticket widens a working slice, with CI added immediately after it (19).

**Milestones**, each verifiable offline against the fake provider before the next starts: M0 docs; M1 core additions; M2 test foundation; M3 sidecar proven at two seams; M4 GUI foundation (toolchain, supervisor, hardened window, e2e harness, shell); M5 first session (tracer bullet and CI); M6 full V1 behaviour (tools, approvals, changes, undo, trust, resilience, settings, the user's visual parity review); M7 ship (unsigned DMG and ZIP, clean-checkout acceptance).

**Documentation drafts.** They remain unapplied. Ticket 01 applies the decision and vocabulary documents first and blocks everything else; the rest of the draft text is applied ticket by ticket (07, 09, 13, 15, 17, 25, 26), each hunk landing with the code that makes it true, and ticket 27 proves the repository matches the drafts. The reasoning and the full table are in spec section 7.

**Decisions made while slicing (none reopens an earlier ticket):**
- Each bridge ticket (09 to 11) commits the golden frames for the methods it adds, so the GUI's contract tests can start after ticket 09 instead of waiting for the whole bridge.
- The Playwright harness and `make gui-e2e` are created in ticket 15 (the first ticket that launches the real app), not later.
- Settings lists only the shortcuts that exist (Enter, Shift+Enter, Esc); the global shortcuts in the prototype were never specified and stay out of V1, consistent with the map's "not yet specified" fog.
- Tickets flagged L name their split point, so a ticket that overruns a session is split instead of rushed.

**Remaining blockers:** none for starting ticket 01. The only open question is the default already applied in ticket 01 (append the "Update" paragraph to ADR 0001).
