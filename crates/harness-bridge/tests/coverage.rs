//! The checked-in golden frames cover every method and event kind of protocol v1.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{Client, golden};
use harness_core::{Event, EventKind};
use serde_json::{Value, json};

/// The protocol v1 method table.
const METHODS: [&str; 16] = [
    "session.list",
    "workspace.open",
    "workspace.close",
    "workspace.list",
    "provider.list",
    "session.create",
    "session.resume",
    "session.rename",
    "session.switch_provider",
    "session.undo",
    "turn.start",
    "turn.cancel",
    "action.reply",
    "changes.workspace",
    "changes.turn",
    "shutdown",
];

/// Every `EventKind`'s wire name.
const EVENT_KINDS: [&str; 13] = [
    "turn.created",
    "turn.done",
    "turn.undone",
    "model.message",
    "model.message.delta",
    "tool.call",
    "tool.response",
    "context.compacted",
    "mcp.auth_required",
    "thread.created",
    "thread.done",
    "tool.approval_required",
    "tool.response_required",
];

fn walk(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                if key == "type"
                    && let Some(kind) = v.as_str()
                {
                    found.insert(kind.to_string());
                }
                walk(v, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| walk(v, found)),
        _ => {}
    }
}

#[test]
fn golden_frames_cover_every_method_and_event_kind() {
    // A remote OAuth server is out of reach of an offline test, so this one frame is built from
    // core's own event type in the bridge's envelope; the file still pins its wire shape.
    let event = Event {
        id: "<id-0>".to_string(),
        kind: EventKind::McpAuthRequired {
            thread_id: "<id-1>".to_string(),
            server: "docs".to_string(),
            url: "https://auth.example.com/authorize".to_string(),
        },
    };
    golden(
        "event-mcp-auth",
        &[json!({
            "kind": "event", "workspace": "<workspace>", "session_id": "<id-2>",
            "turn_id": "<id-3>", "event": event,
        })],
    );

    let mut methods = BTreeSet::new();
    let mut types = BTreeSet::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let frame: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{}: not a JSON line ({e})", path.display()));
            if let Some(method) = frame["method"].as_str() {
                methods.insert(method.to_string());
            }
            walk(&frame, &mut types);
        }
    }
    let missing_methods: Vec<&str> = METHODS
        .into_iter()
        .filter(|m| !methods.contains(*m))
        .collect();
    let missing_kinds: Vec<&str> = EVENT_KINDS
        .into_iter()
        .filter(|k| !types.contains(*k))
        .collect();
    assert!(
        missing_methods.is_empty(),
        "no golden frame for methods {missing_methods:?}"
    );
    assert!(
        missing_kinds.is_empty(),
        "no golden frame for event kinds {missing_kinds:?}"
    );
}

#[tokio::test]
async fn every_listed_method_exists() {
    // Ties the list above to the server: a method it does not know would be `unknown_method`.
    let mut client = Client::start().await;
    client.next().await;
    for method in METHODS.into_iter().filter(|m| *m != "shutdown") {
        let response = client.call("m", method, json!({})).await;
        assert_ne!(response["error"]["code"], "unknown_method", "{method}");
    }
    let unknown = client.call("m", "files.read", json!({})).await;
    assert_eq!(unknown["error"]["code"], "unknown_method");
}
