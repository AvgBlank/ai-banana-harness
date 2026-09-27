use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context, Result, bail};
use harness_llm::{
    ChatClient, ChatEvent, ClientConfig, Dialect, Message, Role, ToolDefinition, Usage,
};
use harness_store::{FinishTurn, NewEvent, NewSession, NewTurn, Store};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ulid::{Generator, Ulid};

use crate::config::{Config, Provider};
use crate::event::{Event, EventKind, ModelOutput, TurnState};
use crate::instructions::{SystemPromptInput, system_prompt};
use crate::skills::load_skills;

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
    client: ChatClient,
    config: Config,
    provider: Provider,
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
        let provider_config = options.config.provider(options.provider);
        let client = ChatClient::new(ClientConfig {
            dialect: match options.provider {
                Provider::DeepSeek => Dialect::DeepSeek,
                Provider::Qwen => Dialect::Qwen,
            },
            base_url: provider_config.base_url.clone(),
            api_key: options.api_key.clone(),
            model: provider_config.model.clone(),
            temperature: provider_config.temperature,
            seed: provider_config.seed,
        })?;
        Ok(Self {
            inner: Arc::new(Inner {
                store: options.store,
                client,
                config: options.config,
                provider: options.provider,
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
        self.emit(EventKind::TurnCreated {
            session_id: session_id.to_string(),
            turn_id: self.turn_id.clone(),
        })
        .await?;

        let thread_id = new_id();
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
        let mut messages = vec![
            Message {
                role: Role::System,
                content: prompt,
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: input.to_string(),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        ];
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
            let mut stream = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Ok(TurnState::Cancelled),
                result = inner.client.stream(&messages, &tools) => result?,
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

            if completion.tool_calls.is_empty() {
                return Ok(TurnState::Done);
            }

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
            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                results.push(handle.await.context("tool call task panicked")?);
            }

            for (call, result) in completion.tool_calls.iter().zip(results) {
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
