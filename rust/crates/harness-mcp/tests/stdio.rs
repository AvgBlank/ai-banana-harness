//! A stdio round trip against the fixture server, spawned as a real child process.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use harness_mcp::{CallOutcome, McpClient, McpServerConfig, McpTransport};
use serde_json::{Map, json};

/// `cargo test` builds examples next to the test binaries: `target/<profile>/examples`.
fn fixture_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let path = exe
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join("examples")
        .join(format!("mcp_fixture{}", std::env::consts::EXE_SUFFIX));
    assert!(path.is_file(), "{} missing", path.display());
    path
}

fn config(env: &[(&str, &str)]) -> McpServerConfig {
    McpServerConfig {
        transport: McpTransport::Local {
            command: vec![fixture_bin().to_string_lossy().into_owned()],
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
        },
        preload: false,
        description: None,
    }
}

fn args(value: serde_json::Value) -> Map<String, serde_json::Value> {
    value.as_object().unwrap().clone()
}

#[tokio::test]
async fn lists_every_page_and_renders_call_results() {
    let dir = tempfile::tempdir().unwrap();
    let client = McpClient::connect(&config(&[]), dir.path(), None)
        .await
        .unwrap();

    let names: Vec<_> = client
        .list_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(
        names,
        [
            "echo",
            "fail",
            "structured",
            "env",
            "read_file",
            "delete_everything"
        ]
    );

    assert_eq!(
        client
            .call_tool("echo", args(json!({"text": "hi"})))
            .await
            .unwrap(),
        CallOutcome {
            content: "echo: hi".into(),
            is_error: false
        }
    );
    assert_eq!(
        client.call_tool("fail", Map::new()).await.unwrap(),
        CallOutcome {
            content: r#"{"error":"fixture failure"}"#.into(),
            is_error: true
        }
    );
    assert_eq!(
        client.call_tool("structured", Map::new()).await.unwrap(),
        CallOutcome {
            content: r#"{"answer":42}"#.into(),
            is_error: false
        }
    );
    // A JSON-RPC error (not an `isError` result) surfaces as `Err`.
    assert!(client.call_tool("nope", Map::new()).await.is_err());
}

#[tokio::test]
async fn a_repeated_cursor_ends_the_listing() {
    let dir = tempfile::tempdir().unwrap();
    let client = McpClient::connect(&config(&[("FIXTURE_REPEAT_CURSOR", "1")]), dir.path(), None)
        .await
        .unwrap();
    // First page, then the page for "again", whose cursor is "again" once more: stop there.
    assert_eq!(client.list_tools().await.unwrap().len(), 2);
}

#[tokio::test]
async fn env_values_expand_env_references() {
    let dir = tempfile::tempdir().unwrap();
    // Cargo sets CARGO_PKG_NAME in the test process, so no test has to mutate the environment.
    let client = McpClient::connect(
        &config(&[("GREETING", "hi from {env:CARGO_PKG_NAME}")]),
        dir.path(),
        None,
    )
    .await
    .unwrap();
    let outcome = client
        .call_tool("env", args(json!({"name": "GREETING"})))
        .await
        .unwrap();
    assert_eq!(outcome.content, "hi from harness-mcp");

    let missing = config(&[("X", "{env:BANANA_HARNESS_SURELY_UNSET}")]);
    let error = McpClient::connect(&missing, dir.path(), None)
        .await
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("BANANA_HARNESS_SURELY_UNSET"));
}
