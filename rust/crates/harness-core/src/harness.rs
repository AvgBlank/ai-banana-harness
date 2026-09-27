use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use harness_llm::{
    ChatClient, ChatEvent, ClientConfig, Completion, Dialect, Message, Role, ToolDefinition, Usage,
};
use harness_store::{FinishTurn, NewEvent, NewSession, NewTurn, Session, Store, UpsertThread};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ulid::{Generator, Ulid};

use crate::compaction;
use crate::config::{Config, Provider};
use crate::event::{Event, EventKind, ModelOutput, TurnState};
use crate::instructions::{SystemPromptInput, system_prompt};
use crate::skills::load_skills;

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
    store: Store,
    config: Config,
    /// The provider a brand-new session starts with; each turn afterwards reads the session's
    /// own `provider` column, which `/provider` can change independently.
    provider: Provider,
    api_key: String,
    workspace: PathBuf,
    redactor: Redactor,
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
        Ok(Self {
            inner: Arc::new(Inner {
                store: options.store,
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

/// The only writer to the store: each durable event is persisted, then forwarded.
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
            root: true,
        });
        let history: Vec<Message> = existing_thread
            .as_ref()
            .and_then(|t| serde_json::from_str(&t.context_json).ok())
            .unwrap_or_default();
        let mut messages = Vec::with_capacity(history.len() + 2);
        messages.push(Message {
            role: Role::System,
            content: prompt,
            tool_calls: Vec::new(),
            tool_call_id: None,
        });
        messages.extend(history);
        messages.push(Message {
            role: Role::User,
            content: input.to_string(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        });
        // The stored usage already counts the history; only what's new since is estimated.
        let stored_usage: Option<Usage> = existing_thread
            .as_ref()
            .and_then(|t| t.context_usage_json.as_deref())
            .and_then(|json| serde_json::from_str(json).ok());
        let uncounted = if stored_usage.is_some() {
            &messages[messages.len() - 1..]
        } else {
            &messages[..]
        };
        let mut context_usage = stored_usage.unwrap_or_default();
        context_usage.prompt_tokens += compaction::estimate_tokens(uncounted);
        let threshold = compaction::threshold(provider_config);
        let tool_specs = harness_tools::host_tool_specs();
        let tools: Vec<ToolDefinition> = tool_specs
            .into_iter()
            .map(|spec| ToolDefinition {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            })
            .collect();

        // One iteration per LLM round: stream an answer, and if it asked for tool calls, run
        // them and go around again. A round with no tool calls ends the turn.
        loop {
            if compaction::is_due(&context_usage, threshold) {
                let request = compaction::summary_request(&messages[1..]);
                let Some(summary) = self.complete(&client, &request).await? else {
                    return Ok(TurnState::Cancelled);
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
                self.snapshot_thread(session_id, &thread_id, &messages, &context_usage)?;
            }

            let mut stream = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Ok(TurnState::Cancelled),
                result = client.stream(&messages, &tools) => result?,
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
                    () = self.cancel.cancelled() => return Ok(TurnState::Cancelled),
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
                content: completion.content,
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
            self.snapshot_thread(session_id, &thread_id, &messages, &context_usage)?;

            if completion.tool_calls.is_empty() {
                return Ok(TurnState::Done);
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

            // Each host tool call is blocking filesystem work; spawn_blocking runs them on
            // separate OS threads so they genuinely execute in parallel.
            let workspace = inner.workspace.clone();
            let mut handles = Vec::with_capacity(completion.tool_calls.len());
            for call in completion.tool_calls.clone() {
                let workspace = workspace.clone();
                handles.push(tokio::task::spawn_blocking(move || {
                    harness_tools::call_by_name(&workspace, &call.name, &call.arguments)
                }));
            }
            let mut batch = Vec::with_capacity(handles.len());
            for (call, handle) in completion.tool_calls.iter().zip(handles) {
                let result = handle.await.context("tool call task panicked")?;
                batch.push((call.name.clone(), result));
            }
            let batch = tokio::task::spawn_blocking(move || {
                harness_tools::cap_large_outputs(&workspace, &mut batch);
                batch
            })
            .await
            .context("large tool output task panicked")?;

            let first_result = messages.len();
            for (call, (_, result)) in completion.tool_calls.iter().zip(batch) {
                self.emit(EventKind::ToolResponse {
                    thread_id: thread_id.clone(),
                    tool_call_id: call.id.clone(),
                    content: result.content.clone(),
                    blocked: result.blocked,
                })
                .await?;
                messages.push(Message {
                    role: Role::Tool,
                    content: result.content,
                    tool_calls: Vec::new(),
                    tool_call_id: Some(call.id.clone()),
                });
            }
            context_usage.prompt_tokens += compaction::estimate_tokens(&messages[first_result..]);
            // And again after the tool step, so the round trip is durable too.
            self.snapshot_thread(session_id, &thread_id, &messages, &context_usage)?;
        }
    }

    /// Upserts the thread's snapshot from `messages`, excluding the leading system message
    /// (which is rebuilt fresh every turn, so it is never part of the persisted context).
    fn snapshot_thread(
        &self,
        session_id: &str,
        thread_id: &str,
        messages: &[Message],
        context_usage: &Usage,
    ) -> Result<()> {
        let history = &messages[1..];
        let context_json = self.inner.redactor.apply(&serde_json::to_string(history)?);
        let context_usage_json = serde_json::to_string(context_usage)?;
        self.inner.store.upsert_thread(&UpsertThread {
            id: thread_id,
            session_id,
            parent_id: None,
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

/// Replaces the API key with `[REDACTED]` in anything the harness persists.
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
