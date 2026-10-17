#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_store::{NewSession, Store};
use serde_json::json;

fn store_with_session(dir: &std::path::Path) -> Store {
    let store = Store::open(&dir.join("harness.db")).unwrap();
    store
        .insert_session(&NewSession {
            id: "s1",
            workspace: "/work/repo",
            provider: "deepseek",
            model: "deepseek-flash",
        })
        .unwrap();
    store
}

#[test]
fn set_session_title_overwrites_an_existing_title() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(store.session("s1").unwrap().unwrap().title, None);

    store.set_session_title("s1", "first").unwrap();
    store.set_session_title("s1", "second").unwrap();
    assert_eq!(
        store.session("s1").unwrap().unwrap().title.as_deref(),
        Some("second")
    );

    // The overwrite is durable and visible to a second connection.
    let reopened = Store::open(&dir.path().join("harness.db")).unwrap();
    assert_eq!(
        reopened.session("s1").unwrap().unwrap().title.as_deref(),
        Some("second")
    );
}

#[test]
fn a_title_is_not_activity_so_the_session_keeps_its_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.session("s1").unwrap().unwrap();

    store.set_session_title("s1", "renamed").unwrap();
    let after = store.session("s1").unwrap().unwrap();

    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.updated_at, before.updated_at);
}

#[test]
fn a_session_serializes_with_the_documented_fields() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let untitled = serde_json::to_value(store.session("s1").unwrap().unwrap()).unwrap();
    let fields: Vec<&str> = untitled
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = fields.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "created_at",
            "id",
            "model",
            "provider",
            "title",
            "updated_at",
            "workspace"
        ]
    );
    assert_eq!(untitled["id"], json!("s1"));
    assert_eq!(untitled["title"], json!(null));
    assert_eq!(untitled["workspace"], json!("/work/repo"));
    assert_eq!(untitled["provider"], json!("deepseek"));
    assert_eq!(untitled["model"], json!("deepseek-flash"));
    assert!(untitled["created_at"].is_string() && untitled["updated_at"].is_string());

    store.set_session_title("s1", "Fix the retry test").unwrap();
    let titled = serde_json::to_value(store.session("s1").unwrap().unwrap()).unwrap();
    assert_eq!(titled["title"], json!("Fix the retry test"));
}
