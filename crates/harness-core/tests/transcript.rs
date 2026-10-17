#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{TestHarness, collect};
use harness_core::{Event, EventKind, Provider, TurnRecord, TurnState, Usage};
use harness_store::{FinishTurn, NewEvent, NewTurn};
use harness_testkit::{fixture, sse_text, sse_tool_calls};
use serde_json::json;

fn names(events: &[Event]) -> Vec<&'static str> {
    events.iter().map(|e| e.kind.name()).collect()
}

fn turn_id_of(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::TurnCreated { turn_id, .. } => Some(turn_id.clone()),
            _ => None,
        })
        .expect("a turn.created event")
}

/// What a live stream carries that is never stored: every delta and each message's start marker.
fn is_streamed_only(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::ModelMessageDelta { .. } | EventKind::ModelMessage { message: None, .. }
    )
}

/// A session with a turn that reads a file and answers, then a turn that is cancelled mid-stream.
/// Returns the session id and each turn's live events.
async fn two_turns(t: &TestHarness) -> (String, Vec<Event>, Vec<Event>) {
    std::fs::write(t.workspace.join("a.txt"), "hello\n").unwrap();
    t.provider.push_sse(sse_tool_calls(&[(
        "c1",
        "read_file",
        r#"{"path":"a.txt"}"#,
    )]));
    t.provider.push_sse(sse_text("done reading"));
    let session_id = t.harness.create_session().unwrap();
    let first = collect(t.harness.start_turn(&session_id, "read it".to_string())).await;

    t.provider
        .push_sse_slow(fixture("deepseek_slow.sse"), Duration::from_millis(30));
    let mut turn = t
        .harness
        .start_turn(&session_id, "count to five".to_string());
    let mut second = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), turn.events.recv())
            .await
            .expect("timed out waiting for the stream to start")
            .expect("turn ended before it started streaming");
        let started = matches!(event.kind, EventKind::ModelMessage { message: None, .. });
        second.push(event);
        if started {
            break;
        }
    }
    turn.cancel();
    second.extend(collect(turn).await);
    (session_id, first, second)
}

#[tokio::test]
async fn transcript_returns_each_turn_in_order_with_its_stored_events() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, first, second) = two_turns(&t).await;

    let transcript = t.harness.transcript(&session_id).unwrap();
    assert_eq!(transcript.len(), 2);
    assert_eq!(transcript.iter().map(|r| r.seq).collect::<Vec<_>>(), [1, 2]);

    let (one, two) = (&transcript[0], &transcript[1]);
    assert_eq!(one.turn_id, turn_id_of(&first));
    assert_eq!(one.input, "read it");
    assert_eq!(one.state, Some(TurnState::Done));
    assert_eq!(one.error, None);
    // The turn's usage is its last model call's, not a sum over the two calls.
    assert_eq!(
        one.usage,
        Some(Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15
        })
    );

    assert_eq!(two.turn_id, turn_id_of(&second));
    assert_eq!(two.input, "count to five");
    assert_eq!(two.state, Some(TurnState::Cancelled));
    assert_eq!(two.error, None);
    assert_eq!(two.usage, None);

    // Each turn's events are exactly the live ones that were stored, in the same order and
    // with the same ids: replay differs from the live stream only by what was never stored.
    for (record, live) in [(one, &first), (two, &second)] {
        let stored: Vec<Event> = live
            .iter()
            .filter(|e| !is_streamed_only(e))
            .cloned()
            .collect();
        assert_eq!(record.events, stored);
        assert!(record.events.iter().all(|e| !is_streamed_only(e)));
    }
    assert_eq!(
        names(&one.events),
        [
            "turn.created",
            "model.message",
            "tool.call",
            "tool.response",
            "model.message",
            "turn.done"
        ]
    );
    assert_eq!(names(&two.events), ["turn.created", "turn.done"]);
}

#[tokio::test]
async fn the_final_model_message_carries_the_authoritative_text() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, first, _) = two_turns(&t).await;
    let record = &t.harness.transcript(&session_id).unwrap()[0];

    // The live stream had deltas building the text; the replay has only the final message.
    assert!(first.iter().any(is_streamed_only));
    let texts: Vec<&str> = record
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ModelMessage {
                message: Some(output),
                ..
            } => Some(output.content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["", "done reading"]);
}

#[tokio::test]
async fn a_reopened_harness_returns_the_same_transcript() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, _, _) = two_turns(&t).await;

    let before = t.harness.transcript(&session_id).unwrap();
    assert_eq!(t.harness.transcript(&session_id).unwrap(), before);
    let reopened = t.reopen(Provider::DeepSeek).await;
    assert_eq!(reopened.transcript(&session_id).unwrap(), before);
}

#[tokio::test]
async fn a_record_serializes_with_its_documented_fields() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, _, _) = two_turns(&t).await;
    let transcript = t.harness.transcript(&session_id).unwrap();

    let first = serde_json::to_value(&transcript[0]).unwrap();
    let mut keys: Vec<&str> = first
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "error", "events", "input", "seq", "state", "turn_id", "usage"
        ]
    );
    assert_eq!(first["state"], json!("done"));
    assert_eq!(first["error"], json!(null));
    assert_eq!(first["usage"]["total_tokens"], json!(15));
    assert_eq!(first["events"][0]["type"], json!("turn.created"));

    let cancelled = serde_json::to_value(&transcript[1]).unwrap();
    assert_eq!(cancelled["state"], json!("cancelled"));
    assert_eq!(cancelled["usage"], json!(null));
}

#[tokio::test]
async fn an_errored_turn_keeps_its_state_and_error() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider
        .push_error(500, r#"{"error":{"message":"the provider is overloaded"}}"#);
    let (session_id, events) = t.run_turn("hello").await;

    let record = &t.harness.transcript(&session_id).unwrap()[0];
    assert_eq!(record.state, Some(TurnState::Error));
    let error = record.error.as_deref().unwrap();
    assert!(error.contains("overloaded"), "{error}");
    assert_eq!(record.usage, None);
    let last = record.events.last().unwrap();
    assert!(matches!(
        last.kind,
        EventKind::TurnDone {
            state: TurnState::Error,
            ..
        }
    ));
    assert_eq!(record.events.last(), events.last());
}

#[tokio::test]
async fn a_turn_left_running_appears_with_no_state() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();
    let store = t.reopen_store();
    // The process stopped mid-turn: the turn row exists, was never finished, and wrote one event.
    store
        .insert_turn(&NewTurn {
            id: "turn-interrupted",
            session_id: &session_id,
            input: "never finished",
        })
        .unwrap();
    let created = Event {
        id: "event-1".to_string(),
        kind: EventKind::TurnCreated {
            session_id: session_id.clone(),
            turn_id: "turn-interrupted".to_string(),
        },
    };
    store
        .insert_event(&NewEvent {
            id: &created.id,
            turn_id: "turn-interrupted",
            thread_id: None,
            kind: created.kind.name(),
            payload_json: &serde_json::to_string(&created).unwrap(),
        })
        .unwrap();

    let transcript = t.harness.transcript(&session_id).unwrap();
    assert_eq!(
        transcript,
        vec![TurnRecord {
            turn_id: "turn-interrupted".to_string(),
            seq: 1,
            input: "never finished".to_string(),
            state: None,
            error: None,
            usage: None,
            events: vec![created],
        }]
    );
    assert_eq!(
        serde_json::to_value(&transcript[0]).unwrap()["state"],
        json!(null)
    );
}

/// A session whose one turn finished normally, and the store to damage it through.
async fn finished_turn(t: &TestHarness) -> (String, String, harness_core::Store) {
    t.provider.push_sse(sse_text("fine"));
    let (session_id, events) = t.run_turn("hello").await;
    (session_id, turn_id_of(&events), t.reopen_store())
}

fn assert_fails_naming(t: &TestHarness, session_id: &str, needles: &[&str]) {
    let error = t.harness.transcript(session_id).unwrap_err();
    let message = format!("{error:#}");
    for needle in needles {
        assert!(
            message.contains(needle),
            "`{needle}` missing from: {message}"
        );
    }
}

#[tokio::test]
async fn an_event_that_does_not_parse_fails_the_call_naming_the_turn_and_event() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, turn_id, store) = finished_turn(&t).await;
    assert!(t.harness.transcript(&session_id).is_ok());

    store
        .insert_event(&NewEvent {
            id: "bad-event",
            turn_id: &turn_id,
            thread_id: None,
            kind: "tool.call",
            payload_json: "{not json",
        })
        .unwrap();
    assert_fails_naming(&t, &session_id, &[&turn_id, "bad-event", "tool.call"]);
}

#[tokio::test]
async fn an_event_that_disagrees_with_its_own_row_fails_the_call() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, turn_id, store) = finished_turn(&t).await;
    let payload = serde_json::to_string(&Event {
        id: "payload-id".to_string(),
        kind: EventKind::TurnDone {
            state: TurnState::Done,
            error: None,
        },
    })
    .unwrap();

    // A different id in the row than in the payload.
    store
        .insert_event(&NewEvent {
            id: "row-id",
            turn_id: &turn_id,
            thread_id: None,
            kind: "turn.done",
            payload_json: &payload,
        })
        .unwrap();
    assert_fails_naming(&t, &session_id, &[&turn_id, "row-id"]);

    // And a different kind in the row than in the payload.
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, turn_id, store) = finished_turn(&t).await;
    store
        .insert_event(&NewEvent {
            id: "payload-id",
            turn_id: &turn_id,
            thread_id: None,
            kind: "tool.call",
            payload_json: &payload,
        })
        .unwrap();
    assert_fails_naming(&t, &session_id, &[&turn_id, "payload-id"]);
}

#[tokio::test]
async fn an_unreadable_state_or_usage_fails_the_call_naming_the_turn() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, turn_id, store) = finished_turn(&t).await;
    store
        .finish_turn(&FinishTurn {
            id: &turn_id,
            state: "paused",
            error: None,
            usage_json: None,
        })
        .unwrap();
    assert_fails_naming(&t, &session_id, &[&turn_id, "paused"]);

    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, turn_id, store) = finished_turn(&t).await;
    store
        .finish_turn(&FinishTurn {
            id: &turn_id,
            state: "done",
            error: None,
            usage_json: Some("not json"),
        })
        .unwrap();
    assert_fails_naming(&t, &session_id, &[&turn_id, "usage"]);
}

#[tokio::test]
async fn an_unknown_session_is_an_error_and_an_empty_session_has_no_turns() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let error = t.harness.transcript("no-such-session").unwrap_err();
    assert!(format!("{error:#}").contains("not found"), "{error:#}");

    let session_id = t.harness.create_session().unwrap();
    assert!(t.harness.transcript(&session_id).unwrap().is_empty());
}

#[test]
fn a_turn_states_text_form_round_trips_and_running_is_not_a_state() {
    for state in [TurnState::Done, TurnState::Error, TurnState::Cancelled] {
        assert_eq!(state.as_str().parse::<TurnState>().unwrap(), state);
    }
    for invalid in ["running", "Done", "", "paused"] {
        assert!(invalid.parse::<TurnState>().is_err(), "{invalid}");
    }
}

#[tokio::test]
async fn a_stored_undo_marker_replays_as_a_typed_event_with_its_wire_shape() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();
    let store = t.reopen_store();
    store
        .insert_turn(&NewTurn {
            id: "turn-1",
            session_id: &session_id,
            input: "edit it",
        })
        .unwrap();
    store
        .finish_turn(&FinishTurn {
            id: "turn-1",
            state: "done",
            error: None,
            usage_json: None,
        })
        .unwrap();
    let marker = Event {
        id: "event-undone".to_string(),
        kind: EventKind::TurnUndone {
            turn_id: "turn-1".to_string(),
        },
    };
    store
        .insert_event(&NewEvent {
            id: &marker.id,
            turn_id: "turn-1",
            thread_id: None,
            kind: marker.kind.name(),
            payload_json: &serde_json::to_string(&marker).unwrap(),
        })
        .unwrap();

    let record = &t.harness.transcript(&session_id).unwrap()[0];
    assert_eq!(record.state, Some(TurnState::Done));
    assert_eq!(record.events, vec![marker.clone()]);
    assert_eq!(
        serde_json::to_value(&record.events[0]).unwrap(),
        json!({"id": "event-undone", "type": "turn.undone", "turn_id": "turn-1"})
    );
    assert_eq!(marker.kind.thread_id(), None);
}
