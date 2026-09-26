//! Streaming chat client for DeepSeek and Qwen over OpenAI-compatible chat completions.

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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
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

    pub async fn stream(&self, messages: &[Message]) -> Result<ChatStream> {
        let c = &self.config;
        let mut body = json!({
            "model": c.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "temperature": c.temperature,
        });
        match c.dialect {
            Dialect::DeepSeek => body["thinking"] = json!({ "type": "disabled" }),
            Dialect::Qwen => {
                body["enable_thinking"] = json!(false);
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
    finished: bool,
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
                if let Some(content) = choice.delta.and_then(|d| d.content) {
                    delta.push_str(&content);
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
        Ok(Some(ChatEvent::Done(Completion {
            content: std::mem::take(&mut self.content),
            finish_reason,
            usage: self.usage.take(),
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
