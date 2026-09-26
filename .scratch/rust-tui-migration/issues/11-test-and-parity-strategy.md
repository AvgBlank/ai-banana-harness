# Test and parity strategy

Type: grilling
Status: resolved
Blocked by: 03, 08

## Question

How is the port proven correct?

- Which core-loop scenarios get golden event transcripts recorded from the TS harness?
- How does the scripted mock LLM work?
- Which intricate TS unit tests get ported?
- What does `make test` run offline without `AI_API_KEY`?
- What optional live smoke test runs when the key is set?
- How do implementing agents use all this as their acceptance checks?

## Answer

The user asked (2026-09-27) for recommended answers. The full detail is in [spec.md](../spec.md) §10.

- **No golden transcripts are recorded from TS.** The TS code remains a reading reference only.
- **Scripted mock LLM:** `Provider::Scripted` replays canned responses, and tests for the real client feed recorded DeepSeek and Qwen SSE fixtures into the parser.
- **Test layers:**
  - unit tests in each leaf crate
  - scenario tests in `harness-core`, using the scripted LLM and a temporary git workspace
  - store tests against a temporary database
  - TUI buffer assertions with ratatui's `TestBackend`
  - a stdio MCP test against a fixture server
- **`make test`:**
  - Always: `cargo test --workspace --locked`, offline, with no key needed.
  - When `AI_API_KEY` is set: also a live headless smoke run against a small fixture repo.
- **Acceptance check for every implementation ticket:** its named `cargo test` filter passes, plus `cargo clippy -- -D warnings` and `cargo fmt --check`.

**Superseded by to-spec (2026-09-27):** there is no `Provider::Scripted`. Tests point the real client at a fake OpenAI-compatible HTTP server that serves queued SSE fixtures (see `../../banana-harness/spec.md`, Testing Decisions).
