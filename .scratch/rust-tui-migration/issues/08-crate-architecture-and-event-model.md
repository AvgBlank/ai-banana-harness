# Crate architecture and event model

Type: grilling
Status: resolved
Blocked by: 02, 03

## Question

What are the Cargo workspace layout and module seams? For example, a core agent-loop crate, a provider crate, a tools/guardrails crate, an MCP crate, the TUI and the headless binary.

What turn event model flows from the core to both surfaces? Specifically:
- which TrueForge event types survive
- how events are delivered in-process (channels)
- how the TUI and headless run consume them

## Answer

The user decided this on 2026-09-27. The full detail is in [spec.md](../spec.md) §3–§4.

- **Layout:** a Cargo workspace in `rust/`, kept apart from the TS code.
  - Leaf crates, which don't depend on each other: `harness-llm`, `harness-tools`, `harness-mcp`, `harness-store`.
  - `harness-core` depends on all four.
  - `harness-tui` depends on core.
  - The `banana-harness` binary depends on core and tui.
  - Crate names are deliberately neutral so a product rename doesn't touch them.
- **Paused turns wait in place.** A thread waiting for an approval or a question answer blocks on a oneshot channel inside the live turn. There is no end-and-resume and no snapshot rebuild. See [ADR 0002](../../../docs/adr/0002-in-turn-waiting.md).
- **Surface API:** `Harness::start_turn(input) -> TurnHandle { events, reply(), cancel() }` is the only thing either surface uses.
- **Events:** one serde enum that keeps the TS event names. The minimal set comes from the core-loop inventory, plus a new `context.compacted`.
- **Persistence:** the turn runner is the only writer to SQLite, and it persists each event before forwarding it.
- **Built-ins** are called directly at fixed points in the loop. There's no capability or plugin trait.
- **Dispatch:** closed enums for tool sources (`Host | Mcp | Deferred | Builtin`) and providers (`DeepSeek | Qwen`).
- **Conventions:**
  - `anyhow`, plus typed errors only where the code branches on them
  - tokio multi-thread runtime
  - tests in `tests/`
  - no `unwrap()` outside tests
  - a Rust `AGENTS.md` at cutover
