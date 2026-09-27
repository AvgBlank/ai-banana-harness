# Spec: Banana Harness — Rust + ratatui port

Status: ready to implement · Written 2026-09-27 · Deadline ≈ 2026-10-04
Map: [map.md](map.md) · Glossary: [CONTEXT.md](../../CONTEXT.md) · ADRs: [0001](../../docs/adr/0001-rust-local-first-fork.md), [0002](../../docs/adr/0002-in-turn-waiting.md)

This spec is the destination of the wayfinder map. Every decision below links back to the ticket that made it. Implementers are AI agents: each work item in §11 names the check that proves it's done.

**Research inputs.** Each research file lives on its own local branch; read it with `git show <branch>:<path>`.

| Research | Branch | File |
|---|---|---|
| DeepSeek and Qwen API behaviour | `research/deepseek-qwen-api` | `docs/research/deepseek-qwen-api.md` |
| Rust crate choices | `research/rust-crates` | `docs/research/rust-crates.md` |
| Core-loop inventory | `research/core-loop-inventory` | `docs/research/core-loop-inventory.md`, which maps every TS file referenced below |

---

## 1. Goal

Banana Harness is a local-first coding agent. The hackathon flow is:

1. The evaluator exports `AI_API_KEY`.
2. They run `make setup` and then `make run`.
3. A ratatui TUI opens. They choose DeepSeek or Qwen in a popup and paste an evaluation issue.
4. The agent reads, edits and tests the code in `WORKSPACE` until the issue is resolved, streaming every step to the TUI.

`make test` must pass offline.

## 2. Non-goals

These are out of scope for this effort (see the map):

- **Hosted surfaces:** the HTTP API, the SDKs and the React UI.
- **Multi-tenant infrastructure:** auth, tenants, sharing, Postgres and Redis.
- **Deployment and integrations:** Helm, Docker and the TrueFoundry integration.
- **Execution features:** schedules, sandboxes, Code Mode and web search.
- **UI features:** OpenUI, metrics, images and thinking mode.
- **Platforms and data:** Windows, and importing existing TrueForge data.
- **Providers:** anything other than DeepSeek and Qwen.
- **Named agents.** v1 has a single agent.

---

## 3. Architecture ([Crate architecture and event model](issues/08-crate-architecture-and-event-model.md))

```
rust/
  Cargo.toml            [workspace] resolver = "3"; every dependency pinned under [workspace.dependencies]
  rust-toolchain.toml   channel = "1.95" (rustup installs it automatically)
  config/default.toml   committed evaluation config, embedded with include_str!
  crates/
    harness-llm/        leaf: DeepSeek/Qwen chat client
    harness-tools/      leaf: host tools + guardrail engine
    harness-mcp/        leaf: MCP client (rmcp), OAuth
    harness-store/      leaf: SQLite
    harness-core/       config, events, turn runner, thread loop, built-ins; depends on the 4 leaves
    harness-tui/        ratatui app; depends on core
    banana-harness/     [[bin]]: `banana-harness` (TUI) and `banana-harness run` (headless); depends on core + tui
```

**Rules for the crates:**
- The leaf crates never depend on each other. Each defines its own boundary types, and `harness-core` converts between them. This is what lets agents build the leaves in parallel.
- Crate names are neutral. The product name lives only in `harness-core::APP_NAME` ("Banana Harness"), `harness-core::APP_SLUG` ("banana-harness") and the bin's `Cargo.toml` `name`. A rename touches exactly those three.

**Pinned dependencies** (versions from the crate research):

| Area | Crates |
|---|---|
| Async and HTTP | tokio 1.53 (multi-thread), tokio-util (`CancellationToken`), reqwest 0.13 with rustls-`ring`, sse-stream 0.2 |
| MCP | rmcp 3.4.1, default features off, **pinned exactly** |
| TUI | ratatui 0.30, crossterm 0.29 (`event-stream`), ratatui-textarea 0.9, tui-markdown 0.3, syntect 5.3 + two-face |
| Diffs | similar 3.2 |
| Storage and config | rusqlite 0.40 (bundled), toml 1.1, etcetera 0.11 |
| Search | ignore + grep-searcher/grep-regex |
| Utilities | serde/serde_json, anyhow, ulid, clap |

**Conventions:**
- `anyhow::Result` everywhere. A `thiserror` enum only where a caller branches on the error.
- No `unwrap()`/`expect()` outside tests.
- Tests live in each crate's `tests/`.
- `cargo fmt` and `cargo clippy -- -D warnings` must be clean.

---

## 4. Core (`harness-core`)

### 4.1 Surface API

```rust
Harness::open(config, store, provider) -> Harness
Harness::start_turn(&self, session_id, input: String) -> TurnHandle

struct TurnHandle {
    events: mpsc::Receiver<Event>,
    fn reply(&self, action_id, Answer),   // resolves a required action
    fn cancel(&self),
}
```

Both surfaces use only this API.

### 4.2 Events

One `#[serde(tag = "type")]` enum, which keeps the TS names:

| Event | Payload |
|---|---|
| `turn.created` | |
| `turn.done` | `state`: `done` / `cancelled` / `error` |
| `model.message` | start marker, then the final `{content, tool_calls, finish_reason, usage}` |
| `model.message.delta` | streaming fragment |
| `tool.response` | `{tool_call_id, content, blocked?}` |
| `thread.created` | `{thread_id, parent, title}` |
| `thread.done` | |
| `tool.approval_required` | `{action_id, thread_id, tool_call}` |
| `tool.response_required` | `{action_id, thread_id, question, options}` |
| `context.compacted` | `{usage}`. New: TS never streamed compaction. |
| `mcp.auth_required` | `{server, url}` |

Every event gets a lowercase ULID `id`. Deltas are streamed but not persisted.

### 4.3 Turn runner

1. Creates the `turn` row and emits `turn.created`.
2. Runs the orchestrator.
3. **Persists each durable event, then forwards it.** The runner is the only writer to SQLite.
4. Writes the terminal state, with this precedence: cancelled, then error, then done.

**Required actions** ([ADR 0002](../../docs/adr/0002-in-turn-waiting.md)):
- The waiting thread parks on a `oneshot` stored in the runner's map under `action_id`.
- `reply` resolves the matching entry.
- `cancel` resolves every pending entry as deny and trips the `CancellationToken`.

### 4.4 Thread loop

TS reference: `core/runtime/AgentThread.ts`.

**State is derived from context.** Walk back to the last assistant message and subtract the tool replies:
- no open calls → `llm-call-required`
- open calls → `tool-response-required`

**LLM step:**
1. Compaction check (§4.6).
2. Build the request: system prompt (§4.9), then the context, then the tools.
3. Stream the response as deltas.
4. `finish_reason = length` becomes the error "max_tokens breached".
5. No tool calls means the thread is done.

**Tool step:**
- Run all open calls in parallel through the tool dispatcher (§4.5).
- Apply large-response handling (§4.7).
- Emit `tool.response` and append the results to context.
- A `create_sub_agent` call hands control to the orchestrator.

**Limits and failures:**
- The iteration limit is `agent.max_iterations`, default 100 LLM calls per turn.
- There is also the wall-clock limit `agent.max_turn_minutes`, default 30.
- `execute` never panics or throws. Every failure becomes an error `thread.done`.

**Context usage:** after each LLM call, use the provider's `usage`. For every other append, add chars/4.

**Internal messages:** a user message wrapped in `<bh-internal>…</bh-internal>` is internal, and the system prompt tells the model never to write one.

### 4.5 Tool dispatch

TS reference: `core/mcp/*`, `core/runtime/DeferredTool.ts`.

**Tool sources:** `enum ToolSource { Host, Mcp(McpClient), Deferred, Builtin }`.
- Host tools are the six from §7.1.
- The builtins are `create_sub_agent` (root only), `ask_user_question` (root only) and `current_datetime`.

**Tool names:**
- Sanitised to `[A-Za-z0-9_-]` and capped at 64 characters, with a numeric suffix (1–50) on a collision.
- MCP tool descriptions get the prefix `mcp server: {name}\n`.
- JSON-schema parameters keep only `properties` and `required`.

**Registration and deferral:**
- MCP servers with `preload = true` register their tools directly.
- The others go behind the `deferred-tools` meta-tools (`list_tools`, `get_tool_info`, `call_tool`). Their descriptions are listed in the system prompt, capped at 200 characters.

**Result rendering:**
- an error becomes `{"error": msg}`
- `structuredContent` is JSON-encoded
- text parts are joined with `\n`

**Before any call runs**, it passes the permission check (§7) first.

### 4.6 Compaction

TS reference: `core/capabilities/builtins/ContextCompaction.ts`. Port the prompt text verbatim.

- **Threshold:** `min(floor(ctx × 0.8), ctx − max_output)`. The fallback is 50,000.
- **Trigger:** runs before every LLM call, in subagents too, when `prompt + completion + 931 ≥ threshold`.
- **What it does:**
  1. Serialise the context with the TS markers.
  2. Make a non-streaming call to the same model with the 8-section summary prompt.
  3. Replace the context with `[assistant: summary, user: <bh-internal>continuation…]`.
  4. Set usage to the summary's input tokens + 42.
  5. Emit `context.compacted`.

### 4.7 Large tool responses

TS reference: `core/capabilities/builtins/LargeToolResponse.ts`. Tokens are estimated as chars/4.

**Selection:**
1. A batch under 6,000 tokens passes through untouched.
2. Otherwise, block every result of 6,000 tokens or more.
3. Then block the largest remaining results until the total is under 10,000.

**What replaces a blocked result:**
- **Normal result:** the full content goes to `<workspace>/.banana-harness/tool-output/{tool}_{i}_{epoch}.txt`. The model gets the path, guidance to use `read_file`/`grep`, and a preview of the first and last 100 characters.
- **Failure:** truncated to 500 characters.

`<workspace>/.banana-harness/` contains a `.gitignore` of `*`, so it never dirties the repo under test.

### 4.8 Subagents

TS reference: `core/capabilities/builtins/DynamicSubAgents.ts`, `AgentThreadOrchestrator.ts`.

**The tool:** `create_sub_agent{name, input}` on the root thread only, so depth is 1.

**What a child gets:**
- the `SUB_AGENT_IDENTITY` system prompt
- `[user: input]` as its messages
- the parent's tool sources
- no `ask_user_question` and no `create_sub_agent`
- compaction and large-response handling still on

**Scheduling:**
- Only leaf threads run: a parent with live children waits.
- At most 5 run at once, a tokio `JoinSet` gated by a `Semaphore(5)` with events fanned in over mpsc.
- A child's final text becomes the parent's tool message.

### 4.9 Instructions and skills

TS reference: `core/InstructionBuilder.ts`, `core/sandbox/skills/constants.ts`.

**System prompt structure:** XML sections joined with `\n\n`, empty ones dropped:

```
<system-prompt>
  <agent-identity/>
  <agent-capabilities>
    <internal-messages/>
    <workspace/>                      new: the workspace path and the tool rules
    <skills/>
    <deferred-tools-instructions/>
    <sub-agents/>
  </agent-capabilities>
  <user-instructions>                 root only, CDATA
    config instructions + workspace AGENTS.md
  </user-instructions>
</system-prompt>
```

**Skill loader** (new):
- Scans `skills.paths` for `*/SKILL.md` and parses the frontmatter `name` and `description`.
- Names must match `^[A-Za-z0-9._\-/:]+$`. At most 50 skills.
- Renders the TS `<skill>` blocks with absolute paths. The agent reads each skill with `read_file`.

### 4.10 `ask_user_question`

- Arguments: `{question, options[0..5]}`. Port the TS description text, but drop the OpenUI line.
- It emits `tool.response_required` and waits for `reply`.
- In a headless run it's auto-answered with "No user available; proceed with your best judgement."

---

## 5. Config ([Config schema](issues/09-config-schema.md))

**Files and precedence** (later wins):
1. the embedded `config/default.toml`
2. `~/.config/banana-harness/config.toml`
3. `<workspace>/.banana-harness/config.toml`

```toml
[providers.deepseek]
base_url = "https://api.deepseek.com"
model = "deepseek-flash"
temperature = 0.0

[providers.qwen]
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "qwen3.8-flash"
temperature = 0.0
seed = 42

[agent]
instructions = "You are Banana Harness, a coding agent…"
max_iterations = 100
max_turn_minutes = 30
subagents = true

[tools]
bash_timeout_secs = 120        # max 600
output_max_bytes = 30720

[permission]                   # §7
bash = { "*" = "allow", "sudo *" = "deny" }   # …plus the full denylist

[skills]
paths = [".banana-harness/skills", "~/.config/banana-harness/skills"]

# [mcp.fs]     type = "local",  command = ["npx", "-y", "@modelcontextprotocol/server-filesystem", "."], env = {}, preload = false
# [mcp.remote] type = "remote", url = "https://…", headers = { Authorization = "Bearer {env:MY_TOKEN}" }, oauth = true
```

**Secrets:**
- The key comes only from `AI_API_KEY`, never from a file.
- `{env:VAR}` is expanded in MCP headers and env.

**Thinking is off for both providers:** DeepSeek `thinking: {type: "disabled"}`, Qwen `enable_thinking: false`.

---

## 6. Persistence ([Persistence scope](issues/10-persistence-scope.md))

**The database:** `harness-store`, rusqlite bundled, at `etcetera` data dir `/banana-harness/harness.db`, in WAL mode.

**Migrations:** `migrations/NNNN.sql` files are embedded and applied in order, tracked with `PRAGMA user_version`.

| Table | Columns |
|---|---|
| `session` | `id` ULID, `title`, `workspace`, `provider`, `model`, `created_at`, `updated_at` |
| `turn` | `id`, `session_id`, `seq`, `input`, `state`, `error`, `usage_json`, `started_at`, `finished_at` |
| `event` | `id` ULID, `turn_id`, `thread_id`, `type`, `payload_json`, `created_at`. Durable events only. |
| `thread` | `id`, `session_id`, `parent_id`, `title`, `context_json`, `context_usage_json`, `updated_at`. Upserted after every LLM or tool step. |
| `mcp_oauth_token` | `server` PK, `token_json`, `expires_at` |
| `setting` | `key` PK, `value`. For example `last_provider`. |
| `checkpoint` | `turn_id`, `git_ref`, `created_at` |

**Resume:** `/sessions` loads the session's root `thread.context_json` and continues from it.

---

## 7. Tools and guardrails ([Guardrail policy](issues/04-guardrail-policy.md))

### 7.1 Host tools (`harness-tools`)

| Tool | Behaviour |
|---|---|
| `read_file{path, offset?, limit?}` | Returns numbered lines, 2,000 lines by default. |
| `write_file{path, content}` | Creates parent directories as needed. |
| `edit{path, old_string, new_string, replace_all?}` | Fails unless `old_string` matches exactly once, or `replace_all` is set. |
| `glob{pattern}` | Uses the `ignore` crate, so `.gitignore` is respected. |
| `grep{pattern, path?, glob?}` | Uses the grep crates. |
| `bash{command, timeout_secs?}` | Runs via `bash -lc` in its own process group, with cwd set to the workspace. Returns stdout, stderr and the exit code. |

### 7.2 The permission engine

Modelled on opencode.

- **Actions:** each key is `allow`, `ask` or `deny`.
- **Keys:** `read`, `edit` (covers `write_file` and `edit`), `glob`, `grep`, `bash`, `external_directory`, `mcp`, which matches on tool-name patterns, and `doom_loop`.
- **Pattern maps:** for `bash` and `edit`, the **last matching pattern wins**.
- **Default is autonomous:** everything is `allow` except:
  - `bash` deny patterns: `sudo *`, `su *`, `rm -rf /*`, `rm -rf ~*`, `git push*`, `curl * | sh*`, `curl * | bash*`, `wget * | sh*`, `mkfs*`, `dd *`, `shutdown*`, `reboot*`, `chmod -R 777 /*`
  - `read` on `.env*`: deny
  - `external_directory`: deny
- **`ask`:** raises `tool.approval_required` with the choices Allow once, Allow for session (kept in memory) or Deny.
- **Confinement:** paths are resolved with `canonicalize`, symlinks included, and must stay under the workspace, or under `.banana-harness/tool-output`, which is always allowed.
- **Limits:**
  - `bash` defaults to a 120 s timeout (at most 600 s) and kills the process group when it expires.
  - Output is capped at `output_max_bytes` (default 30 KB); larger output goes through §4.7.
- **Doom loop:**
  - Three identical consecutive `(tool, args)` calls inject an internal warning.
  - A fifth ends the turn with an error.
- **Checkpoint:**
  - Before the first mutating call in a turn, in a git workspace, run `git stash create` (falling back to `HEAD`) and record the ref in `checkpoint`.
  - `/undo` runs `git checkout <ref> -- .` and then removes the files the turn created.
- **Redaction:**
  - The value of `AI_API_KEY` is replaced with `[REDACTED]` in everything persisted, displayed or returned by a tool.
  - `AI_API_KEY` is stripped from the environment of `bash` subprocesses.
- **Blocked calls:** the model receives `{"error":"Blocked by guardrail: <rule>"}`, and `tool.response.blocked = true`.

### 7.3 MCP (`harness-mcp`)

**Transports:** rmcp's stdio client (`type = "local"`) and streamable-HTTP client (`type = "remote"`).

**Operations:** `list_tools` pages with a guard against a repeated cursor, and `call_tool` returns content plus an `is_error` flag.

**OAuth:**
1. Use rmcp's OAuth 2.1 flow.
2. Open the browser with `open` (macOS) or `xdg-open` (Linux).
3. Listen on `127.0.0.1:<random port>/callback`.
4. Emit `mcp.auth_required{url}` so the TUI can show the link.

Tokens are handed to the caller through a callback, and `harness-core` persists them to `mcp_oauth_token`.

A server whose tools are never preloaded or called is never connected.

---

## 8. TUI ([TUI layout and look](issues/05-tui-layout-and-look.md))

**Startup:**
- A modal popup: **Choose provider**, DeepSeek or Qwen. The organisers require it.
- The popup uses ↑/↓ and Enter, with `setting.last_provider` preselected.
- If `WORKSPACE` is unset, the same popup asks for a workspace path. If `AI_API_KEY` is unset, it shows a blocking error.

**Layout:**
- Transcript pane: the main area.
- Sidebar: toggled with Ctrl+B, auto-hidden below 100 columns. It shows the workspace, provider and model, a context-usage bar and %, tokens, elapsed time, modified files with +/−, MCP status and the subagent tree.
- Input: ratatui-textarea, 3–8 rows, docked at the bottom. Enter sends; Alt+Enter adds a newline; bracketed paste works.
- Status line: a spinner and key hints.

**Transcript items:**
- user messages
- streamed assistant markdown (tui-markdown)
- tool cards, collapsed to one line (`✓ edit src/x.rs +12 −3`, `✗ bash cargo test (exit 101)`). Enter or Ctrl+O expands a card to show a syntect-highlighted diff (similar) or the last 40 lines of bash output.
- subagent cards
- `⟳ context compacted`
- red error and **blocked** badges
- an end-of-run summary card: status, files changed, tokens, duration

**Modals:**
- Approval: `a` allow, `s` allow for session, `d` deny.
- Question: the options plus a free-text field.
- MCP auth: shows the URL.

**Keys:**
- Ctrl+C cancels the turn; a second Ctrl+C quits.
- PgUp and PgDn scroll.
- Ctrl+E opens the input in `$EDITOR`.

**Slash commands:** `/new`, `/sessions`, `/provider`, `/undo`, `/help`, `/quit`.

**Theme:** one dark palette with a banana-yellow accent (`#FFE135`), defined in `theme.rs`, truecolor with a 256-colour fallback.

**Rendering:** the terminal is redrawn at most every 33 ms. Deltas are appended to the live message and never re-render the whole transcript.

## 9. Headless run

`banana-harness run --provider deepseek|qwen [--workspace P] [--json] "<task>"`

- It uses the same `Harness`.
- With `--json` it prints JSONL events; otherwise it prints plain text.
- `ask_user_question` is auto-answered. `ask` permissions are treated as deny.
- Exit codes: 0 for done, 1 for error, 2 for cancelled or limit reached.

---

## 10. Makefile and tests ([Test and parity strategy](issues/11-test-and-parity-strategy.md))

The Makefile sits at the repo root and uses `CARGO ?= $(HOME)/.cargo/bin/cargo`, falling back to `cargo` on `PATH`.

| Target | Does |
|---|---|
| `setup` | Checks for a C compiler (`cc`). If it's missing, prints the fix (`xcode-select --install` / `apt install build-essential`) and exits 1. If `cargo` is missing, runs `curl … sh.rustup.rs \| sh -s -- -y --profile minimal`. Then `cargo build --release --locked`. |
| `run` | `AI_API_KEY=$(AI_API_KEY) WORKSPACE=$(WORKSPACE) target/release/banana-harness` |
| `test` | `cargo test --workspace --locked`, offline. If `AI_API_KEY` is set, also `scripts/live-smoke.sh`: a headless run on `tests/fixtures/buggy-repo` (one failing test to fix) that asserts the test then passes. |
| `clean` | `cargo clean` |

Also commit `.env.example` containing just `AI_API_KEY=`.

**Test layers:**
- **`harness-llm`:** recorded SSE fixtures for DeepSeek and Qwen, covering text, parallel tool calls, Qwen's empty-id follow-up chunks, usage on the last chunk, `length`, and an HTTP error.
- **`harness-tools`:**
  - table tests for the permission matcher, where the last match wins
  - confinement, including a symlink escape
  - `edit` uniqueness
  - bash timeout and kill
  - redaction
- **`harness-mcp`:** a stdio round trip against an rmcp fixture server (dev-dependency).
- **`harness-store`:** migrations and CRUD against a temp database.
- **`harness-core` scenarios**, using a fake OpenAI-compatible HTTP server (queued SSE fixtures) and a temp git repo:
  - a plain answer
  - read → edit → bash
  - `ask` approval, both allow and deny
  - `ask_user_question`
  - two parallel subagents, plus the limit of 5
  - compaction firing
  - a large-output spill
  - the iteration limit
  - `length`
  - cancel in the middle of a turn
  - a guardrail block
  - the doom loop
  - a checkpoint followed by `/undo`
  - a golden snapshot of the system prompt
- **`harness-tui`:** `TestBackend` buffer assertions for the provider popup, a transcript containing a tool card, and the approval modal.

---

## 11. Implementation plan

> **Superseded (2026-09-27):** the canonical plan is now the vertical-slice tickets in `../banana-harness/issues/` (01–16), generated by to-tickets from `../banana-harness/spec.md`. The table below is kept only for the TS reading references.

Every item is sized for one agent session. The "Done when" check in the table applies on top of `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and the named tests passing. TS paths are the reading reference: port behaviour, not structure.

| # | Item | Crate | Depends on | Done when |
|---|---|---|---|---|
| 01 | Workspace skeleton: `rust/` layout, toolchain file, pinned deps, empty crates, `APP_NAME`/`APP_SLUG`, `config/default.toml`, Makefile, `.env.example` | all | — | `make setup && make test` succeeds on a fresh `ubuntu:24.04` container and on macOS |
| 02 | LLM client: request builder, SSE parser, tool-call assembly by index, usage and finish reason, thinking off | harness-llm | 01 | `cargo test -p harness-llm` (fixtures in §10) |
| 03 | Host tools + permission engine + confinement + bash limits + redaction | harness-tools | 01 | `cargo test -p harness-tools` |
| 04 | MCP client: stdio + streamable HTTP, list/call, OAuth with localhost callback | harness-mcp | 01 | `cargo test -p harness-mcp` (stdio round trip). OAuth verified by hand against one public MCP server. |
| 05 | Store: schema, migrations, typed read/write functions | harness-store | 01 | `cargo test -p harness-store` |
| 06 | Core foundations: config loading and merging, `Event` enum, instruction builder, skill loader | harness-core | 01 | tests for config precedence, the system-prompt snapshot and skill parsing |
| 07 | Thread loop + turn runner + `TurnHandle` + required actions + persistence writes + limits | harness-core | 02, 03, 05, 06 | scenarios: plain answer, read→edit→bash, approval, cancel, iteration limit, `length` |
| 08 | Tool dispatch: name sanitising, MCP integration, preload/deferred meta-tools | harness-core | 04, 07 | scenario: a deferred MCP call through `call_tool` |
| 09 | Compaction + large-response spill | harness-core | 07 | scenarios: compaction, spill |
| 10 | Subagents + leaf scheduler (limit 5) | harness-core | 07 | scenarios: 2 parallel children, the limit of 5 |
| 11 | `ask_user_question`, doom loop, checkpoint + undo | harness-core | 07 | the matching scenarios |
| 12 | TUI shell: app loop, provider/workspace popup, input, streaming transcript. Can start against a hand-fed `Event` stream after 06. | harness-tui, bin | 06 (07 to integrate) | `TestBackend` test of the popup; manual run against the fake provider server |
| 13 | TUI detail: tool cards and diffs, sidebar, modals, slash commands, sessions list, theme, summary card | harness-tui | 12, 07 | `TestBackend` tests from §10 |
| 14 | Headless `run` subcommand + `scripts/live-smoke.sh` + `buggy-repo` fixture | bin | 07 | `banana-harness run --json` produces valid JSONL; the live smoke passes with a real key |
| 15 | Cutover (§12) | repo | 01–14 | `git ls-files` shows no TS packages; CI is green |
| 16 | Clean-environment verification: a fresh Linux container and a fresh macOS user. Run `make setup`, `make run` and `make test`, then resolve one real GitHub issue with each provider. | — | 15 | evaluator steps 1–5 of the brief succeed |

**Schedule:**

| Day | Date | Work |
|---|---|---|
| D1 | 09-27 | planning (done) |
| D2 | 09-28 | 01, then 02–06 in parallel |
| D3 | 09-29 | 07, then 12 in parallel |
| D4 | 09-30 | 08, 09, 10, 11, 13 in parallel |
| D5 | 10-01 | 14, fixes, integration |
| D6 | 10-02 | 15 |
| D7 | 10-03 | 16; 10-04 is buffer and submission |

## 12. Cutover (item 15)

**Delete:**
- `packages/`, `python/`, `charts/`, `docker-compose*.yml`, `Dockerfile*`
- `pnpm-*`, `package.json`, `tsconfig*`, eslint/prettier config, `.changeset/`, `patches/`, `vendor/`
- the TS scripts in `scripts/` (keep `scripts/live-smoke.sh`)
- `.github/fern/`, `docs/openapi.json`

**Replace `.github/workflows/`** with a single `ci.yml`: fmt, clippy and test on `ubuntu-latest` and `macos-latest`.

**Rewrite the docs:**
- `README.md`: what it is, the evaluator steps, config, key bindings.
- Root `AGENTS.md`: the Rust conventions from §3, plus "keep the glossary in `CONTEXT.md`". Keep the `CLAUDE.md` → `@AGENTS.md` pattern.
- Delete the nested TS `AGENTS.md`/`CLAUDE.md` files.

**Keep:** `docs/` user docs only where they still apply. Otherwise delete them, so no stale docs are left behind.

`rust/` was later moved to the repository root, after the cutover.
