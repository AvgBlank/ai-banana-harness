//! Frames and errors on the wire.

use serde::Serialize;
use serde_json::{Map, Value, json};

/// A request-level failure. The code is the wire contract; the message is for people.
#[derive(Debug, thiserror::Error)]
pub(crate) enum BridgeError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("unknown method `{0}`")]
    UnknownMethod(String),
    #[error("{0}")]
    InvalidParams(String),
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),
    #[error("workspace is not open: {0}")]
    WorkspaceNotOpen(String),
    #[error("session {0} belongs to a different workspace")]
    WorkspaceMismatch(String),
    #[error("session {0} not found")]
    SessionNotFound(String),
    #[error("AI_API_KEY is not set; the workspace cannot be opened")]
    ApiKeyMissing,
    #[error("{0:#}")]
    Internal(#[source] anyhow::Error),
}

impl BridgeError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::UnknownMethod(_) => "unknown_method",
            Self::InvalidParams(_) => "invalid_params",
            Self::WorkspaceNotFound(_) => "workspace_not_found",
            Self::WorkspaceNotOpen(_) => "workspace_not_open",
            Self::WorkspaceMismatch(_) => "workspace_mismatch",
            Self::SessionNotFound(_) => "session_not_found",
            Self::ApiKeyMissing => "api_key_missing",
            Self::Internal(_) => "internal",
        }
    }
}

/// A frame the client sent, once it parsed as a request.
pub(crate) struct Request {
    pub(crate) id: String,
    pub(crate) method: String,
    pub(crate) params: Map<String, Value>,
}

/// Parses one line. On failure, the error comes with the request id when one could be read, so
/// the response can still be correlated.
pub(crate) fn parse_request(line: &[u8]) -> Result<Request, (Option<String>, BridgeError)> {
    let text = std::str::from_utf8(line).map_err(|_| {
        (
            None,
            BridgeError::InvalidRequest("line is not valid UTF-8".to_string()),
        )
    })?;
    let value: Value = serde_json::from_str(text).map_err(|_| {
        (
            None,
            BridgeError::InvalidRequest("line is not valid JSON".to_string()),
        )
    })?;
    let Value::Object(mut object) = value else {
        return Err((
            None,
            BridgeError::InvalidRequest("frame is not a JSON object".to_string()),
        ));
    };
    let id = match object.remove("id") {
        Some(Value::String(id)) => id,
        _ => {
            return Err((
                None,
                BridgeError::InvalidRequest("`id` must be a string".to_string()),
            ));
        }
    };
    let fail = |message: &str| {
        (
            Some(id.clone()),
            BridgeError::InvalidRequest(message.into()),
        )
    };
    if object.get("kind").and_then(Value::as_str) != Some("request") {
        return Err(fail("`kind` must be \"request\""));
    }
    let Some(Value::String(method)) = object.remove("method") else {
        return Err(fail("`method` must be a string"));
    };
    let params = match object.remove("params") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(params)) => params,
        Some(_) => {
            return Err((
                Some(id),
                BridgeError::InvalidParams("`params` must be an object".to_string()),
            ));
        }
    };
    Ok(Request { id, method, params })
}

/// One line to write, already serialized.
#[derive(Debug)]
pub(crate) struct Line(pub(crate) String);

#[derive(Serialize)]
struct Ready<'a> {
    kind: &'static str,
    protocol: u32,
    app: &'a str,
    version: &'a str,
    api_key_present: bool,
}

pub(crate) fn ready(app: &str, version: &str, api_key_present: bool) -> Line {
    to_line(&Ready {
        kind: "ready",
        protocol: crate::PROTOCOL_VERSION,
        app,
        version,
        api_key_present,
    })
}

pub(crate) fn fatal(code: &str, message: &str) -> Line {
    to_line(&json!({"kind": "fatal", "error": {"code": code, "message": message}}))
}

pub(crate) fn ok(id: &str, result: Value) -> Line {
    to_line(&json!({"kind": "response", "id": id, "ok": true, "result": result}))
}

/// An error response; `message` has already been redacted.
pub(crate) fn error(id: Option<&str>, code: &str, message: &str) -> Line {
    to_line(&json!({
        "kind": "response", "id": id, "ok": false,
        "error": {"code": code, "message": message},
    }))
}

fn to_line(value: &impl Serialize) -> Line {
    // Serializing a `Value` or a plain struct cannot fail; the fallback keeps the stream valid.
    Line(serde_json::to_string(value).unwrap_or_else(|_| {
        r#"{"kind":"fatal","error":{"code":"internal","message":"frame could not be encoded"}}"#
            .to_string()
    }))
}
