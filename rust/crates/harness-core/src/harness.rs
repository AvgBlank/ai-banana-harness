use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use harness_llm::{
    ChatClient, ChatEvent, ClientConfig, Completion, Dialect, Message, Role, ToolCallResult,
    ToolDefinition, Usage,
};
use harness_store::{
    FinishTurn, NewCheckpoint, NewEvent, NewSession, NewTurn, Session, Store, UpsertThread,
};
use harness_tools::{AskOverrides, AskRequest, ToolResult};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use ulid::{Generator, Ulid};

use crate::builtins::{Builtin, MAX_PARALLEL_SUB_AGENTS, QuestionRequest, SubAgentRequest};
use crate::checkpoint;
use crate::compaction;
use crate::config::{Config, Provider};
use crate::event::{Event, EventKind, ModelOutput, TurnState};
use crate::instructions::{SystemPromptInput, system_prompt};
use crate::mcp::{McpRegistry, McpServerStatus};
use crate::safety::{DoomLoop, DoomLoopAction, TurnLimits};
use crate::skills::load_skills;
use crate::tools::{ToolInvocation, TurnTools};

/// How a required action was resolved; see [`TurnHandle::reply`].
#[derive(Clone, Debug)]
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
    store: Store,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndoOutcome {
    Restored,
    NothingToUndo,
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
        })?;
        let mcp = McpRegistry::new(
            &options.config.mcp,
            options.config.permission.clone(),
            options.workspace.clone(),
        );
        Ok(Self {
            inner: Arc::new(Inner {
                mcp: Arc::new(mcp),
                store: options.store,
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
    /// first mutating call, and removes the files that turn created. `NothingToUndo` when that
    /// turn never mutated the workspace (nothing was checkpointed) or there is no last turn.
    pub fn undo(&self, session_id: &str) -> Result<UndoOutcome> {
        let inner = &self.inner;
        let Some(last_turn) = inner.store.turns(session_id)?.pop() else {
            return Ok(UndoOutcome::NothingToUndo);
        };
        let Some(row) = inner.store.checkpoint(&last_turn.id)? else {
            return Ok(UndoOutcome::NothingToUndo);
        };
        checkpoint::restore(&inner.workspace, &row)?;
        Ok(UndoOutcome::Restored)
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
        inner
            .store
            .set_session_title_if_unset(session_id, &inner.redactor.apply(&session_title(input)))?;
        self.emit(EventKind::TurnCreated {
            session_id: session_id.to_string(),
            turn_id: self.turn_id.clone(),
        })
        .await?;

        let session = inner
            .store
            .session(session_id)?
            .ok_or_else(|| anyhow!("session {session_id} not found"))?;
        let provider = Provider::from_str(&session.provider)?;
        let provider_config = inner.config.provider(provider);
        let client = ChatClient::new(ClientConfig {
            dialect: client_dialect(provider),
            base_url: provider_config.base_url.clone(),
            api_key: inner.api_key.clone(),
            model: provider_config.model.clone(),
            temperature: provider_config.temperature,
            seed: provider_config.seed,
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
            tools = TurnTools::build(
                Arc::clone(&inner.mcp),
                inner.workspace.clone(),
                inner.config.tools.clone(),
                inner.config.permission.clone(),
                Arc::clone(&inner.ask_overrides),
                &builtin_definitions,
            ) => {
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
                .run_tool_calls(shared, thread, &completion.tool_calls)
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
                    other.push((
                        index,
                        tokio::spawn(async move {
                            runner.call_host_tool(&tools, &thread_id, &call).await
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
    ) -> ToolResult {
        let invocation = || ToolInvocation {
            name: &call.name,
            arguments_json: &call.arguments,
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

/// Replaces the API key with `[REDACTED]` in persisted data and tool results.
struct Redactor {
    /// The key as it appears inside a JSON string, which covers plain text too.
    needle: String,
}

impl Redactor {
    fn new(api_key: &str) -> Self {
        let quoted = serde_json::to_string(api_key).unwrap_or_default();
        Self {
            needle: quoted.trim_matches('"').to_string(),
        }
    }

    fn apply(&self, text: &str) -> String {
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

/// The first turn's input, trimmed and truncated, becomes the session's title. Later turns
/// don't call this: `set_session_title_if_unset` only writes it once.
fn session_title(input: &str) -> String {
    let first_line = input.lines().next().unwrap_or("").trim();
    match first_line.char_indices().nth(MAX_SESSION_TITLE_LEN) {
        Some((cut, _)) => first_line[..cut].to_string(),
        None => first_line.to_string(),
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
