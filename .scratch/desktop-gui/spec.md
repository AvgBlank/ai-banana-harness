# Spec: Banana Harness desktop app (Electron GUI), implementation plan

Status: **planning complete; implementation has started.** Tickets 01 (the decision and vocabulary documents), 02 (session serde and the single title writer), 03 (core serde, provider options, rename, config paths, redactor) 04 (transcript replay), 05 (changes and diffs), 06 (durable undo marker), 07 (testkit move and `NO_PROXY` fix) 08 (testkit control API, `TestEnv`, scenarios, `fake-provider`) 09 (bridge transport, `serve --stdio`, `session.list`) 10 (bridge workspaces, sessions, providers, changes) 11 (bridge turns, approvals, undo) and 12 (shared scenarios at the bridge and process seams) are done. Tickets 13 to 27 are not started. This file is the final output of the wayfinder map in [`map.md`](map.md).

## 1. Goal

A polished macOS desktop app for Banana Harness, built as a thin Electron client over the Rust core and approved visually in the [prototype](prototype/): a window with a workspace and session sidebar, a live transcript with tool cards, a Changes panel with real diffs, approval dialogs, a provider picker, workspace trust, themes, and an unsigned DMG and ZIP. It reaches the Rust core through a new sidecar protocol, and it is proven with offline fake-provider tests at every layer.

The decisions are in the resolved tickets in [`issues/`](issues/): [scope and stack](issues/00-scope-and-stack-decisions.md), [core gaps](research/01-core-gaps.md) and [core additions](issues/10-core-api-additions.md), [protocol](issues/03-sidecar-placement-and-protocol.md), [Electron design and security](issues/06-electron-process-and-security.md), [renderer state and Changes panel](issues/07-renderer-state-and-changes-panel.md), [fake provider and test seams](issues/05-fake-provider-for-smoke-tests.md), [layout, Makefile, CI and packaging](issues/08-repo-layout-make-ci-packaging.md), [design system](issues/04-banana-design-system.md), [toolchain](issues/02-toolchain-versions.md) and [ADR and vocabulary](issues/09-adr-and-vocabulary.md). This spec does not reopen any of them.

## 2. Non-goals (excluded from V1 by decision)

Voice input and microphone controls; marketing or update badges; Monaco (the lightweight diff view is kept); the `motion` library; a Keychain option (the key stays in the environment or in memory); Windows packaging and Linux packages; universal or Intel macOS builds; signing, notarization and auto-update; an HTTP or WebSocket server and native Node bindings; session delete and pagination; the context-usage bar and token counters; an `mcp.status` method and MCP OAuth presentation beyond the confirmed external link; changing the agent's context after undo (V1 shows a hint); global keyboard shortcuts beyond Enter, Shift+Enter and Esc; the other items still marked "not yet specified" on the map.

## 3. Architecture in one paragraph

Three surfaces (TUI, desktop app, headless run) drive one in-process Rust engine. The desktop app's Electron main process launches the shipped binary as `banana-harness serve --stdio`, the sidecar, and speaks newline-delimited JSON over its stdio (protocol version 1, frames `ready`, `request`, `response`, `event`, `fatal`). One `Harness` serves one open workspace, one turn runs per workspace, and the Rust database is the durable truth. Main owns the process, key, trust, preferences, dialogs and a discardable live-tail buffer; the preload exposes one frozen allowlisted `window.banana`; the renderer is views plus a pure reducer that deduplicates by event id. Workspaces open in restricted mode and load their own config only after the user trusts that exact file. Tests reach a model only through `base_url` in layered config, with isolated `XDG_*` paths, via the dev-only `harness-testkit`.

## 4. Rules every implementation ticket follows

1. One ticket per agent session; a ticket flagged **L** names its split point and is split rather than overrun.
2. Definition of done: `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint` and `make gui-test`, and `make gui-e2e` where the ticket lists it; the ticket's checkboxes and status are updated.
3. The repository rules in `AGENTS.md` apply: one canonical owner per definition, code made unused is deleted in the same change, no forwarding shims, errors with context, no `unwrap` outside tests, comments explain intent, tests live in `tests/` and drive the system from outside.
4. **No test hook in production code.** Test infrastructure lives in `harness-testkit` and `gui/tests`; production reaches a model only through config.
5. The API key and the test key never appear in code, docs, fixtures, logs, artifacts or commits.
6. Vocabulary follows `CONTEXT.md` (the drafted version once ticket 01 lands). The interface says "session", never "chat" or "thread".
7. Design is the approved prototype; a visual change needs a concrete bug or an accessibility failure. The tokens are frozen.

## 5. Milestones and verification strategy

Each milestone ends with a check that does not depend on the next one, and every check that can run offline does, against the fake provider. Layers are proven from the inside out, so a failure points at the layer that introduced it.

| Milestone | Tickets | What exists afterwards | Verified by |
|---|---|---|---|
| **M0 Docs** | 01 | ADR 0003, vocabulary, the vocabulary rule | `diff` against the drafts; `make lint && make test` unchanged |
| **M1 Core additions** | 02 to 06 | serde, provider options with authority and scheme, rename, config paths, redactor, transcript replay, real diffs with flags, durable undo marker; the TUI unchanged in behaviour | core seam-1 tests; `make test` offline |
| **M2 Test foundation** | 07, 08 | `harness-testkit`, the stub with gates and failures, `TestEnv`, scenarios, `fake-provider`, `NO_PROXY` in `make test` | testkit tests; the isolation test (real binary against the stub); `make test` behind a poisoned proxy |
| **M3 Sidecar** | 09 to 12 | `harness-bridge`, `serve --stdio`, every method and event, golden frames | in-process scenarios (seam 3) and the real-process test (seam 2); a hand-driven `serve --stdio` session |
| **M4 GUI foundation** | 13 to 16 | `gui/` toolchain, supervisor, hardened window and IPC, the e2e harness, the ported shell connected to the real session list | `make gui-lint`, `gui-test`, hardening e2e; toolchain assumptions closed |
| **M5 First session** | 17 to 19 | open a workspace, find the key, send a prompt, stream the answer (the tracer bullet), CI protecting it | `plain-answer` and credentials e2e; the `gui` CI job green on Linux and macOS |
| **M6 Full V1 behaviour** | 20 to 25 | tool cards, subagents, approvals, provider picker, Changes, undo, trust, resilience, settings, accessibility | scenario e2e per behaviour; replay-equivalence tests; the user's visual parity review |
| **M7 Ship** | 26, 27 | unsigned DMG and ZIP, packaged smoke test, clean-checkout acceptance, documentation conformance | `make gui-package`, `make gui-package-smoke`, ticket 08's acceptance sets A and B |

**Test seams.** Seam 1: `harness-core` against the fake provider. Seam 2: the real `serve --stdio` process. Seam 3: `harness-bridge`'s `serve` in memory. Seam 4: the Electron app through Playwright. The TUI's seam (`App` with `TestBackend`) is unchanged. One set of scenario files feeds the bridge, process and Electron seams; the model-facing request log and the final workspace files are asserted identically in all of them, which proves the UI path sends the model the same conversation as the Rust path.

## 6. Implementation plan

The tickets are in [`build/`](build/). Each one has its scope, files, acceptance criteria and verification commands. The order below is a valid dependency order (every blocker has a lower number).

| # | Lane | Milestone | Ticket | Blocked by | Size |
|---|---|---|---|---|---|
| [01](build/01-apply-decision-docs.md) | D | M0 Docs | Apply the decision and vocabulary documents | none | S |
| [02](build/02-store-titles-and-session-serde.md) | R | M1 Core additions | Session serde and one canonical title writer | 01 | S |
| [03](build/03-core-small-apis.md) | R | M1 Core additions | Core serde, provider options, rename, config paths, redactor | 02 | M |
| [04](build/04-core-transcript-replay.md) | R | M1 Core additions | Transcript replay | 01 | M |
| [05](build/05-core-changes-and-diffs.md) | R | M1 Core additions | Workspace and turn changes with real patches | 01 | M |
| [06](build/06-core-undo-marker.md) | R | M1 Core additions | Durable undo marker | 04 | S |
| [07](build/07-testkit-move-and-test-env-fix.md) | K | M2 Test foundation | harness-testkit: move the fake provider; NO_PROXY in make test | 01 | M |
| [08](build/08-testkit-stub-env-scenarios.md) | K | M2 Test foundation | harness-testkit: control API, TestEnv, scenarios, fake-provider binary | 07 | L |
| [09](build/09-bridge-skeleton-and-serve.md) | B | M3 Sidecar | harness-bridge: framing, lifecycle, `serve --stdio`, session list | 03, 08 | L |
| [10](build/10-bridge-workspaces-sessions-changes.md) | B | M3 Sidecar | harness-bridge: workspaces, sessions, providers, changes | 09, 04, 05 | M |
| [11](build/11-bridge-turns-approvals-undo.md) | B | M3 Sidecar | harness-bridge: turns, events, approvals, undo | 10, 06 | L |
| [12](build/12-bridge-scenarios-and-process-test.md) | B | M3 Sidecar | Shared scenarios at the bridge and process seams | 11, 08 | M |
| [13](build/13-gui-scaffold-and-toolchain.md) | G | M4 GUI foundation | gui/ scaffold, toolchain and make targets | 01, 07 | L |
| [14](build/14-shared-schemas-and-sidecar-supervisor.md) | G | M4 GUI foundation | Shared protocol schemas and the sidecar supervisor | 13, 09 | L |
| [15](build/15-window-security-ipc-and-e2e-harness.md) | G | M4 GUI foundation | Window hardening, preload, IPC validation and the e2e harness | 14, 08 | L |
| [16](build/16-renderer-shell-and-sessions.md) | G | M4 GUI foundation | Renderer shell, theme and the session list | 15 | M |
| [17](build/17-workspace-open-key-and-preferences.md) | G | M5 First session | Open a workspace, find the key, remember preferences | 16, 10 | M |
| [18](build/18-first-turn-tracer-bullet.md) | G | M5 First session | First turn: composer to streamed answer (tracer bullet) | 17, 11 | L |
| [19](build/19-ci-gui-job.md) | C | M5 First session | CI: the gui job on Linux and macOS | 18 | S |
| [20](build/20-tool-activity-and-subagents.md) | G | M6 Full v1 behaviour | Tool cards, subagent groups and turn summaries | 18 | M |
| [21](build/21-approvals-questions-provider-picker.md) | G | M6 Full v1 behaviour | Approvals, questions and the provider picker | 20 | M |
| [22](build/22-changes-panel-and-undo.md) | G | M6 Full v1 behaviour | Changes panel, summary actions and undo | 18, 11 | L |
| [23](build/23-workspace-trust-and-restricted-mode.md) | G | M6 Full v1 behaviour | Workspace trust and restricted mode | 17, 10, 08 | L |
| [24](build/24-resilience-reload-crash-quit-links.md) | G | M6 Full v1 behaviour | Resilience: reload, crash, quit and link confirmation | 21 | L |
| [25](build/25-settings-theme-a11y-and-visual-parity.md) | G | M6 Full v1 behaviour | Settings, theme, accessibility and the visual parity review | 20, 21, 22, 23, 24 | M |
| [26](build/26-package-macos-and-package-job.md) | P | M7 Ship | macOS packaging and the package job | 24 | M |
| [27](build/27-final-acceptance-and-docs-conformance.md) | F | M7 Ship | Clean-checkout acceptance and documentation conformance | 19, 25, 26 | M |

Lanes: **D** documentation, **R** Rust core, **K** test kit, **B** bridge, **G** GUI, **C** CI, **P** packaging, **F** final acceptance.

**Parallelism.** After 01, three lanes are independent: the core lane (02 then 03, 04, 05 then 06), the test-kit lane (07 then 08), and the GUI scaffold (13, which needs 07 only for the shared Makefile). The bridge (09 to 12) joins the core and test-kit lanes. The GUI proper starts at 14 once the first bridge ticket has produced golden frames, and then follows the bridge ticket by ticket, because each GUI ticket depends on the bridge methods it uses. Tickets 20, 22 and 23 have no dependency on each other beyond their listed blockers and can run in parallel; 21 follows 20.

**Critical path:** 01 to 07 to 08 to 09 to 10 to 11 to 12 (sidecar proven), with the GUI chain 13 to 14 to 15 to 16 to 17 to 18 to 20 to 21 to 24 to 26 to 27 running beside it from 13 onward.

**Why a tracer bullet at 18.** Ticket 18 is the first moment the whole stack runs: Electron, preload, main, the real sidecar, the real core and the fake provider. Everything before it builds one layer; everything after widens a working slice. CI (19) follows it immediately so the later tickets are protected.

## 7. Documentation-application plan

**Status today:** ticket 01 has applied ADR 0003, the ADR 0001 update, `CONTEXT.md` and the `AGENTS.md` vocabulary line. **Every other draft hunk is still unapplied**: the rest of `AGENTS.md`, `gui/AGENTS.md`, `README.md` and `CONTRIBUTING.md` are as they were, and `gui/`, `harness-bridge` and `harness-testkit` do not exist yet.

**Ticket 01 (done) applied** the decision and vocabulary documents (ADR 0003, the ADR 0001 update, `CONTEXT.md`, the `AGENTS.md` vocabulary line). Every other ticket was blocked on it, because tickets introduce domain terms (`CONTEXT.md` must change in the same change as a new term, by the repository's own rule) and cite ADR 0003 as accepted.

**Staged application.** The remaining draft text describes crates, commands and behaviour that do not exist yet. To keep the rules and the code from disagreeing on `main`, each hunk lands with the ticket that makes it true:

| Draft text | Lands in | Why then |
|---|---|---|
| ADR 0003 file; ADR 0001 "Update" paragraph; `CONTEXT.md` (whole draft); `AGENTS.md` vocabulary line | 01 | decisions and vocabulary hold before code; terms are used from ticket 02 |
| `AGENTS.md` testkit crate-boundary sentence, "no test hooks" bullet, fake-provider one-owner example, seam-1 wording; `CONTRIBUTING.md` testkit layout line | 07 | the crate and its rule arrive together |
| `AGENTS.md` bridge boundary and the binary's dependencies, stdout bullet, seam-3 wording; `CONTRIBUTING.md` bridge layout line and `serve` mention | 09 | `harness-bridge` and `serve` arrive |
| `gui/AGENTS.md`, `gui/CLAUDE.md`; `AGENTS.md` lines 1 (the `gui/` clause and the targets that exist), 4 (name exception), 8 (JavaScript pinning), 10 (tokens example); `CONTRIBUTING.md` setup, `gui/` layout line and the pull-request bullet; `README.md` Development test and lint lines | 13 | the `gui/` project arrives |
| `AGENTS.md` `gui-e2e` clause and seam-4 wording; `CONTRIBUTING.md` test sentence and `gui-e2e` bullet; `README.md` `make gui-e2e` line | 15 | `make gui-e2e` exists |
| `AGENTS.md` key sentence and the workspace-trust rule | 17 | the key resolver exists and every workspace opens restricted |
| `AGENTS.md` Settings-shortcuts clause | 25 | Settings exists |
| `README.md` opening paragraph and the "Desktop app" section | 26 | the commands and the package are real |
| Conformance check: the repository files equal the drafts | 27 | nothing was missed or altered |

Rationale for staging instead of applying everything in 01: applying forward-looking rules about crates and commands that do not exist would leave `AGENTS.md` and the repository in disagreement for weeks, which the drafts' own acceptance criterion (ticket 09) forbids. The one open question on the documents, whether to append the "Update" paragraph to ADR 0001 (default: yes), is carried into ticket 01.

## 8. Assumptions recorded in earlier tickets and where each is closed

| Assumption (ticket 08 and others) | Closed by |
|---|---|
| `engine-strict` rejects Node 25; Biome's Tailwind CSS option; Vitest 5 and jsdom 30 on Node 24; `tsc` under TypeScript 7 with `paths` only | 13 |
| `default-members` keeps `make setup` free of the testkit | 07 |
| `NO_PROXY` behind a proxy; the testkit isolation; XDG isolation on Linux | 07, 08 (and Linux in CI, 19) |
| Linux Electron packages, `xvfb-run`, `--no-sandbox` under CI, action versions | 19 |
| Gatekeeper behaviour for a downloaded unsigned app; release sidecar size; the app icon; macOS runner packaging | 26 |
| URL authority and scheme semantics for `host` | 03 |
| Real-sidecar behaviour under Playwright and Electron's environment allowlist | 15, 18 |
| Packaged app has no `node_modules`; sidecar mode 755 in the bundle | 26 |

## 9. Open items

- **One question, default applied:** append the "Update" paragraph to ADR 0001 (ticket 01). If the user declines, ADR 0003's first line carries the supersession.
- **Not blocking any ticket:** keyboard shortcuts beyond the three keys, MCP status and logins, a core change so undo informs the agent, interrupted-turn cleanup, a slow-consumer timeout, hardening the `env` variable expansion in MCP server config, universal builds, Linux packages, signing and auto-update. These stay on the map as not yet specified.
- No decision is open that blocks starting ticket 01.

## 10. How to run this plan

Take the lowest-numbered open ticket whose blockers are done; one ticket per session; update its checkboxes and status when its acceptance criteria hold; keep `map.md` untouched except to record that the plan has started. The drafts remain the intended specification of the documents until ticket 27 confirms them.
