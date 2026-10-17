use std::str::FromStr;

use anyhow::{Result, bail};
use harness_llm::Usage;
use serde::{Deserialize, Serialize};

/// One item of a turn's event stream. `id` is a monotonic lowercase ULID.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventKind {
    #[serde(rename = "turn.created")]
    TurnCreated { session_id: String, turn_id: String },
    #[serde(rename = "turn.done")]
    TurnDone {
        state: TurnState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// The workspace was restored to the checkpoint taken before this turn's first change. It is
    /// written to the undone turn's own log after the turn ended, so a replayed transcript shows
    /// it and the turn cannot be undone twice; it never appears on a live turn stream.
    #[serde(rename = "turn.undone")]
    TurnUndone { turn_id: String },
    /// One LLM call's message. It is streamed twice: first as a start marker with no `message`,
    /// then with the final message. Only the final one is persisted, so a replayed transcript has
    /// no start marker; the marker and the deltas exist on the live stream alone.
    #[serde(rename = "model.message")]
    ModelMessage {
        thread_id: String,
        message_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<ModelOutput>,
    },
    /// A streamed text fragment. Never persisted.
    #[serde(rename = "model.message.delta")]
    ModelMessageDelta {
        thread_id: String,
        message_id: String,
        content: String,
    },
    /// A tool call the model requested, before it runs.
    #[serde(rename = "tool.call")]
    ToolCall {
        thread_id: String,
        tool_call_id: String,
        name: String,
        /// Raw JSON text, exactly as the model sent it.
        arguments: String,
    },
    /// A tool call's outcome. `blocked` is set when a guardrail rejected the call; `content` is
    /// then `"Blocked by guardrail: <rule>"`.
    #[serde(rename = "tool.response")]
    ToolResponse {
        thread_id: String,
        tool_call_id: String,
        content: String,
        blocked: bool,
        /// The file's before/after diff for an edit or write; omitted for other tools.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
    },
    /// The thread's context was replaced by a summary; `usage` is the compacted context's size.
    #[serde(rename = "context.compacted")]
    ContextCompacted { thread_id: String, usage: Usage },
    /// A remote MCP server needs a browser OAuth login; `url` is the address to open.
    #[serde(rename = "mcp.auth_required")]
    McpAuthRequired {
        thread_id: String,
        server: String,
        url: String,
    },
    /// A subagent thread started under `parent`; `title` is the name its parent gave it.
    #[serde(rename = "thread.created")]
    ThreadCreated {
        thread_id: String,
        parent: String,
        title: String,
    },
    /// A subagent thread finished; `error` is set when it failed.
    #[serde(rename = "thread.done")]
    ThreadDone {
        thread_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// A tool call's permission resolved to `ask`; the turn waits for `reply(action_id, ...)`.
    #[serde(rename = "tool.approval_required")]
    ToolApprovalRequired {
        action_id: String,
        thread_id: String,
        tool_call_id: String,
        name: String,
        arguments: String,
        /// The guardrail rule that asked, e.g. `bash rule "sudo *" requires approval`.
        rule: String,
    },
    /// The root-only question tool asked the user something; the turn waits for
    /// `reply(action_id, ...)`.
    #[serde(rename = "tool.response_required")]
    ToolResponseRequired {
        action_id: String,
        thread_id: String,
        question: String,
        options: Vec<String>,
    },
}

impl EventKind {
    /// The wire name, also stored in the `event.type` column.
    pub fn name(&self) -> &'static str {
        match self {
            EventKind::TurnCreated { .. } => "turn.created",
            EventKind::TurnDone { .. } => "turn.done",
            EventKind::TurnUndone { .. } => "turn.undone",
            EventKind::ModelMessage { .. } => "model.message",
            EventKind::ModelMessageDelta { .. } => "model.message.delta",
            EventKind::ToolCall { .. } => "tool.call",
            EventKind::ToolResponse { .. } => "tool.response",
            EventKind::ContextCompacted { .. } => "context.compacted",
            EventKind::McpAuthRequired { .. } => "mcp.auth_required",
            EventKind::ThreadCreated { .. } => "thread.created",
            EventKind::ThreadDone { .. } => "thread.done",
            EventKind::ToolApprovalRequired { .. } => "tool.approval_required",
            EventKind::ToolResponseRequired { .. } => "tool.response_required",
        }
    }

    /// The thread that produced the event; `None` for turn-level events.
    pub fn thread_id(&self) -> Option<&str> {
        match self {
            EventKind::ModelMessage { thread_id, .. }
            | EventKind::ModelMessageDelta { thread_id, .. }
            | EventKind::ToolCall { thread_id, .. }
            | EventKind::ToolResponse { thread_id, .. }
            | EventKind::ContextCompacted { thread_id, .. }
            | EventKind::McpAuthRequired { thread_id, .. }
            | EventKind::ThreadCreated { thread_id, .. }
            | EventKind::ThreadDone { thread_id, .. }
            | EventKind::ToolApprovalRequired { thread_id, .. }
            | EventKind::ToolResponseRequired { thread_id, .. } => Some(thread_id),
            EventKind::TurnCreated { .. }
            | EventKind::TurnDone { .. }
            | EventKind::TurnUndone { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelOutput {
    pub content: String,
    pub finish_reason: String,
    pub usage: Option<Usage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnState {
    Done,
    Error,
    Cancelled,
}

impl TurnState {
    pub fn as_str(self) -> &'static str {
        match self {
            TurnState::Done => "done",
            TurnState::Error => "error",
            TurnState::Cancelled => "cancelled",
        }
    }
}

impl FromStr for TurnState {
    type Err = anyhow::Error;

    /// The inverse of [`TurnState::as_str`]. A turn that never finished is stored as `running`,
    /// which is not a state a turn ends in, so it is rejected here and handled by the caller.
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "done" => Ok(TurnState::Done),
            "error" => Ok(TurnState::Error),
            "cancelled" => Ok(TurnState::Cancelled),
            other => bail!("unknown turn state `{other}`"),
        }
    }
}
