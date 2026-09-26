# Map: Banana Harness — Rust + ratatui port

Label: wayfinder:map

## Destination

**Reached:** [spec.md](spec.md).

A migration spec for **Banana Harness**, a Rust + ratatui fork of TrueForge. It is a local-first coding agent that resolves an evaluation issue via `make setup` / `make run` / `make test` on a clean macOS/Linux machine, using only `AI_API_KEY`. The spec must be executable by AI agents within one week (deadline ≈ 2026-10-04).

## Notes

- **Context:** hackathon submission. The brief is "AI Harness Hackathon 2026: Standardised Makefile-Based Evaluation Setup". Key rules:
  - `make setup` / `make run` / `make test` / `make clean`, with `make run` launching the TUI
  - the key comes only from `AI_API_KEY` and is never committed
  - text-only models
  - the model config is committed
  - randomness/seed must be controlled
- **Implementers are AI agents:** every spec slice needs an acceptance check an agent can run itself.
- **Glossary:** use `CONTEXT.md`. Load `grilling` + `domain-modeling` for HITL tickets.
- **Decided while charting** (standing constraints, not tickets):
  - Big-bang rewrite in this repo. The TS code is deleted once the port lands. It's a fork, so there are no data imports and no upstream compatibility.
  - The harness runs in-process as a single binary. Surfaces: the TUI plus a **headless run** (`… run "<task>" --json`-style). No HTTP API.
  - The agent's hands are host tools confined to the **workspace**: `read_file`, `write_file`, `edit`, `glob`, `grep`, `bash`. There is no sandbox.
  - The workspace comes from `WORKSPACE=/path make run`, and the evaluator pastes the issue text into the TUI.
  - Autonomy: runs to completion inside **guardrails** (see the Guardrail policy ticket), with no per-step approval in evaluation mode.
  - Providers: DeepSeek and Qwen only.
  - Config (model, agents, MCP servers, skills) is files on disk. Runtime state lives in SQLite.
  - Kept from core: compaction, large-tool-response, deferred tools/selectors, dynamic subagents, skills, MCP (stdio + remote with OAuth).
  - `make setup` installs rustup if missing, then `cargo build --release --locked`. Keep dependencies lean. `make test` runs offline without the key.
  - Platforms: macOS + Linux.
  - Product name "Banana Harness", defined in one place so it's easy to rename.
  - Why the fork drops the hosted product: [ADR 0001](../../docs/adr/0001-rust-local-first-fork.md).

## Decisions so far

<!-- one line per closed ticket: [title](link): gist -->

- [Inventory of core-loop behaviour to port](issues/03-core-loop-inventory.md): about 6.5k TS lines become 3.5–4.5k lines of Rust, with the thread loop the only large piece. New work: stdio MCP, local skill loader, spilling large responses to disk, a compaction event, and an allow-for-session approval policy.
- [DeepSeek and Qwen API behaviour](issues/01-deepseek-qwen-api-behaviour.md): both are OpenAI-compatible, so one client with per-provider quirks. Reasoning must be echoed back in history. Qwen keys are region-bound. DeepSeek can be detected with a free `/models` probe but has no seed.
- [Rust crate choices](issues/02-rust-crate-choices.md): tokio, ratatui 0.30, rmcp 3.4 pinned, a hand-rolled reqwest+SSE LLM client (async-openai drops `reasoning_content`), rusqlite bundled, ripgrep crates. About 1 min clean build on an M3, 2–4 min on a Linux 4-core machine. Needs a C compiler and Rust ≥1.95.
- [Confirm the evaluation procedure with the organisers](issues/06-confirm-evaluation-procedure.md): the issue is pasted by a human, `WORKSPACE` is honoured, the machine is macOS or Linux with internet but maybe no Rust, the provider is picked in a startup popup, there are no run limits, and Qwen is likely Beijing.
- [Provider selection from AI_API_KEY alone](issues/07-provider-selection.md): a startup popup picks DeepSeek (`deepseek-flash`) or Qwen (`qwen3.8-flash`, Beijing). Thinking is off for both, so there's no reasoning to echo back. No key detection.
- [Guardrail policy](issues/04-guardrail-policy.md): opencode-style `allow`/`ask`/`deny` rules where the last match wins. Autonomous by default apart from a `bash` denylist. Paths confined to the workspace. `.env` reads denied. Timeouts and output caps, doom-loop check, git checkpoint, key redaction.
- [TUI layout and look](issues/05-tui-layout-and-look.md): opencode-like. Provider popup, then transcript, toggleable sidebar and bottom input. Collapsible tool cards with diffs, approval and question modals, slash commands, a banana-yellow dark theme.
- [Crate architecture and event model](issues/08-crate-architecture-and-event-model.md): Cargo workspace in `rust/` with 4 independent leaf crates plus core, TUI and binary. Paused turns wait in place (ADR 0002). One `TurnHandle`. TS event names kept. The turn runner is the only DB writer.
- [Config schema](issues/09-config-schema.md): layered TOML (embedded default, then user, then workspace). One agent. `AGENTS.md` appended to instructions. `{env:VAR}` for MCP secrets. Name set in two constants.
- [Persistence scope](issues/10-persistence-scope.md): all durable state in SQLite (sessions, turns, events, thread snapshots, OAuth tokens, settings, checkpoints). Migrations tracked by `user_version`.
- [Test and parity strategy](issues/11-test-and-parity-strategy.md): no TS golden transcripts. A scripted LLM plus SSE fixtures and scenario tests. `make test` runs offline, with a live smoke run only when the key is set.

## Not yet specified

Nothing left: the execution plan, cutover, headless surface, MCP OAuth flow and subagent display were all specified in [spec.md](spec.md) §7–§12.

## Out of scope

- HTTP API, OpenAPI spec, and the generated TS/Python SDKs.
- React packages: `trueforge-ui`, `assistant-ui-runtime`, `frontend`.
- Multi-tenancy: auth (API key/OIDC/TrueFoundry), tenants, session sharing, Postgres, Redis, executor peering, Helm, Docker.
- TrueFoundry enterprise integration.
- Schedules and the controller.
- Sandboxes (local, Daytona, TrueFoundry) and Code Mode.
- Web search (needs a second paid key).
- OpenUI generative UI, metrics dashboards, Monaco (replaced by `$EDITOR`), images and any non-text modality.
- Windows.
- Importing existing TrueForge data.
- LLM providers other than DeepSeek and Qwen.
