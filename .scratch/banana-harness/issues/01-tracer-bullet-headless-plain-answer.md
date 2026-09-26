# 01: Tracer bullet: a headless run answers a plain question end to end

**What to build:** create the Cargo workspace in the `rust/` subfolder, with its seven packages, pinned toolchain and pinned dependencies. Add a root Makefile, a `.env.example`, the product-name constants, and the committed default config for both providers.

Wire the thinnest complete path:

`banana-harness run --provider deepseek "<question>"`
→ core opens a session
→ the turn runner streams the model's plain-text reply from the real OpenAI-compatible client (thinking off)
→ it persists the session, turn and durable events to SQLite (migrations included)
→ it prints the reply.

No tools yet. Also build the **fake OpenAI-compatible HTTP server** test helper that all later core tests use (seam 1): it serves queued SSE fixtures, and the provider base URL points at it.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] `make setup` succeeds on a fresh Linux container with no Rust installed (it installs rustup) and on macOS. It exits with an actionable message when no C compiler is present.
- [ ] `make test` passes offline with no `AI_API_KEY`.
- [ ] A seam-1 test gets a plain answer through the fake server, both for a DeepSeek-format stream and for a Qwen-format stream (usage on the last chunk). It asserts the event sequence (turn created, model message start, deltas, final message, turn done) and that the rows can be read back from SQLite.
- [ ] A `length` finish reason ends the turn with the error "max_tokens breached". An HTTP error from the provider ends the turn with an error. Neither panics.
- [ ] With a real key, `banana-harness run --provider deepseek "say hi"` and `--provider qwen` both print a reply.
- [ ] The key comes only from `AI_API_KEY`. It is redacted in the persisted rows and never appears in any committed file.
- [ ] `cargo fmt --check` and clippy with warnings denied are clean.
