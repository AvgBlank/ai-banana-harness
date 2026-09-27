#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_store::Store;

#[test]
fn setting_round_trips_and_upserts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("harness.db")).unwrap();

    assert_eq!(store.get_setting("last_provider").unwrap(), None);

    store.set_setting("last_provider", "qwen").unwrap();
    assert_eq!(
        store.get_setting("last_provider").unwrap(),
        Some("qwen".to_string())
    );

    store.set_setting("last_provider", "deepseek").unwrap();
    assert_eq!(
        store.get_setting("last_provider").unwrap(),
        Some("deepseek".to_string())
    );

    // A second connection sees the same migrated schema and the persisted value.
    let reopened = Store::open(&dir.path().join("harness.db")).unwrap();
    assert_eq!(
        reopened.get_setting("last_provider").unwrap(),
        Some("deepseek".to_string())
    );
}
