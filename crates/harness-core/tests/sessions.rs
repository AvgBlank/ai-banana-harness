#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{TestHarness, collect};
use harness_core::{
    Config, ConfigPaths, Event, EventKind, Harness, HarnessOptions, LAST_PROVIDER_SETTING,
    Provider, Store, TurnState, last_provider,
};
use harness_testkit::{TEST_API_KEY, fixture};
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
async fn a_title_written_by_the_store_is_kept_by_later_turns() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (session_id, _) = t.run_turn("Fix the flaky retry test").await;

    // A rename goes through the same single writer and must survive the next turn.
    t.reopen_store()
        .set_session_title(&session_id, "Retry test")
        .unwrap();

    t.provider.push_sse(fixture("deepseek_plain.sse"));
    collect(
        t.harness
            .start_turn(&session_id, "and the logging too".to_string()),
    )
    .await;

    let session = t.reopen_store().session(&session_id).unwrap().unwrap();
    assert_eq!(session.title.as_deref(), Some("Retry test"));
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

/// A harness on a throwaway database whose providers use `config`'s `base_url`s as given.
fn harness_with(config: Config) -> (tempfile::TempDir, Harness) {
    let dir = tempfile::tempdir().unwrap();
    let harness = Harness::open(HarnessOptions {
        config,
        store: Store::open(&dir.path().join("harness.db")).unwrap(),
        provider: Provider::DeepSeek,
        api_key: TEST_API_KEY.to_string(),
        workspace: dir.path().to_path_buf(),
        headless: false,
    })
    .unwrap();
    (dir, harness)
}

fn config_with_urls(deepseek: &str, qwen: &str) -> Config {
    let mut config = Config::embedded_default().unwrap();
    config.providers.deepseek.base_url = deepseek.to_string();
    config.providers.qwen.base_url = qwen.to_string();
    config
}

#[tokio::test]
async fn providers_lists_deepseek_then_qwen_with_their_models_and_destinations() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let options = t.harness.providers().unwrap();
    let defaults = Config::embedded_default().unwrap();
    // The fake provider is `http://127.0.0.1:<port>`: a non-default port is part of the authority.
    let authority = t.provider.base_url.trim_start_matches("http://");

    assert_eq!(
        options.iter().map(|o| o.provider).collect::<Vec<_>>(),
        [Provider::DeepSeek, Provider::Qwen]
    );
    assert_eq!(options[0].model, defaults.providers.deepseek.model);
    assert_eq!(options[1].model, defaults.providers.qwen.model);
    for option in &options {
        assert_eq!(option.scheme, "http");
        assert_eq!(option.host, authority);
    }
}

#[test]
fn a_destination_is_the_scheme_and_authority_and_never_userinfo() {
    let (_dir, harness) = harness_with(config_with_urls(
        "https://user:pw@h:8443/v1?token=abc#frag",
        "https://dashscope.aliyuncs.com:443/compatible-mode/v1",
    ));
    let options = harness.providers().unwrap();

    assert_eq!(options[0].scheme, "https");
    assert_eq!(options[0].host, "h:8443");
    // The scheme's default port is not shown.
    assert_eq!(options[1].host, "dashscope.aliyuncs.com");
    let json = serde_json::to_string(&options).unwrap();
    for secret in ["user", "pw", "token", "abc", "frag"] {
        assert!(!json.contains(secret), "{secret} leaked into {json}");
    }
}

#[test]
fn destinations_normalise_case_default_ports_and_ipv6() {
    let (_dir, harness) = harness_with(config_with_urls(
        "HTTPS://API.Example.COM:443/v1",
        "http://[::1]:9000",
    ));
    let options = harness.providers().unwrap();

    assert_eq!(options[0].scheme, "https");
    assert_eq!(options[0].host, "api.example.com");
    assert_eq!(options[1].scheme, "http");
    assert_eq!(options[1].host, "[::1]:9000");
}

#[test]
fn an_unusable_base_url_is_an_error_that_does_not_repeat_the_url() {
    for bad in ["secret-token not a url", "mailto:secret-token@example.com"] {
        let (_dir, harness) = harness_with(config_with_urls(
            bad,
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
        ));
        let message = format!("{:#}", harness.providers().unwrap_err());
        assert!(message.contains("deepseek"), "{message}");
        assert!(!message.contains("secret-token"), "{message}");
    }
}

#[test]
fn a_workspace_layer_base_url_override_is_reflected() {
    let dir = tempfile::tempdir().unwrap();
    let workspace_toml = dir.path().join("config.toml");
    std::fs::write(
        &workspace_toml,
        "[providers.qwen]\nbase_url = \"https://llm-proxy.example.net:9443/v1\"\n",
    )
    .unwrap();
    let config = Config::load(ConfigPaths {
        user: None,
        workspace: Some(&workspace_toml),
    })
    .unwrap();
    let (_dir, harness) = harness_with(config);
    let options = harness.providers().unwrap();

    assert_eq!(options[0].host, "api.deepseek.com");
    assert_eq!(options[1].host, "llm-proxy.example.net:9443");
}

#[tokio::test]
async fn last_provider_returns_what_switch_provider_last_stored() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let store = t.reopen_store();
    assert_eq!(last_provider(&store), None);

    let session_id = t.harness.create_session().unwrap();
    t.harness
        .switch_provider(&session_id, Provider::Qwen)
        .unwrap();
    assert_eq!(last_provider(&store), Some(Provider::Qwen));

    // A stored value that is not a provider counts as none rather than failing a startup.
    store.set_setting(LAST_PROVIDER_SETTING, "gpt").unwrap();
    assert_eq!(last_provider(&store), None);
}

#[tokio::test]
async fn rename_session_trims_and_replaces_the_title() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();
    t.harness
        .rename_session(&session_id, "  Plan the migration \t")
        .unwrap();
    let session = t.harness.session(&session_id).unwrap().unwrap();
    assert_eq!(session.title.as_deref(), Some("Plan the migration"));

    t.harness
        .rename_session(&session_id, "Second name")
        .unwrap();
    let session = t.harness.session(&session_id).unwrap().unwrap();
    assert_eq!(session.title.as_deref(), Some("Second name"));
}

#[tokio::test]
async fn rename_session_rejects_an_empty_title_and_an_unknown_session() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();
    t.harness.rename_session(&session_id, "Keep me").unwrap();

    for empty in ["", "   ", " \t "] {
        let error = t.harness.rename_session(&session_id, empty).unwrap_err();
        assert!(format!("{error:#}").contains("empty"), "{error:#}");
    }
    let session = t.harness.session(&session_id).unwrap().unwrap();
    assert_eq!(session.title.as_deref(), Some("Keep me"));

    let error = t
        .harness
        .rename_session("no-such-session", "x")
        .unwrap_err();
    assert!(format!("{error:#}").contains("not found"), "{error:#}");
}

#[tokio::test]
async fn rename_session_caps_the_title_at_fifty_characters() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();

    t.harness
        .rename_session(&session_id, &"a".repeat(80))
        .unwrap();
    let title = t
        .harness
        .session(&session_id)
        .unwrap()
        .unwrap()
        .title
        .unwrap();
    assert_eq!(title, "a".repeat(50));

    // Characters, not bytes: a multi-byte title is cut on a character boundary.
    t.harness
        .rename_session(&session_id, &"é".repeat(60))
        .unwrap();
    let title = t
        .harness
        .session(&session_id)
        .unwrap()
        .unwrap()
        .title
        .unwrap();
    assert_eq!(title.chars().count(), 50);
    assert!(title.chars().all(|c| c == 'é'));
}

#[tokio::test]
async fn rename_session_redacts_the_key_even_when_the_cut_would_split_it() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();

    t.harness
        .rename_session(&session_id, &format!("the key is {TEST_API_KEY} ok"))
        .unwrap();
    let title = t
        .harness
        .session(&session_id)
        .unwrap()
        .unwrap()
        .title
        .unwrap();
    assert!(title.contains("[REDACTED]"), "{title}");
    assert!(!title.contains(TEST_API_KEY), "{title}");

    // 40 filler characters then the key: a cut before the redaction would leave ten characters
    // of the key showing, so the filler is sized to make such a cut visible to this test.
    let title = format!("{}{TEST_API_KEY}", "x".repeat(40));
    t.harness.rename_session(&session_id, &title).unwrap();
    let title = t
        .harness
        .session(&session_id)
        .unwrap()
        .unwrap()
        .title
        .unwrap();
    assert!(title.chars().count() <= 50, "{title}");
    assert!(!title.contains("sk-test"), "{title}");
    assert!(title.contains("[REDACTED]"), "{title}");
}

#[tokio::test]
async fn a_renamed_title_is_kept_by_later_turns() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let session_id = t.harness.create_session().unwrap();
    t.harness
        .rename_session(&session_id, "My own name")
        .unwrap();

    t.provider.push_sse(fixture("deepseek_plain.sse"));
    collect(
        t.harness
            .start_turn(&session_id, "first turn of a renamed session".to_string()),
    )
    .await;

    let session = t.harness.session(&session_id).unwrap().unwrap();
    assert_eq!(session.title.as_deref(), Some("My own name"));
}

async fn three_turns(harness: &Harness, session_id: &str) -> Vec<Event> {
    let mut events = Vec::new();
    for i in 0..3 {
        events.extend(collect(harness.start_turn(session_id, format!("turn {i}"))).await);
    }
    events
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_harnesses_on_one_database_run_turns_concurrently() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let second = t.reopen(Provider::DeepSeek).await;
    for _ in 0..6 {
        t.provider.push_sse(fixture("deepseek_plain.sse"));
    }
    let a = t.harness.create_session().unwrap();
    let b = second.create_session().unwrap();

    let (events_a, events_b) = tokio::join!(three_turns(&t.harness, &a), three_turns(&second, &b));

    // A locking error would end a turn as `error`, not `done`.
    for events in [&events_a, &events_b] {
        let ends: Vec<_> = events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::TurnDone { state, error } => Some((*state, error.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(ends, vec![(TurnState::Done, None); 3]);
    }
    let store = t.reopen_store();
    for session_id in [&a, &b] {
        assert_eq!(store.turns(session_id).unwrap().len(), 3);
        let title = store.session(session_id).unwrap().unwrap().title;
        assert_eq!(title.as_deref(), Some("turn 0"));
    }
}
