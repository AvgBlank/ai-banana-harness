#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::TestHarness;
use harness_core::{Event, EventKind, ModelOutput, Provider, TurnState, Usage};
use harness_testkit::{TEST_API_KEY, fixture};
use serde_json::json;

fn names(events: &[Event]) -> Vec<&'static str> {
    events.iter().map(|e| e.kind.name()).collect()
}

fn deltas(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ModelMessageDelta { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect()
}

fn final_message(events: &[Event]) -> &ModelOutput {
    events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::ModelMessage {
                message: Some(m), ..
            } => Some(m),
            _ => None,
        })
        .expect("no final model.message")
}

fn turn_done(events: &[Event]) -> (TurnState, Option<String>) {
    match &events.last().unwrap().kind {
        EventKind::TurnDone { state, error } => (*state, error.clone()),
        other => panic!("last event is {other:?}"),
    }
}

#[tokio::test]
async fn deepseek_plain_answer_streams_and_persists() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (session_id, events) = t.run_turn("say hi").await;

    assert_eq!(
        names(&events),
        [
            "turn.created",
            "model.message",
            "model.message.delta",
            "model.message.delta",
            "model.message",
            "turn.done",
        ]
    );
    assert!(matches!(
        events[1].kind,
        EventKind::ModelMessage { message: None, .. }
    ));
    assert_eq!(deltas(&events), "Hello there!");
    let usage = Usage {
        prompt_tokens: 21,
        completion_tokens: 3,
        total_tokens: 24,
    };
    assert_eq!(
        final_message(&events),
        &ModelOutput {
            content: "Hello there!".into(),
            finish_reason: "stop".into(),
            usage: Some(usage.clone()),
        }
    );
    assert_eq!(turn_done(&events), (TurnState::Done, None));
    let ids: Vec<_> = events.iter().map(|e| e.id.as_str()).collect();
    assert!(
        ids.iter()
            .all(|id| id.len() == 26 && *id == id.to_lowercase())
    );
    assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "event ids are not monotonic"
    );

    // The request is a streamed chat completion with thinking off and no seed.
    let requests = t.provider.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.path, "/chat/completions");
    // The stub checks the key and records only that it matched.
    assert!(request.auth_ok);
    assert_eq!(request.headers["authorization"], "[redacted]");
    let body = &request.body;
    assert_eq!(body["model"], "deepseek-flash");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"], json!({ "include_usage": true }));
    assert_eq!(body["thinking"], json!({ "type": "disabled" }));
    assert_eq!(body["temperature"], 0.0);
    assert!(body.get("seed").is_none());
    assert!(body.get("enable_thinking").is_none());
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(
        body["messages"][1],
        json!({ "role": "user", "content": "say hi" })
    );

    // A later reader sees the session, the finished turn and only the durable events.
    let store = t.reopen_store();
    let session = store.session(&session_id).unwrap().expect("session row");
    assert_eq!(session.provider, "deepseek");
    assert_eq!(session.model, "deepseek-flash");
    assert_eq!(session.workspace, t.workspace.to_string_lossy());
    let turns = store.turns(&session_id).unwrap();
    assert_eq!(turns.len(), 1);
    let turn = &turns[0];
    assert_eq!((turn.seq, turn.state.as_str()), (1, "done"));
    assert_eq!(turn.input, "say hi");
    assert!(turn.error.is_none() && turn.finished_at.is_some());
    let row_usage: Usage = serde_json::from_str(turn.usage_json.as_deref().unwrap()).unwrap();
    assert_eq!(row_usage, usage);

    let rows = store.events(&turn.id).unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, ["turn.created", "model.message", "turn.done"]);
    let durable: Vec<Event> = rows
        .iter()
        .map(|r| serde_json::from_str(&r.payload_json).unwrap())
        .collect();
    assert_eq!(
        durable,
        [events[0].clone(), events[4].clone(), events[5].clone()]
    );
    assert_eq!(rows[1].thread_id.as_deref(), events[4].kind.thread_id());
}

#[tokio::test]
async fn qwen_plain_answer_reads_usage_from_the_final_chunk() {
    let t = TestHarness::open(Provider::Qwen).await;
    t.provider.push_sse(fixture("qwen_plain.sse"));

    let (session_id, events) = t.run_turn("say hi").await;

    assert_eq!(
        names(&events),
        [
            "turn.created",
            "model.message",
            "model.message.delta",
            "model.message.delta",
            "model.message",
            "turn.done",
        ]
    );
    assert_eq!(deltas(&events), "Hi from Qwen.");
    let message = final_message(&events);
    assert_eq!(message.content, "Hi from Qwen.");
    assert_eq!(message.finish_reason, "stop");
    assert_eq!(
        message.usage,
        Some(Usage {
            prompt_tokens: 25,
            completion_tokens: 4,
            total_tokens: 29,
        })
    );
    assert_eq!(turn_done(&events), (TurnState::Done, None));

    let body = &t.provider.requests()[0].body;
    assert_eq!(body["model"], "qwen3.8-flash");
    assert_eq!(body["enable_thinking"], false);
    assert_eq!(body["seed"], 42);
    assert!(body.get("thinking").is_none());

    let store = t.reopen_store();
    assert_eq!(
        store.session(&session_id).unwrap().unwrap().provider,
        "qwen"
    );
    let turns = store.turns(&session_id).unwrap();
    assert_eq!(turns[0].state, "done");
    assert!(
        turns[0]
            .usage_json
            .as_deref()
            .unwrap()
            .contains("\"total_tokens\":29")
    );
    assert_eq!(store.events(&turns[0].id).unwrap().len(), 3);
}

#[tokio::test]
async fn length_finish_ends_the_turn_with_max_tokens_breached() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_length.sse"));

    let (session_id, events) = t.run_turn("write a novel").await;

    assert_eq!(final_message(&events).finish_reason, "length");
    assert_eq!(
        turn_done(&events),
        (TurnState::Error, Some("max_tokens breached".into()))
    );
    let turn = &t.reopen_store().turns(&session_id).unwrap()[0];
    assert_eq!(turn.state, "error");
    assert_eq!(turn.error.as_deref(), Some("max_tokens breached"));
}

#[tokio::test]
async fn http_error_ends_the_turn_and_the_key_is_redacted() {
    let t = TestHarness::open(Provider::Qwen).await;
    // Providers can echo the key back; it must never reach the database.
    t.provider.push_error(
        401,
        format!(
            r#"{{"error":{{"message":"Incorrect API key provided: {TEST_API_KEY}","type":"invalid_request_error","param":null,"code":"invalid_api_key"}},"request_id":"a1b2"}}"#
        ),
    );

    let (session_id, events) = t.run_turn(&format!("my key is {TEST_API_KEY}")).await;

    assert_eq!(names(&events), ["turn.created", "turn.done"]);
    let (state, error) = turn_done(&events);
    assert_eq!(state, TurnState::Error);
    let error = error.unwrap();
    assert!(error.contains("401"), "{error}");
    assert!(
        error.contains("Incorrect API key provided: [REDACTED]"),
        "{error}"
    );

    let store = t.reopen_store();
    let turn = &store.turns(&session_id).unwrap()[0];
    assert_eq!(turn.state, "error");
    assert_eq!(turn.input, "my key is [REDACTED]");
    let mut persisted = vec![turn.input.clone(), turn.error.clone().unwrap()];
    persisted.extend(
        store
            .events(&turn.id)
            .unwrap()
            .into_iter()
            .map(|e| e.payload_json),
    );
    assert!(persisted.iter().all(|text| !text.contains(TEST_API_KEY)));
    assert!(persisted.iter().any(|text| text.contains("[REDACTED]")));
}
