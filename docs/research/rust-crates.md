# Rust crate choices for Banana Harness

Research for `.scratch/rust-tui-migration/issues/02-rust-crate-choices.md`. As of 2026-09-27.

Versions, publish dates, MSRVs and feature lists come from the crates.io API (`/api/v1/crates/<name>` and `/<version>/dependencies`). Repo activity comes from the GitHub API. Build times were measured locally (see [Build budget](#build-budget)).

## Recommendation table

| Need | Crate@version | Why | Alternatives |
| --- | --- | --- | --- |
| Async runtime | `tokio@1.53.1` | `rmcp` and `reqwest` both require it, so there is no real choice. Enable `rt-multi-thread, macros, process, fs, io-util, io-std, signal, time, sync`. | `smol@2.0.2` has had no release since 2024-09 and does not fit rmcp. |
| TUI framework | `ratatui@0.30.2` (default features) | Standard choice, released 2026-06-19, MSRV 1.88. 0.30 splits into `ratatui-core`/`ratatui-widgets`/`ratatui-crossterm`. Has `ratatui::run`/`init`, `Viewport::Inline` + `insert_before` (chat scrollback that stays in the terminal), `Paragraph::scroll`, `Scrollbar`, `List`/`ListState`. | None worth considering. |
| Terminal backend | `crossterm@0.29.0`, features `event-stream` (plus the default `bracketed-paste`) | Default backend. `ratatui-crossterm@0.1.2` defaults to `crossterm_0_29`. `event-stream` gives an async `EventStream` to use in `tokio::select!`. | `termion` (Unix only), `termwiz`. |
| Multi-line input | `ratatui-textarea@0.9.2` | Fork of `tui-textarea` now maintained under the ratatui org (the upstream `tui-textarea@0.7.0` has had no release since 2024-10). Gives undo/redo, soft wrap, selection, search, Emacs keys, and depends on `ratatui-core` 0.1. | `tui-input@0.15.5` (single line only), `edtui@0.11.7` (vim-style, heavier). |
| Markdown rendering | Hackathon: `tui-markdown@0.3.10`. Later: your own `pulldown-cmark@0.13.4` → `ratatui::text::Text` renderer. | `tui-markdown` returns `Text` from `from_str` and highlights fenced code through syntect. Its README calls it an "experimental Proof of Concept", and it pulls syntect's default (oniguruma) backend. Codex's TUI parses Markdown with pulldown-cmark itself, which is the long-term shape. | `termimad` (not evaluated). |
| Syntax highlighting | `syntect@5.3.0` + `two-face@0.5.2+bat-0.26.1` | Uses TextMate grammars and themes. `two-face` adds bat's extra syntaxes (TOML, TypeScript, …). Codex uses exactly this pair. Pick one regex backend for all three crates. `onig` is what tui-markdown forces and cost only about 4 s CPU in our build. `fancy` is pure Rust. | `tree-sitter-highlight@0.27.0` needs a C-compiled grammar crate per language, which is too heavy for a hackathon. |
| Diffs (compute) | `similar@3.2.0` | Released 2026-08-17. Line/word/char diffs, `unified_diff()`. Render hunks yourself as styled `Line`s. | `imara-diff@0.2.0` (faster, lower-level), `diffy@0.5.2` (patch apply; Codex uses it). |
| Scrollable views | Built-ins: `Paragraph::scroll` + `Scrollbar`, or `Viewport::Inline` | This covers transcript and diff panes. | `tui-scrollview@0.6.8` (ratatui/tui-widgets, released 2026-09-24) for scrolling arbitrary widgets. |
| MCP client | `rmcp@3.4.1`, `default-features = false`, features `client, transport-child-process, transport-streamable-http-client-reqwest, reqwest, auth` | The official SDK. It implements MCP `2026-07-28` and stays compatible with `2025-11-25`. stdio: yes (`TokioChildProcess`). Streamable HTTP client: yes. OAuth 2.1 client: yes (PKCE, RFC 9728/8414 discovery, dynamic registration, CIMD, refresh, a pluggable `CredentialStore`). Legacy HTTP+SSE transport: **no**, it was removed in 0.11.0 (2025-12-08). | None worth considering. |
| LLM HTTP | `reqwest@0.13.5`, features `json, stream` | Shared with rmcp. 0.13 defaults to rustls with the aws-lc provider and platform verifier, so no OpenSSL is needed. | `ureq` (blocking only). |
| LLM SSE parsing | `sse-stream@0.2.x` (resolves to 0.2.6) | Already in the tree through rmcp (`^0.2.4`), so it adds zero crates. `SseStream::from_bytes_stream(resp.bytes_stream())`. | `eventsource-stream@0.2.3` (Codex uses it, no release since 2022). `reqwest-eventsource@0.6.0` (no release since 2024). |
| DeepSeek/Qwen client | **Hand-rolled** (≈200–300 LOC of serde types + the stream loop) | Both providers stream a non-standard `reasoning_content`. DeepSeek requires it to be sent back on later turns when tools are in use, or it returns HTTP 400 ([DeepSeek thinking mode](https://api-docs.deepseek.com/guides/thinking_mode)). Qwen uses `enable_thinking` / `reasoning.effort` and `stream_options.include_usage` ([Model Studio](https://www.alibabacloud.com/help/en/model-studio/deep-thinking)). `async-openai`'s typed `ChatCompletionStreamResponseDelta` has no `reasoning_content` field (checked in the 0.42.0 source), so you would need `byot` and your own types anyway. | `async-openai@0.42.0` with `chat-completion, byot`. It adds 18 crates (`derive_builder`, `darling`, `secrecy`, `eventsource-stream`, …) and turns on extra reqwest features. |
| SQLite | `rusqlite@0.40.2` with `bundled` + `rusqlite_migration@2.6.0` | Standalone clean build: 27 crates, 14 s at `-j4`. Synchronous, so run it on a dedicated thread or `spawn_blocking`. `bundled` compiles SQLite from the amalgamation, so no system libsqlite is needed. | `sqlx@0.9.0` (sqlite, runtime-tokio, macros, migrate): 155 crates, 35 s standalone, MSRV 1.94. Its compile-time-checked macros need `DATABASE_URL` or offline `.sqlx` data. It uses `libsqlite3-sys` 0.37, so it cannot coexist with rusqlite 0.40. |
| Config format | `toml@1.1.6+spec-1.1.0` + `serde@1.0.229` (`derive`) + `serde_json@1.0.151` | `toml` is now 1.x and implements TOML 1.1. Add `toml_edit@0.25.15` only if you need to write config while keeping comments. | — |
| Config/data paths | `etcetera@0.11.0` (`choose_base_strategy()`) | Gives XDG paths (`~/.config`, `~/.local/share`) on macOS too, which is what CLI users expect. | `dirs@7.0.0` (released 2026-09-05; on macOS it returns `~/Library/Application Support`), `directories@6.0.0` (`ProjectDirs`, no release since 2025-01). |
| Glob tool | `ignore@0.4.33` (`WalkBuilder`, `overrides::OverrideBuilder`) + `globset@0.4.20` (transitive) | From ripgrep. Respects `.gitignore`/`.ignore`, has a parallel walker, released 2026-08-04. | `glob@0.3.4` (no ignore support), `wax@0.7.0`. |
| Grep tool | `grep-searcher@0.1.17` + `grep-regex@0.1.14` + `grep-matcher@0.1.9` | ripgrep's engine: binary detection, context lines, mmap. Use the pieces directly rather than the `grep@0.4.1` facade, which also pulls `grep-printer`/`grep-cli`. | Shelling out to `rg`, which adds a runtime dependency. |
| CLI / errors / logs | `clap@4.6.7` (`derive`), `anyhow@1.0.104`, `thiserror@2.0.21`, `tracing@0.1.44` + `tracing-subscriber@0.3.23` | Standard choices, all actively released. | — |
| OAuth browser hop | `webbrowser@1.2.4` or `open@5.4.4`, plus a tiny `tokio::net::TcpListener` loopback callback | rmcp returns the auth URL and expects `handle_callback(code, state)`. The app must provide the redirect listener and token persistence, for example by implementing `CredentialStore` on SQLite. | — |

## Key findings

- **rmcp moves fast.** 3.0.0 shipped 2026-07-28, followed by 3.1–3.4.1 within eight weeks ([CHANGELOG](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/CHANGELOG.md)). Codex pins `rmcp = "=3.2.0"`. Pin exactly and rely on `Cargo.lock` + `--locked`.
- **rmcp feature defaults are server-oriented** (`default = ["base64","macros","server"]`). The `auth` feature only turns on `__reqwest` without TLS, so also enable `reqwest` (rustls) or `reqwest-tls-no-provider` ([Cargo.toml](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/Cargo.toml)).
- **Remote servers that only speak the old 2024-11-05 HTTP+SSE transport are not supported by rmcp.** Handling them would take a small custom `Transport`. The streamable-HTTP client does consume SSE response streams.
- **aws-lc-sys is the single slowest build unit** (52.7 s build script, on the critical path). It needs only a C compiler: no CMake, no bindgen for non-FIPS builds ([aws-lc-rs requirements](https://aws.github.io/aws-lc-rs/requirements/index.html)).
  - To save about 10–15 s, switch to ring: `reqwest` with `default-features = false`, `rustls-no-provider, http2, charset, system-proxy, json, stream`; `rmcp` feature `reqwest-tls-no-provider`; `rustls@0.23` with `ring`; and call `rustls::crypto::ring::default_provider().install_default()` at startup.
- **Clean-machine prerequisites:**
  - rustup stable ≥ 1.95. That floor comes from the highest MSRV in the set, `rusqlite_migration`. Pin it in `rust-toolchain.toml`. Local stable at time of writing is 1.98.1.
  - A C compiler: Xcode CLT on macOS, `build-essential` on Linux. It is needed for SQLite, aws-lc/ring and onig.
  - No OpenSSL, pkg-config or CMake.
- **The crossterm cadence is slow.** 0.29.0 was released 2025-04-05, though the repo still sees commits (last push 2026-09-14). Codex patches crossterm with its own fork. That is acceptable, but the TUI should not depend on unreleased crossterm fixes.

## Build budget

Measured with `cargo build --release --locked` on a clean target directory. The machine was an Apple M3 Pro (11 cores, 18 GB), rustc 1.98.1, with crates already downloaded. The probe crate had every dependency above except `tui-markdown` and `async-openai`: 327 unique crates, 17 MB binary.

| Variant | Default jobs (11) | `-j 4` |
| --- | --- | --- |
| aws-lc (reqwest default TLS) | 59 s | 83 s |
| ring provider | 51 s | 68 s |
| ring + `tui-markdown` + onig syntect | — | 101 s (machine under load avg ≈5.7; onig itself ≈4 s CPU) |
| + `async-openai` (incremental, on top of aws-lc) | +34 s rebuild cascade | — |

Slowest units at `-j4` (aws-lc variant):

| Unit | Time |
| --- | --- |
| `aws-lc-sys` build script | 52.7 s |
| `libsqlite3-sys` build script | 13.5 s |
| `rmcp` | 11.9 s |
| `regex-automata` | 9.8 s |
| final binary | 8.1 s |
| `rustls` | 7.5 s |
| `reqwest` | 6.8 s |
| `tokio` | 6.7 s |

**Budget:** plan for 1–1.5 min on a modern laptop and 2–4 min on a 4-core Linux box or CI runner (extrapolated, not measured). Crate download and a rustup install come on top. Incremental rebuilds of the app crate take about 8 s in release, so use `dev` builds while iterating.

## Could not verify / open uncertainties

- **Build times** come from one machine that other processes were also loading. Linux was not measured.
- **Current DeepSeek/Qwen model IDs.** The DeepSeek page (fetched via a summarizer) cited `deepseek-flash` with `thinking: {type: enabled}`. The claim that Qwen's `reasoning.effort` supersedes `enable_thinking` comes from search-result snippets of alibabacloud.com; I did not read those pages in full.
- **`sse-stream` has thin docs.** The `from_bytes_stream` name comes from the 0.2.6 deprecation warning seen while compiling. Its 0.3.0 release (2026-09-18) is not yet what rmcp uses.
- **Runtime speed of syntect's `fancy` vs `onig` backends** was not measured.
- **rmcp behaviour against real OAuth servers**, and whether servers that are legacy-SSE-only still matter to users, were not tested.
- **`tui-markdown` fidelity** (tables, nested lists) was not evaluated beyond its README.

## Sources

- crates.io API for every crate listed; the GitHub API for repo activity (ratatui, crossterm, rust-sdk, async-openai, rusqlite, sqlx, ripgrep, syntect, tui-markdown, ratatui-textarea).
- ratatui 0.30 docs via Context7 (`/websites/rs_ratatui_0_30_0`), sourced from docs.rs: [`ratatui::run`](https://docs.rs/ratatui/0.30.0/ratatui/init/fn.run.html), [`Scrollbar`](https://docs.rs/ratatui/0.30.0/ratatui/widgets/struct.Scrollbar.html), [`Terminal::insert_before`](https://docs.rs/ratatui/0.30.0/ratatui/prelude/struct.Terminal.html).
- [ratatui-textarea README](https://github.com/ratatui/ratatui-textarea).
- [tui-markdown README](https://github.com/joshka/tui-markdown/tree/main/tui-markdown).
- rmcp: [README](https://github.com/modelcontextprotocol/rust-sdk) (Transports table), [OAUTH_SUPPORT.md](https://github.com/modelcontextprotocol/rust-sdk/blob/main/docs/OAUTH_SUPPORT.md), [`auth.rs`](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/transport/auth.rs) (`CredentialStore`), CHANGELOG, `Cargo.toml` features.
- [reqwest CHANGELOG](https://github.com/seanmonstar/reqwest/blob/master/CHANGELOG.md) (v0.13.0 TLS defaults).
- [async-openai README](https://github.com/64bit/async-openai) and the 0.42.0 source (`types/chat/chat_.rs`).
- [DeepSeek thinking mode](https://api-docs.deepseek.com/guides/thinking_mode); [Alibaba Model Studio deep thinking](https://www.alibabacloud.com/help/en/model-studio/deep-thinking) and [OpenAI compatibility](https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope).
- [aws-lc-rs build requirements](https://aws.github.io/aws-lc-rs/requirements/index.html).
- Reference implementation: [openai/codex `codex-rs/Cargo.toml`](https://github.com/openai/codex/blob/main/codex-rs/Cargo.toml), [`tui/Cargo.toml`](https://github.com/openai/codex/blob/main/codex-rs/tui/Cargo.toml), [`rmcp-client/Cargo.toml`](https://github.com/openai/codex/blob/main/codex-rs/rmcp-client/Cargo.toml).
