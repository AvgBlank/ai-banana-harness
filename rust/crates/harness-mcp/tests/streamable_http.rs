//! A remote (streamable-HTTP) MCP round trip against an in-process fixture, with a static
//! bearer header supplied through config (no OAuth).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "fixture/http_fixture.rs"]
mod http_fixture;

use std::collections::BTreeMap;

use harness_mcp::{CallOutcome, McpClient, McpServerConfig, McpTransport};

#[tokio::test]
async fn lists_and_calls_a_tool_over_streamable_http_with_a_static_bearer_header() {
    let fixture = http_fixture::spawn().await;
    let mut headers = BTreeMap::new();
    headers.insert(
        "Authorization".to_string(),
        "Bearer test-secret-token".to_string(),
    );
    let config = McpServerConfig {
        transport: McpTransport::Remote {
            url: fixture.url.clone(),
            headers,
            oauth: false,
        },
        preload: false,
        description: None,
    };
    let dir = tempfile::tempdir().unwrap();

    let client = McpClient::connect(&config, dir.path(), None).await.unwrap();
    let names: Vec<_> = client
        .list_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["echo"]);

    let mut arguments = serde_json::Map::new();
    arguments.insert("text".to_string(), "hi".into());
    let outcome = client.call_tool("echo", arguments).await.unwrap();
    assert_eq!(
        outcome,
        CallOutcome {
            content: "echo: hi".into(),
            is_error: false,
        }
    );

    let seen = fixture.seen_authorization.lock().unwrap();
    assert!(!seen.is_empty(), "the fixture never received a request");
    assert!(
        seen.iter()
            .all(|h| h.as_deref() == Some("Bearer test-secret-token")),
        "{seen:?}"
    );
}

/// A server that answers every request with a bare 401 (no OAuth involved on our side).
async fn spawn_401() -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 401 Unauthorized\r\n\
                          WWW-Authenticate: Bearer realm=\"test\"\r\n\
                          Content-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            });
        }
    });
    format!("http://127.0.0.1:{port}/mcp")
}

#[tokio::test]
async fn a_401_without_oauth_fails_clearly() {
    let url = spawn_401().await;
    let config = McpServerConfig {
        transport: McpTransport::Remote {
            url,
            headers: BTreeMap::new(),
            oauth: false,
        },
        preload: false,
        description: None,
    };
    let dir = tempfile::tempdir().unwrap();

    let error = McpClient::connect(&config, dir.path(), None)
        .await
        .err()
        .unwrap();
    let message = format!("{error:#}").to_lowercase();
    assert!(message.contains("auth"), "{message}");
}
