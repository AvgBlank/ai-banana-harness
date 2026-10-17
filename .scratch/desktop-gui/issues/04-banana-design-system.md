# Banana design system and screens

Type: prototype
Status: resolved
Blocked by:

## Question

What do the v1 screens look like? Produce a rough but concrete artifact (static HTML, linked from this ticket) covering the Codex-inspired layout (icon rail, sidebar, home screen with composer, conversation view with tool cards, Changes panel, approval, model picker, settings/theme) in both themes, plus the token set (surfaces, text, borders, accents, diff tints, radii, spacing, type scale, focus ring) as CSS variables. React on this with the user before the tokens are frozen.

## Artifact

**Prototype status: approved by the user on 2026-10-17. Tokens are frozen** (see "Approval and token freeze" below). The prototype stays in the repo as the visual reference for the production GUI.

- **Path:** `.scratch/desktop-gui/prototype/` (a real Vite app: React 19, strict TypeScript, Tailwind 4, shadcn-style components on Radix, Lucide). Mock data only; no IPC, no sidecar, no `gui/`.
- **Run it:**
  ```
  cd .scratch/desktop-gui/prototype
  pnpm install && pnpm dev          # http://localhost:5199
  pnpm build && open dist/index.html  # one self-contained file, no server
  ```
- **Review without running anything:** 38 screenshots (19 states x dark and light) in `prototype/screenshots/1440x900/`, and the same at the minimum window in `prototype/screenshots/1100x720/`. File names are `<theme>-<state>.png`. `pnpm shots` regenerates them.
- **How to review in the app:** the *Prototype controls* dock (bottom-left) jumps to each state and switches System / Dark / Light. It is the only non-product UI. Theme also works from Settings. Working interactions: theme switching, sidebar collapse, Changes panel open/close/resize (drag or arrow keys), scope select, file selection, session selection and in-place rename (double-click or the pencil), streaming simulation with Stop, tool-card expand/collapse, provider popover, workspace popover, approval buttons, trust dialog (the "Trust and open" button stays disabled until the host checkbox is ticked), key entry with show/hide, crash banner Stop retrying / Restart, Forget trust in Settings.
- **Checked:** `tsc --noEmit` strict is clean, the build is clean, no console or page errors across all states in both themes, no visible "chat" or "thread" label in any of the 19 states (checked on the rendered text), and layout holds at 1100x720.

### Screens and states

| # | Requested | Dock state (screenshot name) | What it shows |
|---|---|---|---|
| 1 | Home + composer + workspace selection | `1-home-composer`, `1b-workspace-picker` | "What should we work on in <workspace>?", workspace bar above the composer with recents and Open folder, composer with attach, approval-mode chip, provider/model chip and send. No mic. |
| 2 | Conversation | `2-conversation-streaming`, `2b-conversation-finished`, `2c-interrupted-turn` | User bubble, compaction notice, streaming text with caret and Stop, tool cards (grep, read_file, write_file, edit with inline diff, blocked bash, running bash with output), subagent card, "Edited 6 files +161 -49 / Undo / View changes" summary, interrupted-turn notice (`state: null`). Composer is locked while a turn runs in the workspace. |
| 3 | Sidebar + icon rail | `3-sidebar-rail` (also visible in every state) | Rail (Home, Sessions, Changes, Settings), workspace groups with Restricted chip, session rows with relative time, running dot, interrupted marker, hover rename, "New session". |
| 4 | Changes panel | `4-changes-last-turn`, `4b-changes-binary-truncated`, `4c-changes-empty` | Scope select (Last turn / Workspace), totals, file list with counts, unified diff with line numbers, green/red tints and gutter bar, hunk header, binary row, truncated notice, empty state. Resizable. |
| 5 | Approval dialog | `5-approval-dialog` | Tool, command, cwd, matched rule, why asked, Deny (default focus) / Allow for this session / Allow once; the turn is shown paused behind it. |
| 6 | Provider/model selector | `6-provider-model` | DeepSeek and Qwen with model and the host the key goes to; one-model-per-provider note. |
| 7 | API-key onboarding | `7-api-key-onboarding` | `AI_API_KEY` explanation, Check again, masked memory-only entry; composer disabled. |
| 8 | Workspace trust dialog | `8-workspace-trust` | MCP commands (a red flag on `{env:AI_API_KEY}`), guardrail changes, provider host override with the required checkbox, other overrides, AGENTS.md footnote, View raw file, restricted-by-default buttons. |
| 9 | Restricted-mode indicator | `9-restricted-mode` | Chip in the title bar, the sidebar row and the workspace bar; the chip opens the trust dialog. |
| 10 | Sidecar crash banner | `10-sidecar-restarting`, `10b-sidecar-failed` | Banner under the title bar: attempts, "turn was interrupted", Details (redacted stderr tail), Stop retrying / Restart. |
| 11 | Settings | `11-settings` | System/Dark/Light preview cards (live), API-key status (value never shown), trusted workspaces with Forget, shortcuts, About. |
| extra | Quit and link confirmations (ticket 06) | `quit-while-running`, `external-link` | Quit while a turn runs; confirm before opening an `https:` link from agent output. |

### Design tokens (FROZEN; source: `prototype/src/styles/tokens.css`)

| Token | Dark | Light (banana cream) | Use |
|---|---|---|---|
| `--bg` | `#1a1914` | `#fbf6e4` | window and main area |
| `--surface-1` / `-2` / `-3` | `#211f19` / `#28251e` / `#302d25` | `#fffdf3` / `#f5eed5` / `#ede4c3` | sidebar and cards / composer and inputs / selected rows and chips |
| `--border` / `--border-strong` | `#3a362c` / `#4a4538` | `#e2d8b3` / `#cfc291` | hairlines / controls |
| `--text` / `-muted` / `-faint` | `#f2eedf` / `#a8a28f` / `#928b77` | `#2a2618` / `#6b6448` / `#736c53` | body / secondary / hints |
| `--accent` | `#ffd60a` | `#ffd60a` | primary action, rail selection, focus, caret |
| `--accent-hover` | `#ffe144` | `#f5c800` | |
| `--accent-fg` | `#1a1914` | `#2a2618` | text on yellow |
| `--accent-text` | `#ffd60a` | `#7d5f00` | yellow used as text; light uses a deep banana for contrast |
| `--accent-soft` / `--accent-line` | `rgba(255,214,10,.12)` / `.38` | `rgba(255,214,10,.26)` / `rgba(160,120,0,.45)` | selected backgrounds / focused borders |
| `--peel` / `-soft` / `-line` | `#b68a3f` / `#35291a` / `#5a4522` | `#845f1e` / `#f1e4bd` / `#d9c389` | banana-peel brown: Restricted chips, subagent mark |
| `--add-bg` / `-fg` / `-gutter` | `rgba(74,160,90,.13)` / `#86d493` / `#2f8a52` | `rgba(60,140,80,.11)` / `#1f6b3a` / `#3a9a5b` | added lines |
| `--del-bg` / `-fg` / `-gutter` | `rgba(200,80,70,.10)` / `#f1998f` / `#b5483f` | `rgba(190,70,60,.09)` / `#a53a30` / `#c9564b` | removed lines |
| `--hunk-bg` | `rgba(255,214,10,.07)` | `rgba(200,150,0,.10)` | hunk headers |
| `--ok` / `--warn` / `--danger` | `#86d493` / `#f2a93b` / `#ea6b61` | `#1f6b3a` / `#9a5f00` / `#b3382d` | status; each has a `-soft` tint |
| `--focus` | `#ffd60a` | `#ad8300` | 2 px ring, 2 px offset |
| `--shadow` | `0 18px 50px rgba(0,0,0,.5), 0 2px 8px rgba(0,0,0,.4)` | `0 18px 50px rgba(90,70,10,.2), 0 2px 8px rgba(90,70,10,.12)` | dialogs and popovers only |

Type, shape and layout: UI font `-apple-system, "SF Pro Text", Inter`; code font `"SF Mono", "JetBrains Mono", ui-monospace`; scale 12 / 13 / 14 / 16 / 20 / 32 px; radii 6 / 10 / 16 / pill; 4 px spacing base. Title bar 44 px, rail 56 px, sidebar 280 px, Changes panel 480 px by default (clamped to 40% of the window, resizable 360 to 760). Yellow is used only for the primary action, rail selection, focus, the caret and key highlights; there are no gradients. Theme: `data-theme` of `system | dark | light` on `<html>`, `system` follows `prefers-color-scheme`, kept in `localStorage` in the prototype only (production persists through Electron main, ticket 06).

### Mock versus real

Mock: all sessions, events, diffs, the trust config, hashes, the provider hosts, stderr, relative times, the streaming simulation, and the product name (one `APP_NAME` constant). Real in shape: the data mirrors the core types in tickets 03 and 10 (`Session`, `FileChange`, event kinds, `ProviderOption{provider, model, host}`, `state: null` turns). Not designed here: Monaco or any syntax highlighting, real Markdown rendering (backtick code spans only), keyboard shortcuts beyond Enter/Esc, native menu.

### What building it taught us (for other tickets)

- **Ticket 02/08, TypeScript 7:** `tsc` 7.0.2 rejects `compilerOptions.baseUrl` (error TS5102), which shadcn's setup docs still show. The `@` alias works with `"paths": { "@/*": ["./src/*"] }` and no `baseUrl`. The pnpm install, Vite 8, `@vitejs/plugin-react` 6, Tailwind 4.3 and Playwright 1.64 combination all built and ran here, on Node 25.9 (ticket 02 recommends 24).
- **Ticket 07, layout rule found at 1100x720:** with the sidebar and a 480 px Changes panel both open the conversation column fell to about 260 px and the composer wrapped. The prototype now hides the sidebar below 1280 px while the Changes panel is open, clamps the panel to 40% of the window, and collapses composer labels by the composer's own width (container queries). Ticket 07 should keep that rule.
- **Ticket 07, diffs:** the lightweight diff renderer (about 50 lines) handles line numbers, tints, hunks and a truncation notice at these sizes; whether Monaco is worth its weight is still ticket 07's call.
- **Dropped from the plan:** the `motion` library (CSS keyframes cover the dialog, banner and caret animations, and they respect reduced-motion). Add it back only if a real need appears.

### Decisions you made in this run (recorded)

- No Keychain option in v1: the key stays environment-based or memory-only (answers ticket 06 Question 1).
- GUI package metadata may define the window title, app-data folder and bundle ID, with the exception documented in ADR 0003 (answers ticket 06 Question 2; ticket 09 writes it).

### Questions that were put to the user (answered by approval; see below)

1. **Overall feel:** is the density right (sidebar 280 px, 14 px text, 44 px title bar), or closer to or looser than Codex?
2. **Banana mark:** the home screen uses a small crescent banana as the only illustration; keep, redraw, or drop it?
3. **Yellow on cream:** light theme text that needs to read as "banana" uses a deep gold (`#7d5f00`) rather than `#ffd60a`, which fails contrast on cream. Acceptable, or do you want a brighter or browner accent in light?
4. **Restricted mode shows in three places** (title bar, sidebar row, workspace bar). Keep all three, or only the workspace bar and sidebar?
5. **Rail contents:** Home, Sessions, Changes, Settings. Anything else for v1 (for example Search, or a Workspaces entry)?
6. **Changes panel default:** closed until "View changes" or the title-bar button is used (as now), or open automatically after a turn that changed files?
7. **Diff colour strength:** I lowered the tints after the first render (dark: green 13%, red 10%). Too quiet, too loud, or right?
8. **Approval buttons:** "Allow once" is the yellow primary and Deny is the default-focused safe choice. Is that hierarchy right for risky actions?
9. **Dock labels:** nothing in the product UI changes, but tell me if you want the Changes toggle (title bar plus rail) kept in both places.

### Approval and token freeze (2026-10-17)

The user approved the prototype and the overall visual direction without asking for changes: layout, spacing, typography, banana-yellow branding, restrained diff colours and responsive behaviour are preserved as built. The nine review questions were not answered one by one, so each stays as shipped in the prototype (the banana mark, three Restricted indicators, closed-by-default Changes panel, "Allow once" as the yellow primary with Deny focused, current diff tints).

**Freeze.** `prototype/src/styles/tokens.css` is marked FROZEN. The production GUI copies it unchanged; a value changes only for a concrete bug or an accessibility failure. `pnpm tokens:check` (`prototype/scripts/check-tokens.mjs`) enforces two things and was exercised both ways: the light values and the system-light media query stay identical, and the text, focus and chip pairs meet WCAG AA in both themes. Port the same script to `gui/` (ticket 08) so the freeze is enforced there too.

**The only values that changed, all accessibility, found by measuring contrast before freezing:**

| Token | Was | Now | Why |
|---|---|---|---|
| `--text-faint` dark | `#756f5e` (3.0 to 3.5:1) | `#928b77` (4.5 to 5.2:1) | timestamps, hints, paths and "No sessions" are real text and failed AA |
| `--text-faint` light | `#968e6e` (2.8 to 3.2:1) | `#736c53` (4.5 to 5.2:1) | same |
| `--peel` dark | `#a9803a` (3.9:1 on its tint) | `#b68a3f` (4.5:1) | the Restricted chip's 11 px label |
| `--peel` light | `#8b6420` (4.2:1 on its tint) | `#845f1e` (4.6:1) | same |
| `--focus` light | `#b58900` (2.97:1) | `#ad8300` (3.2:1) | focus ring is under the 3:1 minimum on cream |

One related code fix: the time label on the selected session row (on surface-3) now uses the muted colour, since faint on surface-3 is below AA there. Nothing else in layout, spacing, typography, branding or diff styling was touched.

**Known deviation, left as designed, flagged for the build:** `--border-strong` is 1.8:1 against the dark background and 1.65:1 against cream, below the 3:1 that WCAG 1.4.11 asks of a control boundary that is the only cue. Buttons and inputs also differ by fill and carry text, and this is the Codex-like subtle outline the user approved, so it is not changed. If an automated audit later flags text inputs or the unchecked checkbox, raise only those control borders. The yellow primary button on cream is 1.3:1 against the background but its label is 10.7:1 and it carries text, so it is acceptable.

**The prototype is kept.** Do not delete or replace `.scratch/desktop-gui/prototype/`. The production `gui/` takes its tokens, layout measurements, copy and component structure from it, and ticket 08 decides how the code is carried over.
