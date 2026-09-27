//! Streaming chat client for DeepSeek and Qwen over OpenAI-compatible chat completions.

use std::collections::BTreeMap;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sse_stream::{Error as SseError, Sse, SseStream};

/// The request and stream quirks that differ between the two providers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    DeepSeek,
    Qwen,
}

pub struct ClientConfig {
    pub dialect: Dialect,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f64,
    /// Sent only to Qwen; DeepSeek has no seed parameter.
    pub seed: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Set on an assistant message that requested tool calls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallRequest>,
    /// Set on a `Role::Tool` message, echoing back the call it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// An assistant message's tool call, as sent back to the provider in message history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// One tool a model call may use. `parameters` is the reduced schema (`type`/`properties`/
/// `required` only) a caller has already prepared.
#[derive(Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// One tool call the model asked for, fully assembled from its streamed fragments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub id: String,
    pub name: String,
    /// Raw JSON text; the caller parses it (a model "does not always generate valid JSON").
    pub arguments: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

/// The assembled result of one streamed completion.
#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub content: String,
    pub finish_reason: String,
    pub usage: Option<Usage>,
    /// Non-empty when `finish_reason` is `"tool_calls"`, assembled from streamed fragments by
    /// each call's `index` (Qwen sends an empty `id` on continuation chunks, never overwriting
    /// the accumulated one).
    pub tool_calls: Vec<ToolCallResult>,
}

pub enum ChatEvent {
    Delta(String),
    Done(Completion),
}

pub struct ChatClient {
    http: reqwest::Client,
    config: ClientConfig,
}

impl ChatClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        // reqwest is built without a bundled crypto provider; the first client installs ring.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            // DeepSeek holds queued requests open with keep-alive comments for up to 10 minutes.
            .read_timeout(Duration::from_secs(660))
            .build()
            .context("building HTTP client")?;
        Ok(Self { http, config })
    }

    pub async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<ChatStream> {
        let c = &self.config;
        let mut body = json!({
            "model": c.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "temperature": c.temperature,
        });
        if !tools.is_empty() {
            body["tools"] = json!(
                tools
                    .iter()
                    .map(|t| json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        },
                    }))
                    .collect::<Vec<_>>()
            );
        }
        match c.dialect {
            Dialect::DeepSeek => body["thinking"] = json!({ "type": "disabled" }),
            Dialect::Qwen => {
                body["enable_thinking"] = json!(false);
                // Qwen defaults this to false, which silently limits a turn to one call at a time.
                if !tools.is_empty() {
                    body["parallel_tool_calls"] = json!(true);
                }
                if let Some(seed) = c.seed {
                    body["seed"] = json!(seed);
                }
            }
        }
        let url = format!("{}/chat/completions", c.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(url)
            .bearer_auth(&c.api_key)
            .json(&body)
            .send()
            .await
            .context("sending request to the provider")?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("provider returned HTTP {status}: {}", error_message(&text));
        }
        Ok(ChatStream {
            sse: Box::pin(SseStream::from_bytes_stream(response.bytes_stream())),
            content: String::new(),
            finish_reason: None,
            usage: None,
            tool_calls: BTreeMap::new(),
            finished: false,
        })
    }
}

type SseItems = Pin<Box<dyn Stream<Item = Result<Sse, SseError>> + Send>>;

pub struct ChatStream {
    sse: SseItems,
    content: String,
    finish_reason: Option<String>,
    usage: Option<Usage>,
    tool_calls: BTreeMap<usize, PartialToolCall>,
    finished: bool,
}

#[derive(Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl PartialToolCall {
    fn finish(self, index: usize) -> Result<ToolCallResult> {
        Ok(ToolCallResult {
            id: self
                .id
                .ok_or_else(|| anyhow!("tool call at index {index} never received an id"))?,
            name: self
                .name
                .ok_or_else(|| anyhow!("tool call at index {index} never received a name"))?,
            arguments: self.arguments,
        })
    }
}

impl ChatStream {
    /// Yields content deltas, then one `Done` with the assembled completion, then `None`.
    pub async fn next(&mut self) -> Result<Option<ChatEvent>> {
        if self.finished {
            return Ok(None);
        }
        loop {
            let Some(item) = self.sse.next().await else {
                return self.finish();
            };
            let Some(data) = item.context("reading the provider stream")?.data else {
                continue;
            };
            if data.trim() == "[DONE]" {
                return self.finish();
            }
            let chunk: Chunk = serde_json::from_str(&data)
                .with_context(|| format!("malformed stream chunk: {data}"))?;
            if let Some(error) = chunk.error {
                bail!("provider stream error: {}", error_value_message(&error));
            }
            // DeepSeek puts usage on the last content chunk; Qwen sends it in a final `choices: []` chunk.
            if chunk.usage.is_some() {
                self.usage = chunk.usage;
            }
            let mut delta = String::new();
            for choice in chunk.choices.into_iter().flatten() {
                if let Some(d) = choice.delta {
                    if let Some(content) = d.content {
                        delta.push_str(&content);
                    }
                    for fragment in d.tool_calls.into_iter().flatten() {
                        let entry = self.tool_calls.entry(fragment.index).or_default();
                        // Qwen sends `id: ""` (not absent) on continuation chunks; never let an
                        // empty fragment erase the id the first chunk gave us.
                        if let Some(id) = fragment.id.filter(|id| !id.is_empty()) {
                            entry.id = Some(id);
                        }
                        if let Some(function) = fragment.function {
                            if let Some(name) = function.name {
                                entry.name.get_or_insert_with(String::new).push_str(&name);
                            }
                            if let Some(arguments) = function.arguments {
                                entry.arguments.push_str(&arguments);
                            }
                        }
                    }
                }
                if choice.finish_reason.is_some() {
                    self.finish_reason = choice.finish_reason;
                }
            }
            if !delta.is_empty() {
                self.content.push_str(&delta);
                return Ok(Some(ChatEvent::Delta(delta)));
            }
        }
    }

    fn finish(&mut self) -> Result<Option<ChatEvent>> {
        self.finished = true;
        let finish_reason = self
            .finish_reason
            .take()
            .ok_or_else(|| anyhow!("provider stream ended without a finish_reason"))?;
        let tool_calls = std::mem::take(&mut self.tool_calls)
            .into_iter()
            .map(|(index, partial)| partial.finish(index))
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(ChatEvent::Done(Completion {
            content: std::mem::take(&mut self.content),
            finish_reason,
            usage: self.usage.take(),
            tool_calls,
        })))
    }
}

#[derive(Deserialize)]
struct Chunk {
    choices: Option<Vec<Choice>>,
    usage: Option<Usage>,
    error: Option<Value>,
}

#[derive(Deserialize)]
struct Choice {
    delta: Option<Delta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCallFragment>>,
}

#[derive(Deserialize)]
struct ToolCallFragment {
    index: usize,
    id: Option<String>,
    function: Option<FunctionFragment>,
}

#[derive(Deserialize)]
struct FunctionFragment {
    name: Option<String>,
    arguments: Option<String>,
}

/// Extracts a message from `{"error":{"message"}}`, DashScope's `{"message"}`, or raw text.
fn error_message(body: &str) -> String {
    match serde_json::from_str::<Value>(body) {
        Ok(value) => {
            let error = value.get("error").unwrap_or(&value);
            error_value_message(error)
        }
        Err(_) => body.trim().to_string(),
    }
}

fn error_value_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map_or_else(|| error.to_string(), str::to_string)
}
