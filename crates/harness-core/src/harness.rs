use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail, ensure};
use harness_llm::{
    ChatClient, ChatEvent, ClientConfig, Completion, Dialect, Message, Role, ToolCallResult,
    ToolDefinition, Usage,
};
use harness_store::{
    FinishTurn, NewCheckpoint, NewEvent, NewSession, NewTurn, Session, Store, UpsertThread,
};
use harness_tools::{AskOverrides, AskRequest, ToolResult};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use ulid::{Generator, Ulid};

use crate::builtins::{Builtin, MAX_PARALLEL_SUB_AGENTS, QuestionRequest, SubAgentRequest};
use crate::changes::{self, TurnChanges, WorkspaceChanges};
use crate::checkpoint;
use crate::compaction;
use crate::config::{Config, Provider, ProviderOption};
use crate::event::{Event, EventKind, ModelOutput, TurnState};
use crate::instructions::{SystemPromptInput, system_prompt};
use crate::mcp::{McpAuthNotifier, McpRegistry, McpServerStatus};
use crate::safety::{DoomLoop, DoomLoopAction, TurnLimits};
use crate::skills::load_skills;
use crate::tools::{ToolInvocation, TurnTools};

/// How a required action was resolved; see [`TurnHandle::reply`]. On the wire it is `"allow"`,
/// `"allow_for_session"`, `"deny"` or `{"text": "..."}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Approval: run the call once, without remembering the rule.
    Allow,
    /// Approval: run the call, and skip the prompt for the same rule for the rest of the
    /// session.
    AllowForSession,
    /// Approval: don't run the call. Also a question's fallback when it can't be answered.
    Deny,
    /// A question's answer: the chosen option's text, or free text.
    Text(String),
}

/// A required action's pending reply, keyed by its `action_id`.
type PendingActions = Arc<Mutex<HashMap<String, oneshot::Sender<Answer>>>>;

/// Host tool names whose call mutates the workspace, so the turn's first one triggers a
/// checkpoint. Matches `harness-tui`'s `ToolCallItem::expandable`.
const MUTATING_TOOLS: [&str; 3] = ["write_file", "edit", "bash"];

/// The setting key that stores the provider last chosen, read back to preselect the startup
/// popup and updated whenever `/provider` switches a session.
pub const LAST_PROVIDER_SETTING: &str = "last_provider";

/// The provider last chosen, if any: what a startup popup preselects and what a new harness
/// defaults to. A missing, unreadable or unknown stored value counts as none.
pub fn last_provider(store: &Store) -> Option<Provider> {
    store
        .get_setting(LAST_PROVIDER_SETTING)
        .ok()
        .flatten()?
        .parse()
        .ok()
}

/// A session's title is the first turn's input, truncated; longer inputs are cut here.
const MAX_SESSION_TITLE_LEN: usize = 50;

pub struct HarnessOptions {
    pub config: Config,
    pub store: Store,
    pub provider: Provider,
    /// The value of `AI_API_KEY`; see [`crate::api_key_from_env`].
    pub api_key: String,
    pub workspace: PathBuf,
    /// A headless run has no one to answer a required action: questions are auto-answered and
    /// `ask` rules resolve to deny, both without waiting.
    pub headless: bool,
}

/// The in-process agent harness. Both surfaces drive it only through `start_turn` and the handle.
pub struct Harness {
    inner: Arc<Inner>,
}

struct Inner {
    store: Arc<Store>,
    config: Config,
    /// The provider a brand-new session starts with; each turn afterwards reads the session's
    /// own `provider` column, which `/provider` can change independently.
    provider: Provider,
    api_key: String,
    workspace: PathBuf,
    redactor: Redactor,
    /// Lives as long as the harness, so a server connected in one turn is reused by the next.
    mcp: Arc<McpRegistry>,
    /// `ask` rules allowed "for session"; lives as long as the harness, across every turn.
    ask_overrides: Arc<AskOverrides>,
    headless: bool,
}

pub struct TurnHandle {
    pub events: mpsc::Receiver<Event>,
    cancel: CancellationToken,
    pending: PendingActions,
}

impl TurnHandle {
    /// Stops the running turn; it ends with `turn.done{state: cancelled}`. Every required
    /// action currently waiting resolves as denied (ADR 0002): each one is independently
    /// selecting on this same token inside [`TurnRunner::wait_for_answer`].
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Resolves a required action raised as `tool.approval_required` or `tool.response_required`.
    /// A reply to an id that already resolved (answered twice, or after cancel) is a no-op.
    pub fn reply(&self, action_id: &str, answer: Answer) {
        let sender = {
            let mut pending = lock(&self.pending);
            pending.remove(action_id)
        };
        if let Some(sender) = sender {
            let _ = sender.send(answer);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One turn of a replayed session: what was asked, how it ended, and its durable events.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TurnRecord {
    pub turn_id: String,
    /// The turn's 1-based position in its session.
    pub seq: i64,
    pub input: String,
    /// `None` for a turn the database still shows as running: the process stopped before it
    /// finished, so no `turn.done` was ever written (an interrupted turn).
    pub state: Option<TurnState>,
    pub error: Option<String>,
    /// The usage of the turn's last model call, as stored when it finished.
    pub usage: Option<Usage>,
    /// In the order they were written, which is the order the live stream delivered them.
    pub events: Vec<Event>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoOutcome {
    Restored,
    NothingToUndo,
}

/// What [`Harness::undo`] did. It serializes as `{"outcome": ...}` plus an `event` field only
/// when the workspace was restored.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UndoReport {
    pub outcome: UndoOutcome,
    /// The `turn.undone` event just written to the undone turn's log: `Some` exactly when the
    /// outcome is `Restored`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<Event>,
}

impl Harness {
    pub fn open(options: HarnessOptions) -> Result<Self> {
        // Built once, purely to fail fast on a broken config and install the rustls crypto
        // provider early; each turn builds its own client for the session's current provider.
        let provider_config = options.config.provider(options.provider);
        let _ = ChatClient::new(ClientConfig {
            dialect: client_dialect(options.provider),
            base_url: provider_config.base_url.clone(),
            api_key: options.api_key.clone(),
            model: provider_config.model.clone(),
            temperature: provider_config.temperature,
            seed: provider_config.seed,
            max_tokens: provider_config.max_output,
        })?;
        let store = Arc::new(options.store);
        let mcp = McpRegistry::new(
            &options.config.mcp,
            options.config.permission.clone(),
            options.workspace.clone(),
            Arc::new(StoreOAuthTokens(Arc::clone(&store))),
        );
        Ok(Self {
            inner: Arc::new(Inner {
                mcp: Arc::new(mcp),
                store,
                config: options.config,
                provider: options.provider,
                api_key: options.api_key.clone(),
                workspace: options.workspace,
                redactor: Redactor::new(&options.api_key),
                ask_overrides: Arc::new(AskOverrides::new()),
                headless: options.headless,
            }),
        })
    }

    /// Records a new session for the harness's workspace and provider and returns its id.
    pub fn create_session(&self) -> Result<String> {
        let inner = &self.inner;
        let id = new_id();
        inner.store.insert_session(&NewSession {
            id: &id,
            workspace: &inner.workspace.to_string_lossy(),
            provider: inner.provider.as_str(),
            model: &inner.config.provider(inner.provider).model,
        })?;
        Ok(id)
    }

    /// All sessions across every workspace this database has seen, most recently updated first,
    /// for `/sessions` to list.
    pub fn sessions(&self) -> Result<Vec<Session>> {
        self.inner.store.sessions()
    }

    /// A single session by id, e.g. to read back its provider when resuming it.
    pub fn session(&self, session_id: &str) -> Result<Option<Session>> {
        self.inner.store.session(session_id)
    }

    /// The workspace's uncommitted changes against `HEAD`, plus untracked files, each with its
    /// patch. `git: false` outside a git repository. It runs git and waits, so an async caller
    /// runs it on a blocking thread.
    pub fn workspace_changes(&self) -> Result<WorkspaceChanges> {
        changes::workspace_changes(&self.inner.workspace)
    }

    /// What the session's last turn changed, against the checkpoint taken before its first
    /// change: edits made before the turn are left out and `bash` edits are in. `checkpoint:
    /// false` when there is no last turn or it has no checkpoint. After an undo the files are
    /// back at the checkpoint, so the list is empty. Blocks on git like [`Self::workspace_changes`].
    pub fn turn_changes(&self, session_id: &str) -> Result<TurnChanges> {
        let inner = &self.inner;
        let none = || TurnChanges {
            checkpoint: false,
            files: Vec::new(),
        };
        let Some(last_turn) = inner.store.turns(session_id)?.pop() else {
            return Ok(none());
        };
        let Some(row) = inner.store.checkpoint(&last_turn.id)? else {
            return Ok(none());
        };
        Ok(TurnChanges {
            checkpoint: true,
            files: changes::changes_since_checkpoint(&inner.workspace, &row)?,
        })
    }

    /// A session's turns in order, each with the durable events it wrote. Streamed-only events
    /// (every `model.message.delta`, and the start marker of each `model.message`) were never
    /// stored and are absent; a message's text is its final `model.message`. Anything stored
    /// that cannot be read back, or that disagrees with its own row, fails the whole call
    /// instead of being skipped, so a partial history is never shown as complete.
    pub fn transcript(&self, session_id: &str) -> Result<Vec<TurnRecord>> {
        let store = &self.inner.store;
        if store.session(session_id)?.is_none() {
            bail!("session {session_id} not found");
        }
        store
            .turns(session_id)?
            .into_iter()
            .map(|turn| turn_record(store, turn))
            .collect()
    }

    /// The providers a session can run, in the picker's order, with the model each runs and where
    /// the API key would be sent. Fails when a configured `base_url` is not a usable URL.
    pub fn providers(&self) -> Result<Vec<ProviderOption>> {
        self.inner.config.provider_options()
    }

    /// Renames a session through the store's single title writer. The title is redacted, then
    /// trimmed and cut to the length an automatic title gets, and an empty result is rejected.
    /// A later turn keeps it, because only a session with no title is given an automatic one.
    pub fn rename_session(&self, session_id: &str, title: &str) -> Result<()> {
        let inner = &self.inner;
        if inner.store.session(session_id)?.is_none() {
            bail!("session {session_id} not found");
        }
        // Redacted before it is cut, so the cut cannot leave the front of a key showing.
        let redacted = inner.redactor.apply(title);
        let title = cut_title(redacted.trim());
        let title = title.trim_end();
        if title.is_empty() {
            bail!("a session title can't be empty");
        }
        inner.store.set_session_title(session_id, title)
    }

    /// Switches a session's provider (`/provider`) and remembers the choice as the next
    /// startup's preselection.
    pub fn switch_provider(&self, session_id: &str, provider: Provider) -> Result<()> {
        let inner = &self.inner;
        let model = &inner.config.provider(provider).model;
        inner
            .store
            .set_session_provider(session_id, provider.as_str(), model)?;
        inner
            .store
            .set_setting(LAST_PROVIDER_SETTING, provider.as_str())?;
        Ok(())
    }

    /// Each configured MCP server's connection state, in name order.
    pub fn mcp_status(&self) -> Vec<McpServerStatus> {
        self.inner.mcp.status()
    }

    /// The context size at which a thread on `provider` is compacted, e.g. for a usage bar.
    pub fn compaction_threshold(&self, provider: Provider) -> u64 {
        compaction::threshold(self.inner.config.provider(provider))
    }

    /// `/undo`: restores the workspace to the checkpoint taken before the session's last turn's
    /// first mutating call, removes the files that turn created, and records `turn.undone` in
    /// that turn's log. `NothingToUndo` when there is no last turn, when that turn was already
    /// undone (its checkpoint is stale, and restoring it again would discard the work done
    /// since), or when it never mutated the workspace (nothing was checkpointed).
    // ponytail: the already-undone check and the marker write are not atomic, so two overlapping
    // undos of one session could both restore; make it one transaction if undo ever overlaps.
    pub fn undo(&self, session_id: &str) -> Result<UndoReport> {
        let inner = &self.inner;
        let nothing = || UndoReport {
            outcome: UndoOutcome::NothingToUndo,
            event: None,
        };
        let Some(last_turn) = inner.store.turns(session_id)?.pop() else {
            return Ok(nothing());
        };
        let marker = EventKind::TurnUndone {
            turn_id: last_turn.id.clone(),
        };
        let already_undone = inner
            .store
            .events(&last_turn.id)?
            .iter()
            .any(|row| row.kind == marker.name());
        if already_undone {
            return Ok(nothing());
        }
        let Some(row) = inner.store.checkpoint(&last_turn.id)? else {
            return Ok(nothing());
        };
        checkpoint::restore(&inner.workspace, &row)?;

        // Written here rather than by a turn runner: the turn that owns this event ended long
        // ago. Restoring comes first, so a failure never marks a turn undone that was not.
        let event = Event {
            id: new_id(),
            kind: marker,
        };
        inner
            .store
            .insert_event(&NewEvent {
                id: &event.id,
                turn_id: &last_turn.id,
                thread_id: event.kind.thread_id(),
                kind: event.kind.name(),
                payload_json: &serde_json::to_string(&event)?,
            })
            .context("the workspace was restored but the undo could not be recorded")?;
        Ok(UndoReport {
            outcome: UndoOutcome::Restored,
            event: Some(event),
        })
    }

    /// Starts a turn in the background. The stream always ends with `turn.done`.
    pub fn start_turn(&self, session_id: &str, input: String) -> TurnHandle {
        let (tx, events) = mpsc::channel(256);
        let cancel = CancellationToken::new();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let turn = TurnRunner {
            inner: Arc::clone(&self.inner),
            turn_id: new_id(),
            tx,
            cancel: cancel.clone(),
            pending: Arc::clone(&pending),
        };
        tokio::spawn(turn.run(session_id.to_string(), input));
        TurnHandle {
            events,
            cancel,
            pending,
        }
    }
}

/// The only writer to the store: each durable event is persisted, then forwarded. Cloned into
/// each subagent task, so every thread of the turn writes through the same channel.
#[derive(Clone)]
struct TurnRunner {
    inner: Arc<Inner>,
    turn_id: String,
    tx: mpsc::Sender<Event>,
    cancel: CancellationToken,
    pending: PendingActions,
}

impl TurnRunner {
    async fn run(self, session_id: String, input: String) {
        let mut usage = None;
        let (state, error) = match self.execute(&session_id, &input, &mut usage).await {
            Ok(state) => (state, None),
            Err(e) => (
                TurnState::Error,
                Some(self.inner.redactor.apply(&format!("{e:#}"))),
            ),
        };
        // The turn row may be missing if the failure happened before it was written; the
        // terminal event still reaches the consumer.
        let usage_json = usage.and_then(|u: Usage| serde_json::to_string(&u).ok());
        let _ = self.inner.store.finish_turn(&FinishTurn {
            id: &self.turn_id,
            state: state.as_str(),
            error: error.as_deref(),
            usage_json: usage_json.as_deref(),
        });
        let done = EventKind::TurnDone { state, error };
        if self.emit(done.clone()).await.is_err() {
            let _ = self
                .tx
                .send(Event {
                    id: new_id(),
                    kind: done,
                })
                .await;
        }
    }

    async fn execute(
        &self,
        session_id: &str,
        input: &str,
        usage: &mut Option<Usage>,
    ) -> Result<TurnState> {
        let inner = &self.inner;
        inner.store.insert_turn(&NewTurn {
            id: &self.turn_id,
            session_id,
            input: &inner.redactor.apply(input),
        })?;
        let session = inner
            .store
            .session(session_id)?
            .ok_or_else(|| anyhow!("session {session_id} not found"))?;
        // ponytail: the check and the write are not atomic, so a rename landing between them is
        // overwritten; make it one conditional UPDATE if turns ever overlap within a session.
        if session.title.is_none() {
            inner
                .store
                .set_session_title(session_id, &inner.redactor.apply(&session_title(input)))?;
        }
        self.emit(EventKind::TurnCreated {
            session_id: session_id.to_string(),
            turn_id: self.turn_id.clone(),
        })
        .await?;

        let provider = Provider::from_str(&session.provider)?;
        let provider_config = inner.config.provider(provider);
        let client = ChatClient::new(ClientConfig {
            dialect: client_dialect(provider),
            base_url: provider_config.base_url.clone(),
            api_key: inner.api_key.clone(),
            model: provider_config.model.clone(),
            temperature: provider_config.temperature,
            seed: provider_config.seed,
            max_tokens: provider_config.max_output,
        })?;

        // The session's root thread carries the conversation so far (never the system
        // message, which is rebuilt fresh below); a session with no thread yet is starting.
        let existing_thread = inner.store.root_thread(session_id)?;
        let thread_id = existing_thread
            .as_ref()
            .map(|t| t.id.clone())
            .unwrap_or_else(new_id);

        let agents_md = std::fs::read_to_string(inner.workspace.join("AGENTS.md")).ok();
        let loaded_skills = load_skills(&inner.config.skills.paths, &inner.workspace);
        for warning in &loaded_skills.warnings {
            eprintln!("warning: {warning}");
        }
        let prompt = system_prompt(SystemPromptInput {
            workspace: &inner.workspace,
            config_instructions: &inner.config.agent.instructions,
            agents_md: agents_md.as_deref(),
            skills: &loaded_skills.skills,
            subagents_enabled: inner.config.agent.subagents,
            deferred_mcp_servers: &inner.mcp.deferred_servers(),
            root: true,
        });
        // A subagent never gets the user instructions or the subagent section: it can't spawn.
        let child_prompt = system_prompt(SystemPromptInput {
            workspace: &inner.workspace,
            config_instructions: "",
            agents_md: None,
            skills: &loaded_skills.skills,
            subagents_enabled: false,
            deferred_mcp_servers: &inner.mcp.deferred_servers(),
            root: false,
        });
        let history: Vec<Message> = existing_thread
            .as_ref()
            .and_then(|t| serde_json::from_str(&t.context_json).ok())
            .unwrap_or_default();
        let mut messages = Vec::with_capacity(history.len() + 2);
        messages.push(text_message(Role::System, prompt));
        messages.extend(history);
        messages.push(text_message(Role::User, input.to_string()));

        let stored_usage: Option<Usage> = existing_thread
            .as_ref()
            .and_then(|t| t.context_usage_json.as_deref())
            .and_then(|json| serde_json::from_str(json).ok());
        let builtin_definitions = Builtin::definitions(true, &inner.config.agent);
        // Starts any preload MCP server not yet connected, which can take a while.
        let tools = tokio::select! {
            biased;
            () = self.cancel.cancelled() => return Ok(TurnState::Cancelled),
            tools = self.with_mcp_auth_watch(&thread_id, |notifier| TurnTools::build(
                Arc::clone(&inner.mcp),
                inner.workspace.clone(),
                inner.config.tools.clone(),
                inner.config.permission.clone(),
                Arc::clone(&inner.ask_overrides),
                &builtin_definitions,
                notifier,
            )) => {
                Arc::new(tools)
            }
        };
        let shared = Arc::new(TurnShared {
            session_id: session_id.to_string(),
            client,
            child_prompt,
            tools,
            threshold: compaction::threshold(provider_config),
            limits: TurnLimits::new(&inner.config.agent),
            checkpoint_taken: AtomicBool::new(false),
        });
        let root = ThreadInfo {
            id: thread_id,
            parent: None,
            title: None,
        };
        Ok(
            match self
                .run_thread(&shared, &root, &mut messages, usage, stored_usage)
                .await?
            {
                ThreadEnd::Done(_) => TurnState::Done,
                ThreadEnd::Cancelled => TurnState::Cancelled,
            },
        )
    }

    /// One thread's loop, root or subagent: stream an answer, and if it asked for tool calls,
    /// run them and go around again. A round with no tool calls ends the thread with its text.
    async fn run_thread(
        &self,
        shared: &Arc<TurnShared>,
        thread: &ThreadInfo,
        messages: &mut Vec<Message>,
        usage: &mut Option<Usage>,
        stored_usage: Option<Usage>,
    ) -> Result<ThreadEnd> {
        let mut tools: Vec<ToolDefinition> = shared.tools.definitions.clone();
        tools.extend(Builtin::definitions(
            thread.is_root(),
            &self.inner.config.agent,
        ));
        let thread_id = &thread.id;
        // The stored usage already counts history; estimate only new input on resume.
        let uncounted = if stored_usage.is_some() {
            &messages[messages.len() - 1..]
        } else {
            &messages[..]
        };
        let mut context_usage = stored_usage.unwrap_or_default();
        context_usage.prompt_tokens += compaction::estimate_tokens(uncounted);
        let mut doom_loop = DoomLoop::default();

        loop {
            shared.limits.check()?;
            if compaction::is_due(&context_usage, shared.threshold) {
                let request = compaction::summary_request(&messages[1..]);
                let Some(summary) = self.complete(&shared.client, &request).await? else {
                    return Ok(ThreadEnd::Cancelled);
                };
                let compacted = compaction::compacted_context(summary.content);
                let summary_tokens = summary.usage.map_or_else(
                    || compaction::estimate_tokens(&compacted[..1]),
                    |u| u.completion_tokens,
                );
                messages.truncate(1);
                messages.extend(compacted);
                context_usage = compaction::compacted_usage(summary_tokens);
                self.emit(EventKind::ContextCompacted {
                    thread_id: thread_id.clone(),
                    usage: context_usage.clone(),
                })
                .await?;
                self.snapshot_thread(&shared.session_id, thread, messages, &context_usage)?;
            }

            let mut stream = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Ok(ThreadEnd::Cancelled),
                result = shared.client.stream(messages, &tools) => result?,
            };
            let message_id = new_id();
            self.forward(EventKind::ModelMessage {
                thread_id: thread_id.clone(),
                message_id: message_id.clone(),
                message: None,
            })
            .await;
            let completion = loop {
                let chat_event = tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => return Ok(ThreadEnd::Cancelled),
                    chat_event = stream.next() => chat_event?,
                };
                let Some(chat_event) = chat_event else {
                    bail!("provider stream ended without a final message");
                };
                match chat_event {
                    ChatEvent::Delta(content) => {
                        self.forward(EventKind::ModelMessageDelta {
                            thread_id: thread_id.clone(),
                            message_id: message_id.clone(),
                            content,
                        })
                        .await;
                    }
                    ChatEvent::Done(completion) => {
                        usage.clone_from(&completion.usage);
                        let finish_reason = completion.finish_reason.clone();
                        self.emit(EventKind::ModelMessage {
                            thread_id: thread_id.clone(),
                            message_id: message_id.clone(),
                            message: Some(ModelOutput {
                                content: completion.content.clone(),
                                finish_reason: completion.finish_reason.clone(),
                                usage: completion.usage.clone(),
                            }),
                        })
                        .await?;
                        if finish_reason == "length" {
                            bail!("max_tokens breached");
                        }
                        break completion;
                    }
                }
            };

            messages.push(Message {
                role: Role::Assistant,
                content: completion.content.clone(),
                tool_calls: completion
                    .tool_calls
                    .iter()
                    .map(|call| harness_llm::ToolCallRequest {
                        id: call.id.clone(),
                        kind: "function".to_string(),
                        function: harness_llm::FunctionCall {
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                        },
                    })
                    .collect(),
                tool_call_id: None,
            });
            match usage.as_ref() {
                Some(provider_usage) => context_usage.clone_from(provider_usage),
                None => {
                    context_usage.prompt_tokens +=
                        compaction::estimate_tokens(&messages[messages.len() - 1..]);
                }
            }
            // A snapshot after every LLM step, so a resumed session picks up mid-turn state
            // even if nothing runs after this (a plain answer, or the process stopping here).
            self.snapshot_thread(&shared.session_id, thread, messages, &context_usage)?;

            if completion.tool_calls.is_empty() {
                return Ok(ThreadEnd::Done(completion.content));
            }

            let mut doom_warning = None;
            for call in &completion.tool_calls {
                self.emit(EventKind::ToolCall {
                    thread_id: thread_id.clone(),
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .await?;
                match doom_loop.note(&call.name, &call.arguments) {
                    DoomLoopAction::None => {}
                    DoomLoopAction::Warn(message) => doom_warning = Some(message),
                    DoomLoopAction::End(message) => bail!(message),
                }
            }
            let results = self
                .with_mcp_auth_watch(&thread.id, |notifier| {
                    self.run_tool_calls(shared, thread, &completion.tool_calls, notifier)
                })
                .await?;
            if self.cancel.is_cancelled() {
                return Ok(ThreadEnd::Cancelled);
            }
            let mut batch: Vec<(String, ToolResult)> = completion
                .tool_calls
                .iter()
                .zip(results)
                .map(|(call, mut result)| {
                    // Spill files are durable, so redact before large-output handling writes one.
                    result.content = self.inner.redactor.apply(&result.content);
                    (self.inner.redactor.apply(&call.name), result)
                })
                .collect();
            let workspace = self.inner.workspace.clone();
            let subagents_enabled = self.inner.config.agent.subagents;
            let batch = tokio::task::spawn_blocking(move || {
                harness_tools::cap_large_outputs(&workspace, subagents_enabled, &mut batch);
                batch
            })
            .await
            .context("large tool output task panicked")?;

            let first_result = messages.len();
            for (call, (_, result)) in completion.tool_calls.iter().zip(batch) {
                // Redacted here, not just on persist, so the model never sees the key either.
                let content = self.inner.redactor.apply(&result.content);
                self.emit(EventKind::ToolResponse {
                    thread_id: thread_id.clone(),
                    tool_call_id: call.id.clone(),
                    content: content.clone(),
                    blocked: result.blocked,
                    diff: result.diff.map(|diff| self.inner.redactor.apply(&diff)),
                })
                .await?;
                messages.push(Message {
                    role: Role::Tool,
                    content,
                    tool_calls: Vec::new(),
                    tool_call_id: Some(call.id.clone()),
                });
            }
            if let Some(message) = doom_warning {
                messages.push(text_message(Role::User, message));
            }
            context_usage.prompt_tokens += compaction::estimate_tokens(&messages[first_result..]);
            // And again after the tool step, so the round trip is durable too.
            self.snapshot_thread(&shared.session_id, thread, messages, &context_usage)?;
        }
    }

    /// Runs one round's calls, returning results in the calls' order. Host tools run in
    /// parallel on blocking threads (an `ask` permission pauses just that call for approval);
    /// `create_sub_agent` calls become child threads, and this thread waits for all of them
    /// (only leaf threads run); `ask_question` calls are answered in sequence, since the TUI
    /// has only one modal slot at a time.
    async fn run_tool_calls(
        &self,
        shared: &Arc<TurnShared>,
        thread: &ThreadInfo,
        calls: &[ToolCallResult],
        mcp_auth: McpAuthNotifier,
    ) -> Result<Vec<ToolResult>> {
        self.checkpoint_if_first_mutation(shared, calls).await?;
        let mut results: Vec<Option<ToolResult>> = vec![None; calls.len()];
        let mut other = Vec::new();
        let mut children = Vec::new();
        let mut questions = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            match Builtin::find(&call.name, thread.is_root(), &self.inner.config.agent) {
                Some(Builtin::CreateSubAgent) => match SubAgentRequest::parse(&call.arguments) {
                    Ok(request) => children.push((index, request)),
                    Err(message) => results[index] = Some(ToolResult::error(message)),
                },
                Some(Builtin::AskQuestion) => match QuestionRequest::parse(&call.arguments) {
                    Ok(request) => questions.push((index, request)),
                    Err(message) => results[index] = Some(ToolResult::error(message)),
                },
                None => {
                    let tools = Arc::clone(&shared.tools);
                    let call = call.clone();
                    let runner = self.clone();
                    let thread_id = thread.id.clone();
                    let mcp_auth = mcp_auth.clone();
                    other.push((
                        index,
                        tokio::spawn(async move {
                            runner
                                .call_host_tool(&tools, &thread_id, &call, mcp_auth)
                                .await
                        }),
                    ));
                }
            }
        }

        if !children.is_empty() {
            let permits = Arc::new(Semaphore::new(MAX_PARALLEL_SUB_AGENTS));
            let mut running = JoinSet::new();
            for (index, request) in children {
                let child = self.clone().run_child(ChildSpec {
                    shared: Arc::clone(shared),
                    parent: thread.id.clone(),
                    request,
                    permits: Arc::clone(&permits),
                });
                running.spawn(async move { (index, child.await) });
            }
            while let Some(joined) = running.join_next().await {
                let (index, result) = joined.context("subagent task panicked")?;
                results[index] = Some(result);
            }
        }
        for (index, request) in questions {
            let answer = self
                .request_question(&thread.id, &request.question, &request.options)
                .await;
            let text = match answer {
                Answer::Text(text) => text,
                Answer::Allow | Answer::AllowForSession | Answer::Deny => {
                    "No user available; proceed with your best judgement".to_string()
                }
            };
            results[index] = Some(ToolResult::ok(text));
        }
        for (index, handle) in other {
            results[index] = Some(handle.await.context("tool call task panicked")?);
        }
        results
            .into_iter()
            .map(|result| result.ok_or_else(|| anyhow!("a tool call produced no result")))
            .collect()
    }

    /// Runs one host tool call, resolving an `ask` permission through a required action before
    /// retrying it: allow-once bypasses just this call, allow-for-session also remembers the
    /// rule on `tools` so later identical calls skip the prompt, and deny returns without
    /// running the call at all.
    async fn call_host_tool(
        &self,
        tools: &TurnTools,
        thread_id: &str,
        call: &ToolCallResult,
        mcp_auth: McpAuthNotifier,
    ) -> ToolResult {
        let invocation = || ToolInvocation {
            name: &call.name,
            arguments_json: &call.arguments,
            mcp_auth: mcp_auth.clone(),
            cancel: self.cancel.clone(),
        };
        let result = tools.call(invocation(), None).await;
        let Some(ask) = result.needs_approval.clone() else {
            return result;
        };
        match self.request_approval(thread_id, call, &ask).await {
            Answer::Deny | Answer::Text(_) => ToolResult::denied(&ask.rule),
            Answer::Allow => {
                tools
                    .call(invocation(), Some((ask.key.clone(), ask.pattern.clone())))
                    .await
            }
            Answer::AllowForSession => {
                tools.allow_for_session(&ask.key, &ask.pattern);
                tools.call(invocation(), None).await
            }
        }
    }

    /// Raises `tool.approval_required` and waits for the reply, or resolves to deny immediately
    /// in a headless run.
    async fn request_approval(
        &self,
        thread_id: &str,
        call: &ToolCallResult,
        ask: &AskRequest,
    ) -> Answer {
        let action_id = new_id();
        let _ = self
            .emit(EventKind::ToolApprovalRequired {
                action_id: action_id.clone(),
                thread_id: thread_id.to_string(),
                tool_call_id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
                rule: ask.rule.clone(),
            })
            .await;
        self.wait_for_answer(action_id, Answer::Deny).await
    }

    /// Raises `tool.response_required` and waits for the reply, or auto-answers immediately in a
    /// headless run.
    async fn request_question(
        &self,
        thread_id: &str,
        question: &str,
        options: &[String],
    ) -> Answer {
        let action_id = new_id();
        let _ = self
            .emit(EventKind::ToolResponseRequired {
                action_id: action_id.clone(),
                thread_id: thread_id.to_string(),
                question: question.to_string(),
                options: options.to_vec(),
            })
            .await;
        let default =
            Answer::Text("No user available; proceed with your best judgement".to_string());
        self.wait_for_answer(action_id, default).await
    }

    /// Parks on the action's reply, unless this is a headless run (no one will ever reply, so
    /// `headless_default` resolves it immediately) or the turn is cancelled (every pending
    /// action resolves to deny independently, per ADR 0002).
    async fn wait_for_answer(&self, action_id: String, headless_default: Answer) -> Answer {
        if self.inner.headless {
            return headless_default;
        }
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(action_id.clone(), tx);
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => {
                lock(&self.pending).remove(&action_id);
                Answer::Deny
            }
            answer = rx => answer.unwrap_or(Answer::Deny),
        }
    }

    /// Takes the turn's checkpoint the first time any thread is about to run a mutating host
    /// tool call, in a git workspace. A no-op every other time (including a non-git workspace,
    /// where `checkpoint::capture` returns `None`).
    async fn checkpoint_if_first_mutation(
        &self,
        shared: &Arc<TurnShared>,
        calls: &[ToolCallResult],
    ) -> Result<()> {
        let mutates = calls
            .iter()
            .any(|call| MUTATING_TOOLS.contains(&call.name.as_str()));
        if !mutates || shared.checkpoint_taken.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let inner = Arc::clone(&self.inner);
        let turn_id = self.turn_id.clone();
        let session_id = shared.session_id.clone();
        tokio::task::spawn_blocking(move || {
            let Some(snapshot) = checkpoint::capture(&inner.workspace, &turn_id) else {
                return;
            };
            let untracked_json = snapshot.untracked_json();
            let _ = inner.store.insert_checkpoint(&NewCheckpoint {
                turn_id: &turn_id,
                session_id: &session_id,
                head_sha: &snapshot.head_sha,
                stash_ref: snapshot.stash_ref.as_deref(),
                untracked_json: &untracked_json,
            });
        })
        .await
        .context("checkpoint task panicked")
    }

    /// A subagent thread as a boxed future: it waits for a free slot, runs its own loop, and
    /// resolves to the parent's tool result. Boxed because a thread's loop spawns these.
    fn run_child(self, spec: ChildSpec) -> Pin<Box<dyn Future<Output = ToolResult> + Send>> {
        Box::pin(async move {
            let Ok(_permit) = spec.permits.acquire_owned().await else {
                return ToolResult::error("subagent scheduler closed");
            };
            let thread = ThreadInfo {
                id: new_id(),
                parent: Some(spec.parent),
                title: Some(spec.request.name),
            };
            let outcome = self
                .run_child_thread(&spec.shared, &thread, spec.request.input)
                .await;
            let (result, error) = match outcome {
                Ok(text) => (ToolResult::ok(text), None),
                Err(e) => {
                    let error = self.inner.redactor.apply(&format!("{e:#}"));
                    (ToolResult::error(&error), Some(error))
                }
            };
            // A store failure here must not lose the parent's result; the child is over anyway.
            let _ = self
                .emit(EventKind::ThreadDone {
                    thread_id: thread.id,
                    error,
                })
                .await;
            result
        })
    }

    async fn run_child_thread(
        &self,
        shared: &Arc<TurnShared>,
        thread: &ThreadInfo,
        input: String,
    ) -> Result<String> {
        self.emit(EventKind::ThreadCreated {
            thread_id: thread.id.clone(),
            parent: thread.parent.clone().unwrap_or_default(),
            title: thread.title.clone().unwrap_or_default(),
        })
        .await?;
        let mut messages = vec![
            text_message(Role::System, shared.child_prompt.clone()),
            text_message(Role::User, input),
        ];
        let mut usage = None;
        match self
            .run_thread(shared, thread, &mut messages, &mut usage, None)
            .await?
        {
            ThreadEnd::Done(text) => Ok(text),
            ThreadEnd::Cancelled => bail!("cancelled"),
        }
    }

    /// Upserts the thread's snapshot from `messages`, excluding the leading system message
    /// (which is rebuilt fresh every turn, so it is never part of the persisted context).
    fn snapshot_thread(
        &self,
        session_id: &str,
        thread: &ThreadInfo,
        messages: &[Message],
        context_usage: &Usage,
    ) -> Result<()> {
        let history = &messages[1..];
        let redactor = &self.inner.redactor;
        let context_json = redactor.apply(&serde_json::to_string(history)?);
        let context_usage_json = serde_json::to_string(context_usage)?;
        let title = thread.title.as_deref().map(|t| redactor.apply(t));
        self.inner.store.upsert_thread(&UpsertThread {
            id: &thread.id,
            session_id,
            parent_id: thread.parent.as_deref(),
            title: title.as_deref(),
            context_json: &context_json,
            context_usage_json: Some(&context_usage_json),
        })?;
        Ok(())
    }

    /// A non-streaming call to the same model: the stream is drained without forwarding deltas.
    /// `None` when the turn is cancelled first.
    async fn complete(
        &self,
        client: &ChatClient,
        messages: &[Message],
    ) -> Result<Option<Completion>> {
        let mut stream = tokio::select! {
            biased;
            () = self.cancel.cancelled() => return Ok(None),
            result = client.stream(messages, &[]) => result?,
        };
        loop {
            let chat_event = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Ok(None),
                chat_event = stream.next() => chat_event?,
            };
            match chat_event {
                Some(ChatEvent::Done(completion)) => return Ok(Some(completion)),
                Some(ChatEvent::Delta(_)) => {}
                None => bail!("provider stream ended without a final message"),
            }
        }
    }

    /// Persists a durable event (redacted), then forwards it.
    async fn emit(&self, kind: EventKind) -> Result<()> {
        let json = self
            .inner
            .redactor
            .apply(&serde_json::to_string(&Event { id: new_id(), kind })?);
        let event: Event = serde_json::from_str(&json)?;
        self.inner.store.insert_event(&NewEvent {
            id: &event.id,
            turn_id: &self.turn_id,
            thread_id: event.kind.thread_id(),
            kind: event.kind.name(),
            payload_json: &json,
        })?;
        let _ = self.tx.send(event).await;
        Ok(())
    }

    /// Forwards a streamed-only event without persisting it.
    async fn forward(&self, kind: EventKind) {
        // A consumer that stopped listening doesn't stop the turn; it still runs to completion.
        let _ = self.tx.send(Event { id: new_id(), kind }).await;
    }

    /// Runs `work`, streaming `mcp.auth_required` for every OAuth login URL it reports while it
    /// runs (an OAuth login can take a while — the TUI needs the URL well before `work` finishes,
    /// not after).
    async fn with_mcp_auth_watch<T, F, Fut>(&self, thread_id: &str, work: F) -> T
    where
        F: FnOnce(McpAuthNotifier) -> Fut,
        Fut: Future<Output = T>,
    {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let watcher_self = self.clone();
        let watcher_thread_id = thread_id.to_string();
        let watcher = tokio::spawn(async move {
            while let Some((server, url)) = rx.recv().await {
                let _ = watcher_self
                    .emit(EventKind::McpAuthRequired {
                        thread_id: watcher_thread_id.clone(),
                        server,
                        url,
                    })
                    .await;
            }
        });
        let result = work(tx).await;
        let _ = watcher.await;
        result
    }
}

/// What every thread of one turn shares.
struct TurnShared {
    session_id: String,
    client: ChatClient,
    /// The system prompt each subagent thread starts with.
    child_prompt: String,
    tools: Arc<TurnTools>,
    threshold: u64,
    /// The turn's LLM-call/wall-clock budget, checked by every thread before each LLM call.
    limits: TurnLimits,
    /// Set by whichever thread's tool call is the turn's first mutating one.
    checkpoint_taken: AtomicBool,
}

/// A thread's identity: the root has no parent; a subagent names its parent and its title.
struct ThreadInfo {
    id: String,
    parent: Option<String>,
    title: Option<String>,
}

impl ThreadInfo {
    fn is_root(&self) -> bool {
        self.parent.is_none()
    }
}

enum ThreadEnd {
    /// The thread's final assistant text.
    Done(String),
    Cancelled,
}

struct ChildSpec {
    shared: Arc<TurnShared>,
    parent: String,
    request: SubAgentRequest,
    permits: Arc<Semaphore>,
}

fn text_message(role: Role, content: String) -> Message {
    Message {
        role,
        content,
        tool_calls: Vec::new(),
        tool_call_id: None,
    }
}

/// Backs `harness_mcp::OAuthTokenStore` with the harness's own database, so a remote MCP
/// server's OAuth token survives restarts. A local wrapper because neither `harness-mcp` nor
/// `harness-store` may depend on the other.
struct StoreOAuthTokens(Arc<Store>);

impl harness_mcp::OAuthTokenStore for StoreOAuthTokens {
    fn load(&self, server: &str) -> Result<Option<String>> {
        self.0.get_mcp_oauth_token(server)
    }

    fn save(&self, server: &str, credentials_json: &str) -> Result<()> {
        self.0.set_mcp_oauth_token(server, credentials_json)
    }

    fn clear(&self, server: &str) -> Result<()> {
        self.0.clear_mcp_oauth_token(server)
    }
}

/// Replaces the API key with `[REDACTED]` in persisted data and tool results. Public so a
/// surface that returns its own text, such as error messages, redacts with this one
/// implementation instead of writing another.
pub struct Redactor {
    /// The key as it appears inside a JSON string, which covers plain text too.
    needle: String,
}

impl Redactor {
    /// A redactor for `api_key`; an empty key redacts nothing.
    pub fn new(api_key: &str) -> Self {
        let quoted = serde_json::to_string(api_key).unwrap_or_default();
        Self {
            needle: quoted.trim_matches('"').to_string(),
        }
    }

    /// `text` with every occurrence of the key replaced.
    pub fn apply(&self, text: &str) -> String {
        if self.needle.is_empty() {
            return text.to_string();
        }
        text.replace(&self.needle, "[REDACTED]")
    }
}

fn client_dialect(provider: Provider) -> Dialect {
    match provider {
        Provider::DeepSeek => Dialect::DeepSeek,
        Provider::Qwen => Dialect::Qwen,
    }
}

/// A stored turn and its events as a [`TurnRecord`]; core owns this conversion so no caller
/// reads store rows.
fn turn_record(store: &Store, turn: harness_store::Turn) -> Result<TurnRecord> {
    let state = match turn.state.as_str() {
        "running" => None,
        other => Some(
            other
                .parse::<TurnState>()
                .with_context(|| format!("turn {}: unreadable state", turn.id))?,
        ),
    };
    let usage = turn
        .usage_json
        .as_deref()
        .map(serde_json::from_str::<Usage>)
        .transpose()
        .with_context(|| format!("turn {}: unreadable usage", turn.id))?;
    let events = store
        .events(&turn.id)?
        .into_iter()
        .map(|row| stored_event(&turn.id, row))
        .collect::<Result<Vec<_>>>()?;
    Ok(TurnRecord {
        turn_id: turn.id,
        seq: turn.seq,
        input: turn.input,
        state,
        error: turn.error,
        usage,
        events,
    })
}

/// A stored event row as an [`Event`]. The row repeats the event's id and kind in columns, so a
/// payload that disagrees with them is as corrupt as one that does not parse.
fn stored_event(turn_id: &str, row: harness_store::Event) -> Result<Event> {
    let event: Event = serde_json::from_str(&row.payload_json)
        .with_context(|| format!("turn {turn_id}: unreadable `{}` event {}", row.kind, row.id))?;
    ensure!(
        event.id == row.id && event.kind.name() == row.kind,
        "turn {turn_id}: event {} does not match its stored payload",
        row.id
    );
    Ok(event)
}

/// The first turn's input, trimmed and truncated, becomes the session's title. Later turns
/// don't call this: `execute` only titles a session that has no title yet.
fn session_title(input: &str) -> String {
    cut_title(input.lines().next().unwrap_or("").trim())
}

/// `text` cut to at most [`MAX_SESSION_TITLE_LEN`] characters.
fn cut_title(text: &str) -> String {
    match text.char_indices().nth(MAX_SESSION_TITLE_LEN) {
        Some((cut, _)) => text[..cut].to_string(),
        None => text.to_string(),
    }
}

/// Monotonic lowercase ULIDs, so ids sort in creation order.
pub(crate) fn new_id() -> String {
    static GENERATOR: LazyLock<Mutex<Generator>> = LazyLock::new(|| Mutex::new(Generator::new()));
    let ulid = match GENERATOR.lock() {
        Ok(mut generator) => generator
            .generate()
            .unwrap_or_else(|overflow| overflow.commit_overflow_increment()),
        Err(_) => Ulid::generate(),
    };
    ulid.to_string().to_lowercase()
}
