//! MCP over stdio, end to end: the fixture server from `harness-mcp` runs as a real child
//! process, configured through a workspace config overlay.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{TestHarness, collect, fixture};
use harness_core::{Event, EventKind, McpServerState, McpServerStatus, Provider, TurnState};
use serde_json::{Value, json};
use tempfile::TempDir;

/// `cargo test --workspace` builds harness-mcp's examples next to every test binary.
fn fixture_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let path = exe
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join("examples")
        .join(format!("mcp_fixture{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} missing; run `cargo test --workspace` so harness-mcp's example is built",
        path.display()
    );
    path
}

/// A `[mcp.<name>]` section running the fixture, which records each start in `<markers>/<name>`.
fn server_toml(name: &str, markers: &Path, extra: &str) -> String {
    format!(
        "[mcp.{name}]\ntype = \"local\"\ncommand = [{bin:?}]\n{extra}\n\
         [mcp.{name}.env]\nFIXTURE_STARTED_FILE = {marker:?}\n\
         GREETING = \"hi from {{env:CARGO_PKG_NAME}}\"\n",
        bin = fixture_bin().to_string_lossy(),
        marker = markers.join(name).to_string_lossy(),
    )
}

fn starts(markers: &Path, name: &str) -> usize {
    std::fs::read_to_string(markers.join(name))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

/// A DeepSeek-format stream in which the model requests `calls` (`id`, `name`, JSON args).
fn tool_calls_sse(calls: &[(&str, &str, Value)]) -> String {
    let chunk = |delta: Value, finish: Value, usage: Value| {
        let chunk = json!({
            "id": "mcp-test", "object": "chat.completion.chunk", "created": 1790000100,
            "model": "deepseek-flash",
            "choices": [{ "index": 0, "delta": delta, "logprobs": null, "finish_reason": finish }],
            "usage": usage,
        });
        format!("data: {chunk}\n\n")
    };
    let mut sse = chunk(
        json!({ "role": "assistant", "content": "" }),
        Value::Null,
        Value::Null,
    );
    for (index, (id, name, args)) in calls.iter().enumerate() {
        sse += &chunk(
            json!({ "tool_calls": [{
                "index": index, "id": id, "type": "function",
                "function": { "name": name, "arguments": args.to_string() },
            }]}),
            Value::Null,
            Value::Null,
        );
    }
    sse += &chunk(
        json!({}),
        json!("tool_calls"),
        json!({ "prompt_tokens": 40, "completion_tokens": 10, "total_tokens": 50 }),
    );
    sse + "data: [DONE]\n\n"
}

fn responses(events: &[Event]) -> Vec<(String, bool)> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolResponse {
                content, blocked, ..
            } => Some((content.clone(), *blocked)),
            _ => None,
        })
        .collect()
}

fn assert_done(events: &[Event]) {
    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(EventKind::TurnDone {
                state: TurnState::Done,
                error: None
            })
        ),
        "{:?}",
        events.last()
    );
}

fn tool_names(request_body: &Value) -> Vec<String> {
    request_body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_string())
        .collect()
}

async fn open(toml: &str) -> TestHarness {
    TestHarness::open_layered(Provider::DeepSeek, None, Some(toml)).await
}

#[tokio::test]
async fn a_preloaded_tool_is_called_directly_and_returns_its_result() {
    let markers = TempDir::new().unwrap();
    let t = open(&server_toml("eager", markers.path(), "preload = true")).await;
    t.provider.push_sse(tool_calls_sse(&[
        ("call_1", "echo", json!({ "text": "hi" })),
        ("call_2", "env", json!({ "name": "GREETING" })),
        ("call_3", "structured", json!({})),
    ]));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_, events) = t.run_turn("use the echo tool").await;

    assert_done(&events);
    assert_eq!(
        responses(&events),
        [
            ("echo: hi".to_string(), false),
            // `{env:VAR}` in the config's env was expanded when the server started.
            ("hi from harness-core".to_string(), false),
            (r#"{"answer":42}"#.to_string(), false),
        ]
    );
    assert_eq!(starts(markers.path(), "eager"), 1);

    let body = &t.provider.requests()[0].body;
    let names = tool_names(body);
    // Host tools first; the MCP `read_file` is suffixed away from the host one. No deferred
    // meta-tools: every server is preloaded.
    assert_eq!(
        names,
        [
            "read_file",
            "glob",
            "grep",
            "delete_everything",
            "echo",
            "env",
            "fail",
            "read_file1",
            "structured"
        ]
    );
    let echo = &body["tools"][4]["function"];
    assert_eq!(echo["description"], "mcp server: eager\nEchoes its text.");
    assert_eq!(
        echo["parameters"],
        json!({ "type": "object", "properties": { "text": { "type": "string" } }, "required": [] })
    );
    let prompt = body["messages"][0]["content"].as_str().unwrap();
    assert!(!prompt.contains("deferred-tools-instructions"));
}

#[tokio::test]
async fn a_deferred_server_starts_only_when_a_deferred_call_targets_it() {
    let markers = TempDir::new().unwrap();
    let t = open(&server_toml(
        "lazy",
        markers.path(),
        "description = \"A lazily started fixture server.\"",
    ))
    .await;
    let status = |state| {
        vec![McpServerStatus {
            name: "lazy".to_string(),
            state,
        }]
    };
    let session_id = t.harness.create_session().unwrap();

    // A turn that never touches MCP doesn't start the server, but offers the meta-tools.
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let events = collect(t.harness.start_turn(&session_id, "hello".into())).await;
    assert_done(&events);
    assert_eq!(starts(markers.path(), "lazy"), 0);
    assert_eq!(t.harness.mcp_status(), status(McpServerState::Idle));
    let body = &t.provider.requests()[0].body;
    assert_eq!(
        tool_names(body),
        [
            "read_file",
            "glob",
            "grep",
            "list_tools",
            "get_tool_info",
            "call_tool"
        ]
    );
    let prompt = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        prompt.contains(
            "<deferred-tools-instructions>\nThe following MCP servers have deferred tool loading. \
             Their tools are NOT pre-loaded.\n- lazy: \"A lazily started fixture server.\"\n\n\
             The Agent MUST use list_tools and get_tool_info to discover tools before calling \
             them.\n</deferred-tools-instructions>"
        ),
        "{prompt}"
    );

    // list_tools → get_tool_info → call_tool, all against the one connection.
    t.provider.push_sse(tool_calls_sse(&[(
        "call_1",
        "list_tools",
        json!({ "mcp_server": "lazy" }),
    )]));
    t.provider.push_sse(tool_calls_sse(&[(
        "call_2",
        "get_tool_info",
        json!({ "mcp_server": "lazy", "tool_name": "echo" }),
    )]));
    t.provider.push_sse(tool_calls_sse(&[(
        "call_3",
        "call_tool",
        json!({ "mcp_server": "lazy", "tool_name": "echo", "input": { "text": "deferred" } }),
    )]));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let events = collect(t.harness.start_turn(&session_id, "use lazy".into())).await;
    assert_done(&events);

    let responses = responses(&events);
    assert_eq!(
        responses[0].0,
        "lazy:\n  echo, fail, structured, env, read_file, delete_everything"
    );
    let info: Value = serde_json::from_str(&responses[1].0).unwrap();
    assert_eq!(info["description"], "Echoes its text.");
    assert_eq!(
        info["inputSchema"]["properties"],
        json!({ "text": { "type": "string" } })
    );
    assert_eq!(responses[2], ("echo: deferred".to_string(), false));
    assert_eq!(starts(markers.path(), "lazy"), 1);
    assert_eq!(
        t.harness.mcp_status(),
        status(McpServerState::Connected { tools: 6 })
    );
}

#[tokio::test]
async fn an_mcp_tool_denied_by_a_name_pattern_is_blocked() {
    let markers = TempDir::new().unwrap();
    let toml = format!(
        "{}\n{}\n[permission.mcp]\n\"*\" = \"deny\"\n\"echo\" = \"allow\"\n\"delete_*\" = \"deny\"\n",
        server_toml("eager", markers.path(), "preload = true"),
        server_toml("lazy", markers.path(), ""),
    );
    let t = open(&toml).await;
    t.provider.push_sse(tool_calls_sse(&[
        ("call_1", "delete_everything", json!({})),
        // Through the meta-tool, the permission check uses the underlying tool's name.
        (
            "call_2",
            "call_tool",
            json!({ "mcp_server": "lazy", "tool_name": "delete_everything" }),
        ),
        // The last matching pattern wins: `echo` is allowed after `*` denied everything.
        ("call_3", "echo", json!({ "text": "still allowed" })),
    ]));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_, events) = t.run_turn("delete everything").await;

    assert_done(&events);
    let blocked = "Blocked by guardrail: mcp permission `delete_*`".to_string();
    assert_eq!(
        responses(&events),
        [
            (blocked.clone(), true),
            (blocked, true),
            ("echo: still allowed".to_string(), false),
        ]
    );
    // A denied call is rejected before its server would be started.
    assert_eq!(starts(markers.path(), "lazy"), 0);
}

#[tokio::test]
async fn an_mcp_error_result_reaches_the_model_as_an_error() {
    let markers = TempDir::new().unwrap();
    let toml = format!(
        "{}\n{}",
        server_toml("eager", markers.path(), "preload = true"),
        server_toml("lazy", markers.path(), ""),
    );
    let t = open(&toml).await;
    t.provider.push_sse(tool_calls_sse(&[
        ("call_1", "fail", json!({})),
        (
            "call_2",
            "call_tool",
            json!({ "mcp_server": "lazy", "tool_name": "no_such_tool" }),
        ),
        (
            "call_3",
            "call_tool",
            json!({ "mcp_server": "missing", "tool_name": "echo" }),
        ),
    ]));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_, events) = t.run_turn("try the failing tool").await;

    assert_done(&events);
    let responses = responses(&events);
    assert_eq!(
        responses[0],
        (r#"{"error":"fixture failure"}"#.to_string(), false)
    );
    for (content, blocked) in &responses[1..] {
        let error: Value = serde_json::from_str(content).unwrap();
        assert!(error["error"].is_string(), "{content}");
        assert!(!blocked);
    }
    assert!(responses[1].0.contains("unknown tool: no_such_tool"));
    assert!(responses[2].0.contains("MCP server 'missing' not found"));

    // The error text is what the model sees as the tool message.
    let messages = t.provider.requests()[1].body["messages"].clone();
    let tool_message = messages
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == "call_1")
        .unwrap();
    assert_eq!(tool_message["content"], r#"{"error":"fixture failure"}"#);
}
