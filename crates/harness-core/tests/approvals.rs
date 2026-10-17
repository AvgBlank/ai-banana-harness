#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::TestHarness;
use harness_core::{Answer, PatternRules, PermissionAction, Provider, TurnState, UndoOutcome};
use harness_testkit::{sse_text, sse_tool_calls};
use serde_json::json;

#[tokio::test]
async fn ask_rule_allow_deny_and_allow_for_session() {
    let t = TestHarness::open_configured(Provider::DeepSeek, |config| {
        config.permission.bash =
            PatternRules(vec![("echo ask*".to_string(), PermissionAction::Ask)]);
    })
    .await;
    t.provider.push_sse(sse_tool_calls(&[(
        "c1",
        "bash",
        r#"{"command":"echo ask one"}"#,
    )]));
    t.provider.push_sse(sse_tool_calls(&[(
        "c2",
        "bash",
        r#"{"command":"echo ask two"}"#,
    )]));
    t.provider.push_sse(sse_tool_calls(&[(
        "c3",
        "bash",
        r#"{"command":"echo ask three"}"#,
    )]));
    t.provider.push_sse(sse_tool_calls(&[(
        "c4",
        "bash",
        r#"{"command":"echo ask four"}"#,
    )]));
    t.provider.push_sse(sse_text("done"));

    let session_id = t.harness.create_session().unwrap();
    let mut turn = t.harness.start_turn(&session_id, "go".to_string());

    let mut approvals = 0;
    let mut responses = Vec::new();
    loop {
        let event = turn.events.recv().await.expect("turn ended unexpectedly");
        match event.kind {
            harness_core::EventKind::ToolApprovalRequired { action_id, .. } => {
                approvals += 1;
                let answer = match approvals {
                    1 => Answer::Allow,
                    2 => Answer::Deny,
                    3 => Answer::AllowForSession,
                    n => panic!("unexpected approval #{n}, the 4th identical rule should skip it"),
                };
                turn.reply(&action_id, answer);
            }
            harness_core::EventKind::ToolResponse {
                content, blocked, ..
            } => responses.push((content, blocked)),
            harness_core::EventKind::TurnDone { state, error } => {
                assert_eq!(state, TurnState::Done, "{error:?}");
                break;
            }
            _ => {}
        }
    }

    assert_eq!(
        approvals, 3,
        "the 4th call to the same rule should skip the prompt"
    );
    assert_eq!(responses.len(), 4, "{responses:?}");
    assert!(
        responses[0].0.contains("ask one") && !responses[0].1,
        "{:?}",
        responses[0]
    );
    assert_eq!(
        responses[1].0,
        r#"User denied tool call: bash rule "echo ask*" requires approval"#
    );
    assert!(responses[1].1);
    assert!(
        responses[2].0.contains("ask three") && !responses[2].1,
        "{:?}",
        responses[2]
    );
    assert!(
        responses[3].0.contains("ask four") && !responses[3].1,
        "{:?}",
        responses[3]
    );
}

#[tokio::test]
async fn question_answer_becomes_the_tool_result_and_the_turn_continues() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(sse_tool_calls(&[(
        "c1",
        "ask_question",
        r#"{"question":"Which file?","options":["a.txt","b.txt"]}"#,
    )]));
    t.provider.push_sse(sse_text("done"));

    let session_id = t.harness.create_session().unwrap();
    let mut turn = t.harness.start_turn(&session_id, "go".to_string());
    let action_id = loop {
        let event = turn
            .events
            .recv()
            .await
            .expect("turn ended before asking a question");
        if let harness_core::EventKind::ToolResponseRequired {
            action_id,
            question,
            options,
            ..
        } = event.kind
        {
            assert_eq!(question, "Which file?");
            assert_eq!(options, vec!["a.txt".to_string(), "b.txt".to_string()]);
            break action_id;
        }
    };
    turn.reply(&action_id, Answer::Text("a.txt".to_string()));

    let events = common::collect(turn).await;
    let response = events
        .iter()
        .find_map(|e| match &e.kind {
            harness_core::EventKind::ToolResponse { content, .. } => Some(content.clone()),
            _ => None,
        })
        .expect("no tool.response in the stream");
    assert_eq!(response, "a.txt");
    assert!(matches!(
        events.last().map(|e| &e.kind),
        Some(harness_core::EventKind::TurnDone {
            state: TurnState::Done,
            error: None
        })
    ));
}

#[tokio::test]
async fn cancel_while_approval_is_pending_ends_the_turn_cancelled() {
    let t = TestHarness::open_configured(Provider::DeepSeek, |config| {
        config.permission.bash = PatternRules(vec![("echo*".to_string(), PermissionAction::Ask)]);
    })
    .await;
    t.provider.push_sse(sse_tool_calls(&[(
        "c1",
        "bash",
        r#"{"command":"echo hi"}"#,
    )]));

    let session_id = t.harness.create_session().unwrap();
    let mut turn = t.harness.start_turn(&session_id, "go".to_string());
    loop {
        let event = turn
            .events
            .recv()
            .await
            .expect("turn ended before asking approval");
        if matches!(
            event.kind,
            harness_core::EventKind::ToolApprovalRequired { .. }
        ) {
            break;
        }
    }
    turn.cancel();

    let events = common::collect(turn).await;
    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(harness_core::EventKind::TurnDone {
                state: TurnState::Cancelled,
                ..
            })
        ),
        "{events:?}"
    );
}

#[tokio::test]
async fn headless_auto_answers_a_question_and_the_turn_finishes_done() {
    let t = TestHarness::open_headless(Provider::DeepSeek).await;
    t.provider.push_sse(sse_tool_calls(&[(
        "c1",
        "ask_question",
        r#"{"question":"Proceed?","options":["yes","no"]}"#,
    )]));
    t.provider.push_sse(sse_text("done"));

    let (_, events) = t.run_turn("go").await;

    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(harness_core::EventKind::TurnDone {
                state: TurnState::Done,
                error: None
            })
        ),
        "{events:?}"
    );
    let response = events
        .iter()
        .find_map(|e| match &e.kind {
            harness_core::EventKind::ToolResponse { content, .. } => Some(content.clone()),
            _ => None,
        })
        .expect("no tool.response in the stream");
    assert_eq!(
        response,
        "No user available; proceed with your best judgement"
    );
}

#[test]
fn answers_use_their_documented_json_shapes() {
    let cases = [
        (Answer::Allow, json!("allow")),
        (Answer::AllowForSession, json!("allow_for_session")),
        (Answer::Deny, json!("deny")),
        (Answer::Text("blue".to_string()), json!({"text": "blue"})),
    ];
    for (answer, expected) in cases {
        assert_eq!(serde_json::to_value(&answer).unwrap(), expected);
        let parsed: Answer = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), expected);
    }
    for invalid in [
        json!("Allow"),
        json!("always"),
        json!({"text": 1}),
        json!(null),
    ] {
        assert!(
            serde_json::from_value::<Answer>(invalid.clone()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn undo_outcomes_use_snake_case_json() {
    for (outcome, expected) in [
        (UndoOutcome::Restored, json!("restored")),
        (UndoOutcome::NothingToUndo, json!("nothing_to_undo")),
    ] {
        assert_eq!(serde_json::to_value(outcome).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<UndoOutcome>(expected).unwrap(),
            outcome
        );
    }
    assert!(serde_json::from_value::<UndoOutcome>(json!("NothingToUndo")).is_err());
}

#[test]
fn providers_use_lowercase_json_that_matches_their_text_form() {
    for (provider, expected) in [
        (Provider::DeepSeek, json!("deepseek")),
        (Provider::Qwen, json!("qwen")),
    ] {
        assert_eq!(serde_json::to_value(provider).unwrap(), expected);
        assert_eq!(expected, json!(provider.as_str()));
        assert_eq!(
            serde_json::from_value::<Provider>(expected).unwrap(),
            provider
        );
    }
    // The wire form is strict lowercase, unlike the command line's case-insensitive parse.
    assert!(serde_json::from_value::<Provider>(json!("DeepSeek")).is_err());
    assert!(serde_json::from_value::<Provider>(json!("gpt")).is_err());
}
