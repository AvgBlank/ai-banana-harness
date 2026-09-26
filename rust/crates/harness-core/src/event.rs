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
    /// Sent twice per LLM call: first as a start marker with no `message`, then with the final message.
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
}

impl EventKind {
    /// The wire name, also stored in the `event.type` column.
    pub fn name(&self) -> &'static str {
        match self {
            EventKind::TurnCreated { .. } => "turn.created",
            EventKind::TurnDone { .. } => "turn.done",
            EventKind::ModelMessage { .. } => "model.message",
            EventKind::ModelMessageDelta { .. } => "model.message.delta",
        }
    }

    /// The thread that produced the event; `None` for turn-level events.
    pub fn thread_id(&self) -> Option<&str> {
        match self {
            EventKind::ModelMessage { thread_id, .. }
            | EventKind::ModelMessageDelta { thread_id, .. } => Some(thread_id),
            EventKind::TurnCreated { .. } | EventKind::TurnDone { .. } => None,
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
