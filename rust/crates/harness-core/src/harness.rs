use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use harness_llm::{
    ChatClient, ChatEvent, ClientConfig, Completion, Dialect, Message, Role, ToolCallResult,
    ToolDefinition, Usage,
};
use harness_store::{FinishTurn, NewEvent, NewSession, NewTurn, Session, Store, UpsertThread};
use harness_tools::ToolResult;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use ulid::{Generator, Ulid};

use crate::builtins::{Builtin, MAX_PARALLEL_SUB_AGENTS, SubAgentRequest};
use crate::compaction;
use crate::config::{Config, Provider};
use crate::event::{Event, EventKind, ModelOutput, TurnState};
use crate::instructions::{SystemPromptInput, system_prompt};
use crate::mcp::{McpAuthNotifier, McpRegistry, McpServerStatus};
use crate::skills::load_skills;
use crate::tools::{ToolInvocation, TurnTools};

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
}

pub struct TurnHandle {
    pub events: mpsc::Receiver<Event>,
    cancel: CancellationToken,
}

impl TurnHandle {
    /// Stops the running turn; it ends with `turn.done{state: cancelled}`.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
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

    /// Starts a turn in the background. The stream always ends with `turn.done`.
    pub fn start_turn(&self, session_id: &str, input: String) -> TurnHandle {
        let (tx, events) = mpsc::channel(256);
        let cancel = CancellationToken::new();
        let turn = TurnRunner {
            inner: Arc::clone(&self.inner),
            turn_id: new_id(),
            tx,
            cancel: cancel.clone(),
        };
        tokio::spawn(turn.run(session_id.to_string(), input));
        TurnHandle { events, cancel }
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
            tools = self.with_mcp_auth_watch(&thread_id, |notifier| TurnTools::build(
                Arc::clone(&inner.mcp),
                inner.workspace.clone(),
                inner.config.tools.clone(),
                inner.config.permission.clone(),
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

        loop {
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

            for call in &completion.tool_calls {
                self.emit(EventKind::ToolCall {
                    thread_id: thread_id.clone(),
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .await?;
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
            context_usage.prompt_tokens += compaction::estimate_tokens(&messages[first_result..]);
            // And again after the tool step, so the round trip is durable too.
            self.snapshot_thread(&shared.session_id, thread, messages, &context_usage)?;
        }
    }

    /// Runs one round's calls, returning results in the calls' order. Host tools run in
    /// parallel on blocking threads; `create_sub_agent` calls become child threads, and this
    /// thread waits for all of them (only leaf threads run).
    async fn run_tool_calls(
        &self,
        shared: &Arc<TurnShared>,
        thread: &ThreadInfo,
        calls: &[ToolCallResult],
        mcp_auth: McpAuthNotifier,
    ) -> Result<Vec<ToolResult>> {
        let mut results: Vec<Option<ToolResult>> = vec![None; calls.len()];
        let mut other = Vec::new();
        let mut children = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            match Builtin::find(&call.name, thread.is_root(), &self.inner.config.agent) {
                Some(Builtin::CreateSubAgent) => match SubAgentRequest::parse(&call.arguments) {
                    Ok(request) => children.push((index, request)),
                    Err(message) => results[index] = Some(ToolResult::error(message)),
                },
                None => {
                    let tools = Arc::clone(&shared.tools);
                    let call = call.clone();
                    let mcp_auth = mcp_auth.clone();
                    other.push((
                        index,
                        tokio::spawn(async move {
                            tools
                                .call(ToolInvocation {
                                    name: &call.name,
                                    arguments_json: &call.arguments,
                                    mcp_auth,
                                })
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
        for (index, handle) in other {
            results[index] = Some(handle.await.context("tool call task panicked")?);
        }
        results
            .into_iter()
            .map(|result| result.ok_or_else(|| anyhow!("a tool call produced no result")))
            .collect()
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
