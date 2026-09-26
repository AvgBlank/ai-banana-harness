# Rust crate choices

Type: research
Status: resolved
Blocked by:

## Question

Which Rust crates should the port build on, judged on maturity, maintenance, compile-time weight and fit? Cover:

- **TUI:** current `ratatui` + `crossterm` versions, and widgets for multi-line input, markdown rendering, syntax highlighting, diffs and scrollable views.
- **MCP:** the official Rust SDK (`rmcp`). Does it support stdio, streamable HTTP, SSE clients and OAuth?
- **LLM streaming:** a hand-rolled `reqwest` + SSE client, or `async-openai` pointed at DeepSeek/Qwen?
- **SQLite:** `rusqlite` or `sqlx`.
- **Config and paths:** config formats (`toml`/`serde`) and directory crates.
- **Code search:** a glob/grep implementation, e.g. the `ignore` / `grep-*` crates from ripgrep.
- **Budget:** a rough clean-build time for the whole set.

## Research

Findings: branch `research/rust-crates`, file `docs/research/rust-crates.md` (commit 7e6e780f).

## Answer

Recommended stack. The reasoning and alternatives are in `docs/research/rust-crates.md` on the `research/rust-crates` branch.

| Need | Crate |
|---|---|
| Async runtime | tokio 1.53.1 |
| TUI | ratatui 0.30.2, crossterm 0.29 (`event-stream`) |
| Multi-line input | ratatui-textarea 0.9.2 |
| Syntax highlighting | syntect 5.3 + two-face |
| Diffs | similar 3.2 |
| Markdown | tui-markdown 0.3.10 (self-described experimental; fine for the week) |
| MCP | rmcp 3.4.1, default features off, **pinned exactly**. Supports stdio, streamable HTTP and OAuth 2.1; dropped the legacy HTTP+SSE transport in 0.11.0. |
| LLM client | Hand-rolled on reqwest 0.13.5 + sse-stream 0.2 (already a dependency of rmcp). async-openai's types drop `reasoning_content`, which DeepSeek requires. |
| SQLite | rusqlite 0.40.2 (bundled), not sqlx 0.9: 27 dependency crates against 155 |
| Config | toml 1.1.6 |
| Directories | etcetera 0.11 |
| Search | ripgrep's `ignore` + `grep-*` crates |

**Build cost:** a clean build took 59 s at full parallelism and 83 s at 4 jobs on an M3 Pro. Using the `ring` TLS provider instead of aws-lc brings that to 51 s / 68 s. Expect 2–4 minutes on a 4-core Linux machine. `make setup` needs only a C compiler plus Rust ≥1.95.

**Caveats:**
- The timings come from one Mac that other jobs were loading.
- Linux build time wasn't measured.
- `sse-stream` has very little documentation.
- rmcp ships breaking-level releases often.
