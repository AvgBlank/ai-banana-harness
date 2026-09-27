//! An in-process streamable-HTTP MCP fixture for seam-1 tests: one JSON-RPC response per POST,
//! `Content-Type: application/json` (not SSE), records every request's `Authorization` header.
//!
//! ponytail: no session id, no SSE — our client's default config is stateless, which is all
//! `connect_remote`'s non-OAuth path needs; add session/SSE handling only if a test needs it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct Fixture {
    pub url: String,
    pub seen_authorization: Arc<Mutex<Vec<Option<String>>>>,
}

/// Starts the fixture on a random localhost port and returns immediately; the listener task
/// runs until the test process exits.
pub async fn spawn() -> Fixture {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_for_task = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let seen = Arc::clone(&seen_for_task);
            tokio::spawn(async move {
                let _ = handle(&mut socket, &seen).await;
            });
        }
    });
    Fixture {
        url: format!("http://127.0.0.1:{port}/mcp"),
        seen_authorization: seen,
    }
}

async fn handle(
    socket: &mut tokio::net::TcpStream,
    seen: &Arc<Mutex<Vec<Option<String>>>>,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; 65536];
    let n = socket.read(&mut buf).await?;
    let raw = &buf[..n];
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("no header/body split in test request"))?;
    let header_text = String::from_utf8_lossy(&raw[..split]);
    let body = &raw[split + 4..];

    // OAuth metadata discovery (protected-resource / AS metadata / OIDC) probes several
    // well-known GET paths before falling back to legacy endpoints; a clean 404 is what makes
    // that fallback graceful instead of a hard error.
    if !header_text.starts_with("POST") {
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        return Ok(());
    }

    let authorization = header_text
        .lines()
        .find_map(|line| {
            line.strip_prefix("authorization:")
                .or(line.strip_prefix("Authorization:"))
        })
        .map(|value| value.trim().to_string());
    seen.lock().unwrap().push(authorization);

    let request: Value = serde_json::from_slice(body)?;
    let reply = request
        .get("id")
        .cloned()
        .map(|id| handle_rpc(id, &request));

    let body_str = reply.map(|r| r.to_string()).unwrap_or_default();
    let status_line = if body_str.is_empty() {
        "HTTP/1.1 202 Accepted"
    } else {
        "HTTP/1.1 200 OK"
    };
    let response = format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body_str.len(),
        body_str
    );
    socket.write_all(response.as_bytes()).await?;
    Ok(())
}

fn handle_rpc(id: Value, request: &Value) -> Value {
    let method = request["method"].as_str().unwrap_or_default();
    let params = &request["params"];
    match method {
        "initialize" => json!({ "jsonrpc": "2.0", "id": id, "result": {
            "protocolVersion": params["protocolVersion"],
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "http-fixture", "version": "0.1.0" },
        }}),
        "tools/list" => json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": [{
            "name": "echo",
            "description": "Echoes its text.",
            "inputSchema": {
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "additionalProperties": false,
            },
        }]}}),
        "tools/call" if params["name"] == "echo" => {
            let text = params["arguments"]["text"].as_str().unwrap_or_default();
            json!({ "jsonrpc": "2.0", "id": id, "result": {
                "content": [{ "type": "text", "text": format!("echo: {text}") }],
                "isError": false,
            }})
        }
        _ => json!({ "jsonrpc": "2.0", "id": id, "error": {
            "code": -32601, "message": format!("method not found: {method}"),
        }}),
    }
}
