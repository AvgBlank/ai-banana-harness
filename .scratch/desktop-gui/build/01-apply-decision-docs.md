# 01: Apply the decision and vocabulary documents

**Lane / milestone:** D · M0 Docs   **Size:** S (one agent session)

**What to build:** Apply the Ticket 09 drafts that record decisions and vocabulary, before any code relies on them. Later tickets cite ADR 0003 and the new `CONTEXT.md` terms as decided; until this ticket lands they exist only as drafts in `.scratch/desktop-gui/drafts/`. The rest of the drafts (rules about crates, commands and behaviour that do not exist yet) are staged and applied by the ticket that makes each statement true (spec section 7).

**Blocked by:** none

**Status:** done 2026-10-17. All six acceptance criteria verified. `make lint` and `make test` were not run in this session (the work was done under a no-build constraint; the change touches only Markdown, so it cannot affect them); CI runs them when this is pushed.

**Files and components likely to change:**
- `docs/adr/0003-desktop-app-surface.md` (new, from the draft)
- `docs/adr/0001-rust-local-first-fork.md` (one appended paragraph)
- `CONTEXT.md` (replaced by the draft)
- `AGENTS.md` (the vocabulary line only)

**Acceptance criteria:**
- [x] `docs/adr/0003-desktop-app-surface.md` is byte-identical to `drafts/0003-desktop-app-surface.md`.
- [x] `CONTEXT.md` equals `drafts/CONTEXT.md`.
- [x] ADR 0001 differs from today only by the appended "Update" paragraph from `drafts/docs-edits.md` (applies the default "yes" to the open question in ticket 09; if the user declines, skip it and ADR 0003's first line carries the supersession alone). ADR 0002 is unchanged.
- [x] `AGENTS.md` differs from today in exactly one line: the domain-vocabulary list, as in `drafts/AGENTS.md`.
- [x] Every term defined in `CONTEXT.md` appears in `AGENTS.md`'s vocabulary list and vice versa.
- [x] No other file changes; nothing else from the drafts is applied yet.

**Verification** (commands to run; none has been run by the planning):

```
cmp docs/adr/0003-desktop-app-surface.md .scratch/desktop-gui/drafts/0003-desktop-app-surface.md
diff CONTEXT.md .scratch/desktop-gui/drafts/CONTEXT.md   # no output
git diff -U0 docs/adr/0001-rust-local-first-fork.md   # only the appended paragraph
git diff --stat docs/adr/0002-in-turn-waiting.md      # no output
git diff -U0 AGENTS.md                                # exactly the vocabulary line
make lint
make test
```

**Documentation:** Applies: ADR 0003, ADR 0001 update, `CONTEXT.md`, the `AGENTS.md` vocabulary line.

**Notes:** No code. This is the only ticket that may be done before the others; every other ticket is blocked on it.

**Definition of done (every ticket):** `make lint` and `make test` pass on Linux and macOS (CI); for `gui/` also `make gui-lint`, `make gui-test` and, where listed, `make gui-e2e`; no unused code is left behind; comments follow the repository's rules; no test hook enters production code; this file's checkboxes and status are updated.

**Completion record (2026-10-17):**
- Applied: `docs/adr/0003-desktop-app-surface.md` (new; `cmp` identical to the draft), one appended "Update" paragraph in `docs/adr/0001-rust-local-first-fork.md` (the original text is an exact prefix of the new file; +2 lines), `CONTEXT.md` (identical to the draft), and line 14 of `AGENTS.md` (the vocabulary list; 1 line changed).
- Checks run: the ticket's `cmp`, `diff` and `git diff` commands; ADR 0002 unchanged; vocabulary parity (20 terms defined, 20 in the list, none missing either way); the staged `AGENTS.md` rules (bridge, testkit, `gui/`, test hooks, trust, stdout, pnpm) confirmed absent; README, CONTRIBUTING, Makefile and Cargo.toml hash-identical to before; the user's `.gitignore` edit byte-identical to before.
- Not run: `make lint` and `make test` (they would build the product and create `target/`).
- Deviations from the drafts: none. The ADR 0001 paragraph was applied with the default "yes" from ticket 09's open question; the user has not declined it.
- Not applied (belongs to later tickets, per spec section 7): every other draft hunk, including the rest of `AGENTS.md`, `gui/AGENTS.md`, README and CONTRIBUTING.

