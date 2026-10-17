# Banana Harness: visual prototype (wayfinder ticket 04)

Mock data only. Nothing here talks to the Rust sidecar, Electron, or the file system. It exists so the
design can be reviewed before tokens are frozen. Not production code: the real app goes in `gui/`.

## Run it

```
cd .scratch/desktop-gui/prototype
pnpm install        # once
pnpm dev            # http://localhost:5199
```

Or open the single self-contained build with no server:

```
pnpm build          # typechecks, then writes dist/index.html (one file, ~450 kB)
open dist/index.html
```

## Review it

A small **Prototype controls** dock sits bottom-left. It jumps to each state (19 of them, numbered to match
ticket 04) and switches System / Dark / Light. It is the only non-product UI.

`pnpm shots [WIDTHxHEIGHT]` regenerates `screenshots/<size>/<theme>-<state>.png` from `dist/index.html`
(default 1440x900; 1100x720 is the minimum window). It uses the installed Google Chrome; set
`BH_CHANNEL=` to use Playwright's own browser instead.

## Where things are

- `src/styles/tokens.css` is the single source of design tokens (dark, light, system).
- `src/styles/index.css` maps tokens to Tailwind 4 utilities (`@theme inline`).
- `src/components/ui/` are shadcn-style primitives (Radix + cva), written by hand and restyled to the tokens.
- `src/mock/data.ts` holds every piece of example content (workspaces, sessions, diffs, trust config).
- `src/components/review/ReviewDock.tsx` lists the states.

## Deliberately not here

Monaco (ticket 07 decides Monaco vs a lightweight diff view), motion/animation library (CSS keyframes only),
voice input, update or marketing badges, and any real IPC.
