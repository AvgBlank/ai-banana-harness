# Banana Harness

A local-first, terminal-first coding-agent harness, forked from TrueForge: it runs the agent loop in-process against a workspace and is driven through a TUI, a desktop app, or headlessly.

## Language

### Surfaces

**TUI**:
The interactive terminal interface.

**Desktop app**:
The interactive windowed application for macOS, the second interactive interface. It is a thin client that drives the harness through a sidecar and holds no agent logic; its code lives in `gui/`.
_Avoid_: web UI, chat UI, Electron app

**Sidecar**:
The `banana-harness serve --stdio` process the desktop app launches and talks to in newline-delimited JSON over its standard input and output; it hosts one harness per open workspace.
_Avoid_: server, backend, daemon

**Headless run**:
A non-interactive execution of one task from the command line, with output suited to scripts and CI.
_Avoid_: exec mode, API mode

### Agent loop

**Agent**:
A named specification of model, instructions, MCP servers, skills and runtime settings that a session runs.
_Avoid_: bot, assistant

**Session**:
One conversation between a user and an agent, made of turns. The desktop app lists sessions grouped by workspace and says "session" everywhere in its interface.
_Avoid_: chat, thread

**Turn**:
One execution of the agent loop in response to user input, producing an ordered stream of events.
_Avoid_: run, request

**Interrupted turn**:
A turn that never reached `turn.done` because the process stopped while it ran; the session continues from its last saved context.
_Avoid_: failed turn, crashed turn

**Undone turn**:
A turn whose checkpoint was restored by undo, recorded by a durable `turn.undone` event after which it cannot be undone again.
_Avoid_: reverted turn, rolled-back turn

**Thread**:
One agent loop inside a turn: the main loop or a dynamic subagent's loop.
_Avoid_: subagent run

**Required action**:
A tool approval or a question that a running turn is waiting on the user to answer.
_Avoid_: prompt, interruption, pause

**Provider**:
The LLM service a session talks to: DeepSeek or Qwen, chosen at startup and switchable per session.
_Avoid_: backend, vendor

### Workspace

**Workspace**:
The repository directory the agent reads, edits and runs commands in; the agent's tools cannot reach outside it. The desktop app can have several open at once, each served by its own harness.
_Avoid_: sandbox, project dir, cwd

**Command runner**:
What runs a `bash` tool command: the host runner runs it on this machine in its own process group; the container runner runs it in a container made for that one command. Both stop a running command when the turn is cancelled or the command's timeout passes.
_Avoid_: executor, shell backend

**Container runner**:
The command runner that isolates a command in a container with the workspace mounted, no network and no API key. It isolates commands only; the harness and the file tools stay on the host, and it is not a sandbox for the agent (ADR 0004).
_Avoid_: sandbox, sandbox provider

**Workspace config**:
The workspace's own `.banana-harness/config.toml`, the last layer of configuration; it can start programs, change guardrails and change where the API key is sent.
_Avoid_: project config, local config

**Profile**:
The desktop app's local display name for this Mac, kept in its preferences file. It is not an account: it signs nothing in, is never sent to a provider or the sidecar, and is separate from the API key.
_Avoid_: account, user, login

**Restricted mode**:
A workspace opened without loading its workspace config; the agent still works in the workspace, under the default and user-level settings.
_Avoid_: safe mode, read-only mode

**Workspace trust**:
The user's recorded decision to load one exact workspace config, bound to that file's content so any change asks again. It is not a tool approval; that is a required action.
_Avoid_: approval, allowlist

**Guardrail**:
A rule the harness enforces on the agent's actions regardless of what the model asks for.
_Avoid_: safety filter, policy

**Checkpoint**:
A git snapshot of the workspace taken before a turn's first change, so that turn's changes can be undone.
_Avoid_: backup, savepoint

**Changes**:
The files in a workspace that differ from a baseline, with a patch each: for the last turn, from its checkpoint; for the workspace, from `HEAD` including untracked files.
_Avoid_: modified files, git status

### Evaluation

**Evaluation issue**:
The GitHub issue or test case the hackathon evaluator supplies to the running harness for it to resolve.
_Avoid_: task, prompt, ticket

### Retired

These terms belonged to the hosted multi-tenant product and have no meaning in this fork: **Tenant**, **Session owner**, **Shared session**, **Schedule**, **Controller**, **Executor peering**, **Sandbox provider**.
