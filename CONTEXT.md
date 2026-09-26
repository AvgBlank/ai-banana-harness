# Banana Harness

A local-first, terminal-first coding-agent harness, forked from TrueForge: it runs the agent loop in-process against a workspace and is driven through a TUI or headlessly.

## Language

### Surfaces

**TUI**:
The interactive terminal interface, and the only interactive interface of the harness.
_Avoid_: GUI, web UI, chat UI

**Headless run**:
A non-interactive execution of one task from the command line, with output suited to scripts and CI.
_Avoid_: exec mode, API mode

### Agent loop

**Agent**:
A named specification of model, instructions, MCP servers, skills and runtime settings that a session runs.
_Avoid_: bot, assistant

**Session**:
One conversation between a user and an agent, made of turns.
_Avoid_: chat, thread

**Turn**:
One execution of the agent loop in response to user input, producing an ordered stream of events.
_Avoid_: run, request

**Thread**:
One agent loop inside a turn: the main loop or a dynamic subagent's loop.
_Avoid_: subagent run

**Required action**:
A tool approval or a question that a running turn is waiting on the user to answer.
_Avoid_: prompt, interruption, pause

**Provider**:
The LLM service a session talks to: DeepSeek or Qwen, chosen at startup.
_Avoid_: backend, vendor

### Workspace

**Workspace**:
The repository directory the agent reads, edits and runs commands in; the agent's tools cannot reach outside it.
_Avoid_: sandbox, project dir, cwd

**Guardrail**:
A rule the harness enforces on the agent's actions regardless of what the model asks for.
_Avoid_: safety filter, policy

**Checkpoint**:
A git snapshot of the workspace taken before a turn's first change, so that turn's changes can be undone.
_Avoid_: backup, savepoint

### Evaluation

**Evaluation issue**:
The GitHub issue or test case the hackathon evaluator supplies to the running harness for it to resolve.
_Avoid_: task, prompt, ticket

### Retired

These terms belonged to the hosted multi-tenant product and have no meaning in this fork: **Tenant**, **Session owner**, **Shared session**, **Schedule**, **Controller**, **Executor peering**, **Sandbox provider**.
