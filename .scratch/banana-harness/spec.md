# Banana Harness: Rust + ratatui coding-agent harness

Status: ready-for-agent

## Problem Statement

We are entering the AI Harness Hackathon 2026 with a fork of TrueForge. TrueForge is a TypeScript, multi-tenant, server-hosted agent harness: a web UI, an HTTP API, generated SDKs, Postgres/Redis, and remote sandboxes. The hackathon evaluates something very different:

- An evaluator clones the repo on a clean macOS or Linux machine.
- They export a single `AI_API_KEY` (DeepSeek or Qwen) and run `make setup` then `make run`.
- A terminal UI must open. The evaluator pastes a GitHub issue, and the harness must resolve it in a workspace repository using a text-only model.

Today none of that works:

- There is no Makefile and no TUI.
- The agent can only touch files through a remote sandbox that won't exist on the evaluator's machine.
- Neither provider is supported the way the brief requires.
- Most of the codebase serves a hosted product the evaluation never exercises.

There is one week until the deadline, and the implementation is done by AI agents.

## Solution

**Banana Harness** is a local-first, terminal-first coding agent written in Rust. It ships as one binary with two surfaces: a ratatui **TUI** and a **headless run**.

**Evaluator flow:**
1. `make setup` installs Rust if it's missing and builds the binary.
2. `make run` opens the TUI.
3. A popup asks for the **provider** (DeepSeek or Qwen), and for the **workspace** path if `WORKSPACE` is unset.
4. The evaluator pastes the **evaluation issue**.

**What the agent does:**
- It reads, searches, edits and runs commands in the workspace with host tools. **Guardrails** confine it to the workspace and block dangerous commands.
- It streams every step (model text, tool calls, diffs, command output, subagents) into a polished TUI.
- It runs to completion without per-step approval.

**Retained from TrueForge:** the agent loop logic is ported faithfully: context compaction, large-tool-response handling, deferred tools, dynamic subagents, skills and MCP (stdio plus remote with OAuth). All durable state lives in SQLite.

**Removed:** everything that existed only for the hosted product.

## User Stories

### Evaluator: setup and launch

1. As an evaluator, I want `make setup` to install every dependency on a clean machine, so that I don't have to discover or install anything by hand.
2. As an evaluator, I want `make setup` to install the Rust toolchain automatically when it's missing, so that a machine without Rust still works.
3. As an evaluator, I want `make setup` to stop with a clear, actionable message when a C compiler is missing, so that I know the exact command to fix it.
4. As an evaluator, I want `make run` to launch the TUI directly, so that I never need team-specific commands.
5. As an evaluator, I want the harness to read the key only from `AI_API_KEY`, so that I never edit source or config files.
6. As an evaluator, I want a clear error in the TUI when `AI_API_KEY` is unset, so that I don't wonder why nothing happens.
7. As an evaluator, I want a startup popup where I choose DeepSeek or Qwen, so that the harness talks to the provider my key belongs to.
8. As an evaluator, I want the model for each provider fixed in a committed config file, so that the model used during evaluation is clearly defined.
9. As an evaluator, I want to set the workspace with `WORKSPACE=/path make run`, so that the agent works on the repository under test.
10. As an evaluator, I want the startup popup to ask for a workspace path when `WORKSPACE` is unset, so that launching without it still works.
11. As an evaluator, I want `make test` to run the team's test suite offline, without a key, so that I can verify the submission quickly.
12. As an evaluator, I want `make test` to also run a live end-to-end smoke test when `AI_API_KEY` is set, so that I can see the harness fix a real bug.
13. As an evaluator, I want `make clean` to remove build artefacts, so that I can reset the environment.
14. As an evaluator, I want a `.env.example` listing `AI_API_KEY=` with no value, so that the required variable is documented without committing a secret.

### Evaluator: resolving the evaluation issue

15. As an evaluator, I want to paste a multi-line GitHub issue into the input box, so that the harness receives the full issue text.
16. As an evaluator, I want the agent to run to completion without asking me to approve each step, so that I can watch it work autonomously.
17. As an evaluator, I want to see the model's reply stream token by token, so that I can tell the harness is working.
18. As an evaluator, I want each tool call shown as a compact one-line card with its outcome, so that I can follow the agent's actions at a glance.
19. As an evaluator, I want to expand a file-edit card into a coloured diff, so that I can inspect exactly what changed.
20. As an evaluator, I want to expand a bash card into the tail of its output and its exit code, so that I can see test results.
21. As an evaluator, I want an end-of-run summary showing status, files changed, tokens and duration, so that I can judge the result quickly.
22. As an evaluator, I want to cancel a running turn with Ctrl+C, so that I can stop a run that's going wrong.
23. As an evaluator, I want a sidebar showing the workspace, provider, model, context usage, tokens, elapsed time and modified files, so that I can monitor the run.
24. As an evaluator, I want consistent behaviour across runs, with temperature (and seed where supported) set in config, so that the evaluation is as reproducible as the providers allow.

### Developer: everyday TUI use

25. As a developer, I want to continue a conversation with follow-up messages in the same session, so that I can steer the agent.
26. As a developer, I want `/new` to start a fresh session, so that unrelated tasks don't share context.
27. As a developer, I want `/sessions` to list and resume earlier sessions, so that work survives a restart.
28. As a developer, I want `/provider` to switch between DeepSeek and Qwen, so that I can compare them.
29. As a developer, I want `/undo` to restore the workspace to the checkpoint taken before the last turn's first change, so that I can discard a bad attempt.
30. As a developer, I want `/help` to list commands and key bindings, so that I can learn the TUI.
31. As a developer, I want `/quit` and a double Ctrl+C to exit cleanly, so that my terminal is restored.
32. As a developer, I want Ctrl+E to open my input in `$EDITOR`, so that I can write long prompts comfortably.
33. As a developer, I want Alt+Enter to insert a newline and Enter to send, so that multi-line input is easy.
34. As a developer, I want to scroll the transcript with PgUp and PgDn, so that I can review earlier steps.
35. As a developer, I want to toggle the sidebar with Ctrl+B, and have it hide itself in narrow terminals, so that the transcript stays readable.
36. As a developer, I want the last provider I chose preselected at startup, so that relaunching is fast.
37. As a developer, I want a notice when the context is compacted, so that I know earlier detail was summarised.
38. As a developer, I want errors and blocked actions shown with distinct red badges, so that problems stand out.
39. As a developer, I want the TUI to stay responsive during fast streaming, so that the display never lags or flickers.
40. As a developer, I want a polished dark theme with a banana-yellow accent that degrades gracefully on 256-colour terminals, so that the TUI looks good everywhere.

### Developer: permissions, guardrails and required actions

41. As a developer, I want file tools to reject any path outside the workspace, symlink escapes included, so that the agent can't touch the rest of my machine.
42. As a developer, I want dangerous shell commands (`sudo`, `rm -rf /`, `git push`, piping a download into a shell, disk formatting, shutdown) blocked, so that an autonomous run can't do irreversible damage.
43. As a developer, I want `.env` files unreadable by default, so that secrets in the workspace aren't sent to the model.
44. As a developer, I want the API key redacted from everything persisted, displayed or returned by a tool, and removed from the environment of shell commands, so that it can't leak.
45. As a developer, I want shell commands to time out and kill their whole process group, so that a hung command can't stall the run.
46. As a developer, I want shell output capped, with overflow handled like other large tool responses, so that one command can't flood the context.
47. As a developer, I want a per-turn limit on LLM calls and wall-clock time, so that a runaway agent stops.
48. As a developer, I want the harness to warn the model when it repeats the same tool call, and to stop the turn if the repetition continues, so that loops end.
49. As a developer, I want to configure `allow`, `ask` or `deny` per tool, with command and path patterns where the last match wins, so that I can tighten or loosen the guardrails.
50. As a developer, I want an `ask` rule to pop up an approval modal offering Allow once, Allow for session or Deny, so that I stay in control when I choose to.
51. As a developer, I want a blocked or denied call reported to the model as an error it can react to, so that the agent can change approach.
52. As a developer, I want the agent to be able to ask me a question with suggested options plus free text, so that it can resolve genuine ambiguity.
53. As a developer, I want a running turn to wait in place while it needs my answer, so that answering continues the same turn seamlessly.

### Developer: agent capabilities

54. As a developer, I want the agent to read files with line numbers and ranges, so that it can navigate large files.
55. As a developer, I want the agent to edit files by exact string replacement that fails on an ambiguous match, so that edits are precise.
56. As a developer, I want the agent to find files by glob and search contents by regex while respecting `.gitignore`, so that exploration is fast and relevant.
57. As a developer, I want the agent to run shell commands in the workspace, so that it can build, test and inspect the project.
58. As a developer, I want long conversations compacted into a structured summary automatically before the context overflows, so that long tasks can finish.
59. As a developer, I want oversized tool results saved to a git-ignored folder in the workspace and replaced with a path and preview, so that nothing is lost and the context stays small.
60. As a developer, I want the agent to spawn up to five subagents in parallel for independent subtasks, so that large issues are worked on concurrently.
61. As a developer, I want subagents shown as nested cards and in the sidebar tree, so that I can follow parallel work.
62. As a developer, I want the workspace's `AGENTS.md` added to the agent's instructions automatically, so that project conventions are followed.
63. As a developer, I want skills loaded from local `SKILL.md` directories and offered to the agent by name and description, so that I can extend its know-how without code.
64. As a developer, I want to configure local (stdio) MCP servers by command, so that the agent can use local tool servers.
65. As a developer, I want to configure remote MCP servers by URL, with headers that reference environment variables, so that secrets stay out of config files.
66. As a developer, I want remote MCP servers that need OAuth to open a browser login with a localhost callback, and to persist the tokens, so that authentication happens once.
67. As a developer, I want MCP servers that aren't preloaded offered through discover-then-call meta-tools, so that many servers don't bloat the prompt.
68. As a developer, I want MCP servers that are never used never connected, so that startup is fast and no unnecessary logins appear.

### Developer: configuration and persistence

69. As a developer, I want configuration layered as built-in default, then user config, then workspace config, so that I can override per project.
70. As a developer, I want every setting in human-editable TOML, so that configuration is transparent and version-controllable.
71. As a developer, I want sessions, turns, events, thread contexts, OAuth tokens, settings and checkpoints stored in one local SQLite database, so that all durable state survives restarts.
72. As a developer, I want the database schema upgraded automatically on launch, so that new versions keep my data.

### Developer: scripting

73. As a developer, I want a headless run command that takes a task, provider and workspace, so that I can use the harness in scripts and CI.
74. As a developer, I want headless output available as JSON lines of events, so that other tools can parse it.
75. As a developer, I want headless runs to auto-answer questions and treat `ask` rules as deny, so that they never hang.
76. As a developer, I want distinct exit codes for done, error and cancelled or limit reached, so that scripts can branch on the outcome.

### Maintainer

77. As a maintainer, I want the product name defined in one place, so that renaming "Banana Harness" is a tiny change.
78. As a maintainer, I want the TypeScript product, its packages, its infrastructure and its CI removed at cutover, so that the repo contains only what ships.
79. As a maintainer, I want CI to check formatting, lint and tests on Linux and macOS, so that regressions are caught before submission.
80. As a maintainer, I want the README to document the evaluator steps, configuration and key bindings, so that anyone can run the harness.
81. As a maintainer, I want repository agent conventions rewritten for Rust, so that implementing agents follow consistent rules.

## Implementation Decisions

### Shape

- **One binary, two surfaces.** The TUI is the default and the headless run is a subcommand. There's no HTTP API; the agent loop runs in-process.
- **A Cargo workspace**, built in a `rust/` subfolder beside the TypeScript code until cutover and moved to the repository root after it. It has seven packages:
  - **llm:** the DeepSeek/Qwen chat client.
  - **tools:** the host tools plus the guardrail engine.
  - **mcp:** the MCP client plus OAuth.
  - **store:** SQLite.
  - **core:** config, events, the turn runner, the thread loop and the built-ins.
  - **tui:** the terminal app.
  - **the binary.**
- **Dependency rules:**
  - llm, tools, mcp and store are leaves that never depend on each other; each defines its own boundary types.
  - core depends on all four, the tui depends on core, and the binary depends on core and tui.
  - Package names are neutral. The product name and slug are two constants in core plus the binary name, so a rename touches three lines.

### Core surface

- **The only interface either surface uses:**
  - open a harness from config, store and provider
  - start a turn with a session id and the user's text, which returns a turn handle
  - the handle exposes a stream of events, a reply operation that answers a required action by id, and cancel
- **Paused turns wait in place.** A turn that needs a tool approval or a question answer keeps running, and the waiting thread parks until the handle's reply arrives. This replaces TrueForge's end-then-resume model (ADR 0002). Cancel answers every pending required action with deny and stops the turn.

### Events

- **A single tagged event type that keeps TrueForge's event names:**
  - turn created and turn done, with state done, cancelled or error
  - model message and model message delta
  - tool response, with a blocked flag
  - thread created and thread done
  - tool approval required
  - tool response required, used for questions
  - context compacted (new)
  - MCP auth required
- **Ids:** every event carries a lowercase ULID.
- **Persistence:** deltas are streamed but not persisted. The turn runner is the **only writer** to the database, and it persists each durable event before forwarding it.

### Agent loop (a faithful port of TrueForge behaviour)

- **Thread state is derived from context**, never stored: open tool calls on the last assistant message decide whether to call the LLM or run tools.
- **Finishing and limits:**
  - A `length` finish reason is an error ("max_tokens breached").
  - No tool calls means the thread is done.
  - The per-turn limits are 100 LLM calls and 30 minutes, both configurable.
  - Failures never crash the loop; they end the thread with an error.
- **Context usage:** taken from the provider's usage report after each LLM call; for every other append, estimated as characters/4.
- **Built-ins are called directly** at fixed points in the loop, with no plugin or capability abstraction.
- **Compaction:** runs before every LLM call, in subagents too. The threshold is the smaller of 80% of context length and context length minus max output, falling back to 50,000. It fires when usage + 931 reaches the threshold. It summarises the whole context with TrueForge's 8-section summary prompt, ported verbatim, replaces the context with the summary plus a continuation message, and emits context compacted.
- **Large tool responses** (estimated at characters/4):
  - A batch under 6,000 tokens passes through untouched.
  - Otherwise every result of 6,000 tokens or more is blocked, then the largest others until the total is under 10,000.
  - A blocked result's full content is written to a self-git-ignoring folder inside the workspace and replaced with its path, guidance and a 100-character head/tail preview.
  - Failures are truncated to 500 characters.
- **Subagents:**
  - A root-only create-subagent tool, so depth is 1.
  - A child gets the subagent identity prompt, its task as the only message, and the parent's tools. It gets no question tool and no subagent tool.
  - Only leaf threads run, at most five at once.
  - A child's final text becomes the parent's tool result.
- **Question tool:** a root-only tool that takes a question and up to five options, and raises a required action.
- **Tool names:**
  - Sanitised to letters, digits, underscore and hyphen, capped at 64 characters, with numeric suffixes on collision.
  - MCP tool descriptions are prefixed with the server name.
  - Tool parameter schemas keep only properties and required.
- **Tool sources** are a closed set: host, MCP, deferred meta-tools and built-ins. Dispatch is by enumeration, not trait objects.
- **Deferred tools:** MCP servers not marked preload are reachable through list-tools, get-tool-info and call-tool meta-tools, and are summarised in the system prompt.
- **System prompt:** nested XML sections, with empty ones dropped.
  - identity
  - capabilities:
    - internal-message rule
    - workspace rules
    - skills
    - deferred-tools instructions
    - subagents
  - user instructions, on the root thread only: the config instructions plus the workspace `AGENTS.md`
  - Internal messages use a `bh-internal` tag that the model is told never to produce.

### Provider client

- **One client** covering two providers over OpenAI-compatible chat completions with SSE streaming:
  - DeepSeek: `deepseek-flash` at the DeepSeek API host.
  - Qwen: `qwen3.8-flash` at the Beijing DashScope compatible-mode host.
- **Thinking mode is disabled** for both, so no reasoning content is echoed back and there are no tool-choice restrictions.
- **Streaming details:** tool calls are assembled from streamed fragments **by index**, because Qwen sends empty ids on follow-up chunks. Usage and finish reason are captured from the stream.
- **Settings:** temperature comes from config; seed is sent only for Qwen. There's no key-based provider detection and no Coding Plan handling.

### Host tools and guardrails

- **The six host tools:**
  - read file (numbered lines, offset and limit)
  - write file
  - edit (exact unique match, or replace-all)
  - glob and grep (both `.gitignore`-aware)
  - bash (login shell, own process group, cwd set to the workspace)
- **The permission engine** follows opencode's model:
  - Each tool key is allow, ask or deny.
  - Keys: read, edit (covers write and edit), glob, grep, bash, external directory, MCP by tool-name pattern, and doom loop.
  - Pattern maps work for bash and edit, and the last match wins.
- **The committed default is autonomous:** everything is allowed except:
  - a bash denylist: `sudo`, `su`, `rm -rf` of `/` or `~`, `git push`, piping curl/wget into a shell, `mkfs`, `dd`, shutdown/reboot, and a recursive 777 chmod of `/`
  - reading `.env*`
  - any external directory
- **Confinement:** paths are canonicalised, symlinks included, and must stay under the workspace. The tool-output spill folder is always allowed.
- **Limits:** bash times out after 120 seconds by default (at most 600) and kills its process group. Output is capped at 30 KB.
- **Doom loop:** three identical consecutive tool calls inject a warning; a fifth ends the turn with an error.
- **Checkpoint:** before a turn's first mutating call in a git workspace, a stash-style snapshot ref is recorded. Undo restores it and removes the files the turn created.
- **Redaction:** the API key value is redacted everywhere and removed from the environment of bash subprocesses.
- **Blocked calls:** a blocked call returns a "Blocked by guardrail: <rule>" error to the model and sets the blocked flag on the event.
- **Ask rules** raise an approval required action offering Allow once, Allow for session (in memory) or Deny.

### MCP

- **Transports:** stdio for local servers and streamable HTTP for remote ones, via the official Rust MCP SDK pinned to an exact version. The legacy SSE transport isn't supported.
- **Operations:** tool listing pages with a guard against a repeated cursor.
- **OAuth:**
  - The browser opens with the platform opener.
  - A random localhost port receives the callback.
  - An MCP-auth-required event shows the URL in the TUI.
  - Tokens are handed back to core, which persists them.
- **Lazy connection:** servers are connected only when preloaded or when a deferred call targets them.

### Configuration

- **TOML, three layers**, where later wins:
  1. the committed default, embedded in the binary
  2. the user config directory
  3. the workspace's own harness folder
- **Sections:**
  - per-provider base URL, model, temperature and seed
  - agent: instructions, max iterations, max turn minutes, subagents on/off
  - tools: bash timeout, output cap
  - permission
  - skills paths
  - MCP servers: local with command and env, or remote with URL, headers and an OAuth flag
- **Secrets never live in config.** Environment references are expanded in MCP headers and env.
- **One agent** in v1.

### Persistence

- **The database:** one SQLite file in the platform data directory, in WAL mode.
- **Migrations:** embedded SQL, applied in order and tracked by the SQLite user version.
- **Tables:**
  - **session:** title, workspace, provider, model, timestamps
  - **turn:** sequence, input, state, error, usage, timestamps
  - **event:** durable events only
  - **thread:** context snapshot upserted after every LLM or tool step, plus parent and usage
  - **MCP OAuth token**
  - **setting:** for example, the last provider
  - **checkpoint**
- **Resume** continues a session from its root thread's last context snapshot. A turn that was waiting on a required action when the process died isn't resumed at that point.

### TUI

- **Startup:** the modal popup for provider (last choice preselected) and, when needed, the workspace path.
- **Main screen:**
  - the transcript
  - a sidebar that toggles and auto-hides below 100 columns
  - a 3–8 row multi-line input with bracketed paste
  - a status line with spinner and key hints
- **Transcript items:**
  - user messages
  - streamed markdown
  - collapsible tool cards: diffs are syntax-highlighted; bash cards show the last 40 lines and the exit code
  - subagent cards
  - compaction notices
  - error and blocked badges
  - the summary card
- **Modals:** approval, question (options plus free text) and MCP auth.
- **Slash commands:** new, sessions, provider, undo, help, quit.
- **Rendering:** at most about 30 frames per second, and deltas append without re-rendering the whole transcript.

### Headless run

- **Arguments:** provider, optional workspace, a JSON-lines flag, and the task.
- **Answers:** questions are auto-answered with "No user available; proceed with your best judgement", and ask rules resolve to deny.
- **Exit codes:** 0 for done, 1 for error, 2 for cancelled or limit reached.

### Build and Makefile

- **Toolchain:** Rust 1.95, pinned by a toolchain file.
- **Dependencies** are pinned per the crate research. The notable choices: rustls with the `ring` provider for faster builds, and bundled SQLite.
- **Targets:**
  - **setup:** checks for a C compiler, installs rustup when cargo is missing, then does a locked release build.
  - **run:** launches the built binary with `AI_API_KEY` and `WORKSPACE` passed through.
  - **test:** runs the offline suite, plus the live smoke when the key is set.
  - **clean:** removes build artefacts.

### Conventions

- **Errors:** anyhow, plus typed errors only where a caller branches on them.
- **No `unwrap`** outside tests.
- **Tests** live in each package's top-level tests folder.
- **Checks:** fmt and clippy with warnings denied.

### Cutover

- **Delete** the TypeScript packages, the Python SDK, the Helm chart, the Docker files, pnpm/TS/lint tooling, changesets, the OpenAPI copies and the TS scripts.
- **Replace** CI with one Rust workflow on Linux and macOS.
- **Rewrite** the README and root agent conventions for Rust, and delete the nested TS convention files.
- After cutover the `rust/` folder moved to the repository root.

## Testing Decisions

- **A good test** drives the system from the outside, through the same interface a user or surface uses, and asserts on observable behaviour: the events emitted, the files in the workspace, the rows a later session reads back, the pixels in the terminal buffer. Tests never assert on private functions, internal state layouts or call sequences. That way the loop can be restructured freely as long as the behaviour holds.

- **Seam 1 (primary): the core surface.** Tests open a harness, start turns, read the event stream, reply to required actions and cancel. The environment around it:
  - **The model:** a small in-test fake OpenAI-compatible HTTP server serves queued SSE responses recorded in DeepSeek's and Qwen's real formats. The provider base URL points at it, so the real LLM client is exercised end to end, and no test-only provider exists in the binary.
  - **The workspace:** a temporary git repository.
  - **MCP:** a fixture stdio MCP server.
  - **The database:** a temporary SQLite file.

  Almost all behaviour is proven here:
  - plain answers, and read → edit → bash
  - approval allow and deny
  - questions
  - two parallel subagents, and the five-thread limit
  - compaction firing, and large-output spill
  - iteration limit, `length` finish, cancel mid-turn
  - guardrail blocks: outside path, symlink escape, denylisted command, `.env` read
  - doom loop, and checkpoint plus undo
  - redaction of the key
  - deferred MCP calls
  - session resume from the database
  - streamed-chunk quirks such as Qwen's empty tool-call ids
  - a golden snapshot of the system prompt

- **Seam 2: the TUI.** Tests feed a sequence of events into the TUI's app state and render with ratatui's test backend, asserting on the buffer:
  - the provider popup
  - a transcript with a collapsed and an expanded tool card
  - the approval modal
  - the summary card

- **Seam 3: the process boundary.** The Makefile and binary are verified by:
  - the live smoke test: a headless run against a fixture repository with one failing test, which must pass afterwards; only when `AI_API_KEY` is set
  - the clean-environment check: a fresh Linux container and a fresh macOS user

- **Permitted exception: one table-driven test of the permission pattern matcher** in the tools package, because the pattern combinatorics are too many to cover economically through seam 1.

- **Prior art in this repo:** TrueForge's orchestration tests (plain loop, tools round trip, approval allow and deny), turn-stream terminal-state tests, open-tool-call closer tests, remote MCP tests and compaction threshold tests describe the behaviours to reproduce. They are the reading reference, not code to port. TrueForge never tested the large-response algorithm, the compaction flow, the iteration limit, the `length` finish or the five-thread scheduling, so those get new scenarios here.

- **Every implementation ticket** is done only when its named tests pass, along with `cargo fmt --check` and clippy with warnings denied. `make test` runs the whole offline suite.

## Out of Scope

- **Hosted surfaces:** the HTTP API, the OpenAPI spec, and the generated TypeScript and Python SDKs.
- **React packages:** the embeddable UI SDK, the assistant-ui runtime and the web frontend.
- **Multi-tenant infrastructure:**
  - authentication (API key, OIDC, TrueFoundry control plane)
  - tenants and session sharing
  - Postgres, Redis and executor peering
  - Helm and Docker
- **Integrations:** the TrueFoundry enterprise integration.
- **Execution features:** schedules and the controller; sandboxes (local, Daytona, TrueFoundry) and Code Mode; web search.
- **UI features:** OpenUI generative UI, metrics dashboards, the Monaco editor (replaced by `$EDITOR`), and images or any non-text modality.
- **Provider modes:** thinking/reasoning mode.
- **Platforms:** Windows.
- **Data:** importing existing TrueForge data.
- **Other providers:** any LLM provider other than DeepSeek and Qwen, provider auto-detection, and Qwen Coding Plan keys.
- **Agents:** named or multiple agents beyond the single configured agent.

## Further Notes

- **Deadline:** the brief's deadline is about 2026-10-04. The work is sized so the leaf packages build in parallel on day 2, the loop on day 3, the built-ins and TUI detail on day 4, headless and integration on day 5, cutover on day 6, and clean-environment verification on day 7.
- **Background:** the planning map, its resolved decision tickets, and a detailed design reference with TrueForge source pointers, constants and prompt locations live alongside this spec under the `rust-tui-migration` planning folder.
- **Research findings** live in `docs/research/`: provider API behaviour, Rust crate choices and the core-loop inventory.
- **Glossary and decisions:** use the domain vocabulary in the glossary (TUI, headless run, agent, session, turn, thread, required action, provider, workspace, guardrail, checkpoint, evaluation issue). ADR 0001 explains why the fork drops the hosted product; ADR 0002 explains why paused turns wait in place.
- **The providers' own limits:** DeepSeek offers no seed, so perfect reproducibility isn't possible there. The committed temperature, the model id and the recorded usage are the controls the brief asks for.
- **Unverified API details:** the provider research was done without a live key. Implementers should confirm streaming-chunk shapes against a real key early, in the LLM client ticket, and update the recorded fixtures if they differ.
