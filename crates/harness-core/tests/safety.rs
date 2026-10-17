#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::TestHarness;
use harness_core::{Event, EventKind, Provider, Redactor, TurnState};
use harness_testkit::fixture;
use serde_json::{Value, json};

/// One DeepSeek tool-call reply asking for `name(arguments)`.
fn tool_call(name: &str, arguments: Value) -> String {
    let escaped = serde_json::to_string(&arguments.to_string()).unwrap();
    fixture("deepseek_tool_call.sse")
        .replace("__NAME__", name)
        .replace("__ARGS__", escaped.trim_matches('"'))
}

fn turn_error(events: &[Event]) -> Option<String> {
    match events.last().map(|e| &e.kind) {
        Some(EventKind::TurnDone {
            state: TurnState::Error,
            error,
        }) => error.clone(),
        _ => None,
    }
}

#[tokio::test]
async fn iteration_limit_ends_the_turn_with_an_error() {
    let t = TestHarness::open_configured(Provider::DeepSeek, |config| {
        config.agent.max_llm_calls = 3;
    })
    .await;
    // Each round reads a different path, so this never triggers the doom loop.
    for path in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        t.provider
            .push_sse(tool_call("read_file", json!({ "path": path })));
    }

    let (_, events) = t.run_turn("keep reading files").await;

    let error = turn_error(&events).expect("expected turn.done{state: error}");
    assert!(
        error.contains("3"),
        "expected the configured limit in: {error}"
    );
    assert_eq!(
        t.provider.requests().len(),
        3,
        "must not call the model a 4th time past the limit"
    );
}

#[tokio::test]
async fn doom_loop_warns_at_three_and_ends_at_five() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::write(t.workspace.join("notes.txt"), "hello\n").unwrap();
    for _ in 0..5 {
        t.provider
            .push_sse(tool_call("read_file", json!({ "path": "notes.txt" })));
    }

    let (_, events) = t.run_turn("read the same file over and over").await;

    let error = turn_error(&events).expect("expected turn.done{state: error}");
    assert!(
        error.contains("doom loop"),
        "expected a doom-loop error, got: {error}"
    );

    // The 5th identical call is only known once the model's 5th response arrives, so the turn
    // makes exactly 5 calls before bailing (the 5th call itself is never executed).
    let requests = t.provider.requests();
    assert_eq!(requests.len(), 5);
    let fourth_request_text = requests[3].body.to_string();
    assert!(
        fourth_request_text.contains("<bh-internal>") && fourth_request_text.contains("3 times"),
        "expected the 4th request to carry the warning injected after the 3rd repeat: {fourth_request_text}"
    );
}

#[test]
fn the_redactor_replaces_every_occurrence_of_the_key() {
    let redactor = Redactor::new("sk-abc123");
    assert_eq!(
        redactor.apply("a sk-abc123 b sk-abc123"),
        "a [REDACTED] b [REDACTED]"
    );
    assert_eq!(redactor.apply("nothing to hide"), "nothing to hide");
}

#[test]
fn the_redactor_finds_a_key_inside_json_text_and_an_empty_key_redacts_nothing() {
    // A key with a character JSON escapes still matches in its escaped, in-text form.
    let redactor = Redactor::new("sk\"quoted");
    let text = serde_json::to_string("token sk\"quoted end").unwrap();
    let redacted = redactor.apply(&text);
    assert!(!redacted.contains("quoted"), "{redacted}");
    assert!(redacted.contains("[REDACTED]"));

    assert_eq!(Redactor::new("").apply("unchanged"), "unchanged");
}
