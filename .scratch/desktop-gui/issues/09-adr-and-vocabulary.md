# ADR 0003 and CONTEXT.md wording

Type: grilling
Status: resolved
Blocked by:

## Question

Draft the exact ADR 0003 text (status, context, decision, supersession of ADR 0001's surface restriction, consequences incl. security and testing) and the `CONTEXT.md` edits (the new domain term for the desktop app, removal of the "avoid GUI" line, how "session" is reconciled with Codex-style UI wording, and whether the sidecar needs a term). Planning only: the actual edits happen in the build run.

Note (2026-10-17): ADR 0003 must record two user decisions. (1) The GUI package metadata (window title, app-data folder, bundle ID) is the one allowed copy of the product name outside `harness-core` and the binary crate. (2) There is no Keychain persistence option in v1; the API key stays environment-based or memory-only. See [ticket 06](06-electron-process-and-security.md).

Note (from ticket 07, 2026-10-17): the vocabulary pass should also decide the terms "interrupted turn" (a turn with no `turn.done` after a crash), "restricted mode" and "undone turn", and ADR 0003 should mention the new durable `turn.undone` event.

Note (from ticket 05, 2026-10-17): ADR 0003 should mention the test seam (stub through `base_url`, no production test hook) and the new test-support crate; `AGENTS.md`'s crate-boundary line gains `harness-testkit` (dev-dependency only). Decide whether the test-support crate needs a `CONTEXT.md` term.

Note (from ticket 08, 2026-10-17): ADR 0003 also records the GUI package metadata exception (`productName`, `appId`, window title live in `gui/package.json` and `gui/electron-builder.yml` only), the unsigned macOS-only v1 packaging with Windows and Linux packages out of scope, the new crates (`harness-bridge`, `harness-testkit`) and their boundaries, and Biome/TypeScript 7/Node 24 as the GUI toolchain. The `AGENTS.md` edits to make in the same change are listed in [ticket 08, section 1](08-repo-layout-make-ci-packaging.md#1-final-layout).

## Answer

Resolved 2026-10-17 against the live `CONTEXT.md`, `AGENTS.md`, ADR 0001 and ADR 0002, `README.md`, `CONTRIBUTING.md`, and every resolved ticket (00 to 08, 10). **The exact text is drafted and checked; nothing under `docs/`, `CONTEXT.md`, `AGENTS.md`, `README.md`, `CONTRIBUTING.md` or `gui/` was edited.** The build run applies the drafts.

### Drafts (all in `.scratch/desktop-gui/drafts/`, ready to copy)

| Draft | Becomes | How it was checked |
|---|---|---|
| [`0003-desktop-app-surface.md`](../drafts/0003-desktop-app-surface.md) | `docs/adr/0003-desktop-app-surface.md` | every item from the notes of tickets 05 to 08 is present (scripted grep, 19 of 19) |
| [`CONTEXT.md`](../drafts/CONTEXT.md) | replaces `CONTEXT.md` | `diff` against the live file shows only the intended edits: 6 replaced lines and added entries; every other line is identical |
| [`AGENTS.md`](../drafts/AGENTS.md) | replaces `AGENTS.md` | generated from the live file; lines 5, 6, 11, 12, 13, 16, 18 and 20 to 23 verified byte-for-byte unchanged; every glossary term appears in its vocabulary list and vice versa |
| [`gui-AGENTS.md`](../drafts/gui-AGENTS.md) | new `gui/AGENTS.md` (+ `gui/CLAUDE.md` containing exactly `@AGENTS.md`) | satisfies the existing rule that every nested `AGENTS.md` has a sibling `CLAUDE.md` |
| [`docs-edits.md`](../drafts/docs-edits.md) | exact edits to ADR 0001, `README.md` and `CONTRIBUTING.md` | placed against the real section structure |

### Decisions

**1. ADR 0003 is "Desktop app as a third surface, over a Rust sidecar".** It states the status (accepted; partly supersedes ADR 0001; ADR 0002 stands) and records what is hard to reverse: the third surface and the thin-client rule; the sidecar boundary (newline-delimited JSON over stdio, no HTTP, WebSocket or native binding, `harness-bridge` depending on core only); one harness per open workspace and one running turn per workspace; the durable `turn.undone` event with no schema migration; the key rule (environment, login shell, session memory, no Keychain in v1); the security model (sandboxed renderer, restricted mode and content-hash-bound workspace trust, provider authority and scheme checks with non-HTTPS allowed only for loopback); the test seam (`harness-testkit`, `base_url`, no production hook); unsigned macOS-only v1 packaging; and the one exception to the product-name rule, including the bundle identifier `io.github.avgblank.banana-harness` (the default offered with ticket 08, which you did not change). It lists what it leaves undecided. It is about 920 words, much longer than ADRs 0001 and 0002 (about 100 each), because it settles many linked decisions at once; each bullet is one sentence of the decision, not a discussion.
- **Kept out of the ADR on purpose:** the GUI toolchain (Node 24, pnpm, Biome, TypeScript 7). Ticket 08's note asked for it there, but an ADR is for hard-to-reverse decisions and the toolchain is easy to change, so it lives in `gui/AGENTS.md`, and the ADR points to that file.

**2. How the supersession is documented without erasing history.** ADR 0001 keeps all of its text and gains one appended paragraph ("Update: the 'only surfaces' part is superseded by ADR 0003 ... The rest of this decision stands."). ADR 0002 is not edited. ADR 0003's first line says what it supersedes.

**3. `CONTEXT.md` vocabulary.**
- **Added (9 terms):** **Desktop app**, **Sidecar**, **Interrupted turn**, **Undone turn**, **Workspace config**, **Restricted mode**, **Workspace trust**, **Changes**; the existing **Session**, **Provider** and **Workspace** entries gain one clause each.
  - *Desktop app* (avoid: web UI, chat UI, Electron app) is the new term; the TUI's "avoid GUI" line and its "only interactive interface" claim are removed, as you decided. "GUI" is not banned and not defined: the term for the product is *desktop app*, and `gui/` is just the directory.
  - *Sidecar* (avoid: server, backend, daemon) is the process, so "server" is rejected on purpose: nothing listens on a network.
  - *Interrupted turn* (avoid: failed turn, crashed turn) is kept distinct from a turn that ended in `error`.
  - *Undone turn* (avoid: reverted, rolled-back turn).
  - *Workspace trust* is explicitly **not** a tool approval (avoid: approval, allowlist), which keeps it separate from the existing *required action*.
  - *Restricted mode* (avoid: safe mode, read-only mode) says the agent still works in the workspace, since it is not read-only.
- **"Session" and Codex wording:** unchanged avoid list (chat, thread); the entry now says the desktop app groups sessions by workspace and says "session" everywhere, so Codex's "New chat" is "New session" (your decision), and the agent-loop *Thread* keeps its meaning.
- **Provider:** "chosen at startup" becomes "chosen at startup and switchable per session". This is an accuracy fix: the TUI's `/provider` and the desktop app's picker already switch per session.
- **No term for the test-support crate** (ticket 05's open question): it is engineering infrastructure, not domain language; it is explained in `AGENTS.md` and the ADR.
- **Intro line** now says the harness is driven through "a TUI, a desktop app, or headlessly".

**4. `AGENTS.md` edits** (the full diff is the draft; the changes are listed here): the workspace line names `gui/` and the `make gui-*` targets; CI and the `make gui-e2e` requirement; crate boundaries (`harness-tui` and `harness-bridge` depend on core only, the binary on core, tui and bridge, `harness-testkit` dev-only); the product-name rule and its metadata exception; the test seams (now four: core, TUI, sidecar protocol, Electron; GUI tests in `gui/tests/`, fake provider owned by `harness-testkit`); **new rules** that production code has no test hooks, that the desktop app never loads a workspace config until the user trusts that exact file, and that `harness-bridge` writes only protocol frames to stdout; the pinning rule for JavaScript; the key rule; the one-owner examples (fake provider, design tokens); the vocabulary list; and the Settings shortcuts list in the user-visible-behaviour rule. Every untouched line is byte-identical.

**5. `gui/AGENTS.md`** (11 rules) carries what is specific to the desktop app: toolchain and pins, no `baseUrl`, Biome, the main, preload and renderer boundaries, the security invariants, the trust and provider-check rules, the frozen design tokens and the prototype as the visual reference, wording, the reducer rules, the tests, and the metadata exception.

**6. `README.md` and `CONTRIBUTING.md`:** a "Desktop app" section (setup commands, how the key is found and never stored, restricted mode and trust, the unsigned-package note), the opening paragraph, the development commands, and the layout list gaining `harness-bridge`, `harness-testkit` and `gui/`. `docs/system_architecture.md` and `docs/easy_architecture_guide.md` are untracked and are left alone.

### Build-run steps and acceptance criteria

1. Copy the five drafts as described in `docs-edits.md`; create `gui/CLAUDE.md` with exactly `@AGENTS.md`.
2. **Acceptance:**
   - `git diff` for `CONTEXT.md` and `AGENTS.md` equals the drafts' diffs; ADR 0001 differs from the original only by the appended paragraph; ADR 0002 is unchanged.
   - Every term in `CONTEXT.md` is in `AGENTS.md`'s vocabulary list and vice versa (the scripted check can become a one-line test or CI step).
   - `grep -rniE "only interactive|avoid.*GUI"` over tracked docs returns nothing.
   - `README.md` and `/help` stay in step; the desktop app's keyboard shortcuts are added to the README's key-bindings section when the build fixes them (the map lists them as not yet specified).
   - The ADR and `AGENTS.md` changes ship in the same commit as the first code that adds `harness-bridge`, so the rules and the crates never disagree on `main`.

### Remaining questions (none block the build)

- **Appending the "Update" paragraph to ADR 0001** is the only edit to an existing ADR. Default: yes. If you would rather leave ADR 0001 completely untouched, the supersession then lives only in ADR 0003's first line.
- The ADR is long for this repository's ADRs. Default: keep it as one document so the linked decisions stay together; say if you want it split (for example, a separate ADR for workspace trust).
