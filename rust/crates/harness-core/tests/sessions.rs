#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{TestHarness, collect, fixture};
use harness_core::{EventKind, LAST_PROVIDER_SETTING, Provider};
use serde_json::json;

#[tokio::test]
async fn a_second_turn_in_the_same_session_sends_the_first_turns_messages_back() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (session_id, _events) = t.run_turn("say hi").await;

    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let events = collect(
        t.harness
            .start_turn(&session_id, "say hi again".to_string()),
    )
    .await;
    assert!(matches!(
        events.last().unwrap().kind,
        EventKind::TurnDone { .. }
    ));

    let requests = t.provider.requests();
    assert_eq!(requests.len(), 2);
    let messages = requests[1].body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4, "{messages:?}");
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1], json!({ "role": "user", "content": "say hi" }));
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["content"], "Hello there!");
    assert_eq!(
        messages[3],
        json!({ "role": "user", "content": "say hi again" })
    );
}

#[tokio::test]
async fn reopening_a_harness_on_the_same_database_resumes_the_session() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (session_id, _events) = t.run_turn("say hi").await;

    // A brand new harness, as a later `make run` would open: same database, same workspace.
    let harness2 = t.reopen(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let events = collect(harness2.start_turn(&session_id, "say hi again".to_string())).await;
    assert!(matches!(
        events.last().unwrap().kind,
        EventKind::TurnDone { .. }
    ));

    let requests = t.provider.requests();
    assert_eq!(requests.len(), 2);
    let messages = requests[1].body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4, "{messages:?}");
    assert_eq!(messages[1], json!({ "role": "user", "content": "say hi" }));
    assert_eq!(messages[2]["content"], "Hello there!");
}

#[tokio::test]
async fn the_schema_is_created_fresh_and_reopening_an_existing_database_keeps_its_data() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (session_id, _events) = t.run_turn("say hi").await;

    // Opening the same file again must not fail (the schema is already at the latest
    // migration) or lose what was written.
    let store = t.reopen_store();
    let session = store.session(&session_id).unwrap().expect("session row");
    assert_eq!(session.workspace, t.workspace.to_string_lossy());
    let turns = store.turns(&session_id).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].state, "done");
}

#[tokio::test]
async fn switching_provider_updates_the_session_and_the_last_provider_setting() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();

    t.harness
        .switch_provider(&session_id, Provider::Qwen)
        .unwrap();

    let store = t.reopen_store();
    let session = store.session(&session_id).unwrap().unwrap();
    assert_eq!(session.provider, "qwen");
    assert_eq!(session.model, "qwen3.8-flash");
    assert_eq!(
        store.get_setting(LAST_PROVIDER_SETTING).unwrap().as_deref(),
        Some("qwen")
    );
}

#[tokio::test]
async fn the_first_turns_input_becomes_the_session_title_once() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (session_id, _) = t.run_turn("Fix the flaky retry test").await;

    t.provider.push_sse(fixture("deepseek_plain.sse"));
    collect(
        t.harness
            .start_turn(&session_id, "and the logging too".to_string()),
    )
    .await;

    let store = t.reopen_store();
    let session = store.session(&session_id).unwrap().unwrap();
    assert_eq!(session.title.as_deref(), Some("Fix the flaky retry test"));
}

#[tokio::test]
async fn sessions_lists_every_session_this_database_has_seen() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let first = t.harness.create_session().unwrap();
    let second = t.harness.create_session().unwrap();

    let sessions = t.harness.sessions().unwrap();
    let ids: Vec<_> = sessions.iter().map(|s| s.id.clone()).collect();
    assert!(ids.contains(&first) && ids.contains(&second), "{ids:?}");
}
