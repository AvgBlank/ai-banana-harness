//! A tiny stdio MCP server for seam-1 tests: newline-delimited JSON-RPC on stdin/stdout,
//! written by hand so the tests exercise rmcp's client against an independent implementation.
//!
//! Tools: `echo{text}`, `fail`, `structured`, `env{name}`, `read_file` (collides with the host
//! tool) and `delete_everything`. `tools/list` is served in two pages.
//!
//! Environment knobs:
//! - `FIXTURE_STARTED_FILE`: a line is appended to this file on startup, so a test can tell
//!   whether (and how often) the server was started.
//! - `FIXTURE_REPEAT_CURSOR=1`: every `tools/list` page returns the same `nextCursor`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, Write};

use serde_json::{Value, json};

fn main() {
    if let Ok(path) = std::env::var("FIXTURE_STARTED_FILE") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(file, "started").unwrap();
    }
    let repeat_cursor = std::env::var("FIXTURE_REPEAT_CURSOR").is_ok_and(|v| v == "1");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        // Notifications (no id) need no reply.
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let method = request["method"].as_str().unwrap_or_default();
        let params = &request["params"];
        let reply = match method {
            "initialize" => json!({ "jsonrpc": "2.0", "id": id, "result": {
                "protocolVersion": params["protocolVersion"],
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "fixture", "version": "0.1.0" },
            }}),
            "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
            "tools/list" => {
                let (page, next) = if repeat_cursor {
                    (tools()[..1].to_vec(), Some("again"))
                } else if params["cursor"] == "page-2" {
                    (tools()[3..].to_vec(), None)
                } else {
                    (tools()[..3].to_vec(), Some("page-2"))
                };
                let mut result = json!({ "tools": page });
                if let Some(next) = next {
                    result["nextCursor"] = json!(next);
                }
                json!({ "jsonrpc": "2.0", "id": id, "result": result })
            }
            "tools/call" => call(id, params),
            _ => json!({ "jsonrpc": "2.0", "id": id, "error": {
                "code": -32601, "message": format!("method not found: {method}"),
            }}),
        };
        writeln!(stdout, "{reply}").unwrap();
        stdout.flush().unwrap();
    }
}

fn tools() -> Vec<Value> {
    let tool = |name: &str, description: &str, properties: Value| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "additionalProperties": false,
            },
        })
    };
    vec![
        tool(
            "echo",
            "Echoes its text.",
            json!({ "text": { "type": "string" } }),
        ),
        tool(
            "fail",
            "Always fails.",
            json!({ "bytes": { "type": "integer" } }),
        ),
        tool("structured", "Returns structured content.", json!({})),
        tool(
            "env",
            "Returns an environment variable.",
            json!({ "name": { "type": "string" } }),
        ),
        tool("read_file", "A tool named like a host tool.", json!({})),
        tool("delete_everything", "Dangerous.", json!({})),
    ]
}

fn call(id: Value, params: &Value) -> Value {
    let args = &params["arguments"];
    let text = |text: String, is_error: bool| json!({ "content": [{ "type": "text", "text": text }], "isError": is_error });
    let result = match params["name"].as_str().unwrap_or_default() {
        "echo" => text(
            format!("echo: {}", args["text"].as_str().unwrap_or_default()),
            false,
        ),
        "fail" => {
            let bytes = args["bytes"].as_u64().unwrap_or(0);
            let message = if bytes == 0 {
                "fixture failure".to_string()
            } else {
                "x".repeat(bytes.min(100_000) as usize)
            };
            text(message, true)
        }
        "structured" => json!({
            "content": [{ "type": "text", "text": "{\"answer\":42}" }],
            "structuredContent": { "answer": 42 },
        }),
        "env" => text(
            std::env::var(args["name"].as_str().unwrap_or_default()).unwrap_or_default(),
            false,
        ),
        "read_file" => text("fixture read_file".to_string(), false),
        "delete_everything" => text("deleted everything".to_string(), false),
        other => {
            return json!({ "jsonrpc": "2.0", "id": id, "error": {
                "code": -32602, "message": format!("unknown tool: {other}"),
            }});
        }
    };
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}
