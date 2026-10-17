#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;
#[cfg(unix)]
use std::os::unix::fs::symlink;

use common::TestHarness;
use harness_core::{Event, EventKind, Provider, TurnState};
use harness_testkit::fixture;

fn names(events: &[Event]) -> Vec<&'static str> {
    events.iter().map(|e| e.kind.name()).collect()
}

/// Asserts the turn ran to a normal `done`, proving a blocked or successful tool call doesn't
/// abort the turn — the model still gets to answer afterward.
fn assert_turn_done(events: &[Event]) {
    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(EventKind::TurnDone {
                state: TurnState::Done,
                error: None
            })
        ),
        "expected turn.done{{state: done}}, got {:?}",
        events.last()
    );
}

fn tool_calls(events: &[Event]) -> Vec<(&str, &str, &str)> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolCall {
                tool_call_id,
                name,
                arguments,
                ..
            } => Some((tool_call_id.as_str(), name.as_str(), arguments.as_str())),
            _ => None,
        })
        .collect()
}

/// Builds the expected `read_file` output for lines starting at 1, matching the tool's own
/// `"{:>6}\t{line}"` numbering exactly rather than a hand-counted literal.
fn numbered(lines: &[&str]) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>6}\t{line}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_responses(events: &[Event]) -> Vec<(&str, &str, bool)> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolResponse {
                tool_call_id,
                content,
                blocked,
                ..
            } => Some((tool_call_id.as_str(), content.as_str(), *blocked)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn reads_a_file_in_the_workspace_and_then_answers() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::write(
        t.workspace.join("hello.txt"),
        "line one\nline two\nline three\n",
    )
    .unwrap();
    t.provider
        .push_sse(fixture("deepseek_read_call.sse").replace("__PATH__", "hello.txt"));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (session_id, events) = t.run_turn("what does hello.txt say?").await;

    assert_eq!(
        names(&events),
        [
            "turn.created",
            "model.message",
            "model.message",
            "tool.call",
            "tool.response",
            "model.message",
            "model.message.delta",
            "model.message.delta",
            "model.message",
            "turn.done",
        ]
    );
    let calls = tool_calls(&events);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1, "read_file");
    assert_eq!(calls[0].2, r#"{"path": "hello.txt"}"#);

    let responses = tool_responses(&events);
    assert_eq!(responses.len(), 1);
    let (tool_call_id, content, blocked) = responses[0];
    assert_eq!(tool_call_id, calls[0].0);
    assert!(!blocked);
    assert_eq!(content, numbered(&["line one", "line two", "line three"]));

    // The tool's request and response are durable, read back exactly as streamed.
    let store = t.reopen_store();
    let turn_id = &store.turns(&session_id).unwrap()[0].id;
    let rows = store.events(turn_id).unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| r.kind.clone()).collect();
    assert!(kinds.contains(&"tool.call".to_string()));
    assert!(kinds.contains(&"tool.response".to_string()));

    // Tool names are sanitised and schemas keep only `type`/`properties`/`required`.
    let request = &t.provider.requests()[0];
    let tools = request.body["tools"].as_array().unwrap();
    let tool_names: HashSet<_> = tools
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        tool_names,
        HashSet::from([
            "read_file",
            "write_file",
            "edit",
            "glob",
            "grep",
            "bash",
            "create_sub_agent",
            "ask_question",
        ])
    );
    for tool in tools {
        assert_eq!(tool["type"], "function");
        let parameters = tool["function"]["parameters"].as_object().unwrap();
        let keys: HashSet<_> = parameters.keys().map(String::as_str).collect();
        assert_eq!(keys, HashSet::from(["type", "properties", "required"]));
    }
}

#[tokio::test]
async fn a_read_outside_the_workspace_is_blocked() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let outside = t.workspace.parent().unwrap().join("outside.txt");
    std::fs::write(&outside, "secret").unwrap();
    t.provider
        .push_sse(fixture("deepseek_read_call.sse").replace("__PATH__", "../outside.txt"));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_session_id, events) = t.run_turn("read ../outside.txt").await;

    let responses = tool_responses(&events);
    assert_eq!(responses.len(), 1);
    let (_, content, blocked) = responses[0];
    assert!(blocked);
    assert_eq!(content, "Blocked by guardrail: workspace confinement");
    assert_turn_done(&events);
}

#[tokio::test]
#[cfg(unix)]
async fn a_symlink_that_escapes_the_workspace_is_blocked() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let secret = t.workspace.parent().unwrap().join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();
    symlink("../secret.txt", t.workspace.join("escape_link")).unwrap();
    t.provider
        .push_sse(fixture("deepseek_read_call.sse").replace("__PATH__", "escape_link"));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_session_id, events) = t.run_turn("read escape_link").await;

    let responses = tool_responses(&events);
    assert_eq!(responses.len(), 1);
    let (_, content, blocked) = responses[0];
    assert!(blocked);
    assert_eq!(content, "Blocked by guardrail: workspace confinement");
    assert_turn_done(&events);
}

#[tokio::test]
async fn reading_env_is_blocked() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::write(t.workspace.join(".env"), "AI_API_KEY=sk-should-not-be-read").unwrap();
    t.provider
        .push_sse(fixture("deepseek_read_call.sse").replace("__PATH__", ".env"));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_session_id, events) = t.run_turn("read .env").await;

    let responses = tool_responses(&events);
    assert_eq!(responses.len(), 1);
    let (_, content, blocked) = responses[0];
    assert!(blocked);
    assert_eq!(content, r#"Blocked by guardrail: read rule ".env*""#);
    assert_turn_done(&events);
}

#[tokio::test]
async fn two_tool_calls_in_one_message_both_execute_before_the_next_model_call() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::write(t.workspace.join("a.txt"), "hello\n").unwrap();
    std::fs::write(t.workspace.join("b.txt"), "world\n").unwrap();
    t.provider.push_sse(fixture("deepseek_two_calls.sse"));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_session_id, events) = t.run_turn("look at both files").await;

    // Both calls start before either result comes back.
    let call_positions: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, EventKind::ToolCall { .. }))
        .map(|(i, _)| i)
        .collect();
    let response_positions: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, EventKind::ToolResponse { .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(call_positions.len(), 2);
    assert_eq!(response_positions.len(), 2);
    assert!(call_positions[1] < response_positions[0]);

    let calls = tool_calls(&events);
    assert_eq!(calls[0].1, "read_file");
    assert_eq!(calls[1].1, "glob");

    let responses = tool_responses(&events);
    assert!(!responses[0].2);
    assert_eq!(responses[0].1, numbered(&["hello"]));
    assert!(!responses[1].2);
    assert!(responses[1].1.contains("a.txt"));
    assert!(responses[1].1.contains("b.txt"));
    assert_turn_done(&events);
}

#[tokio::test]
async fn glob_and_grep_respect_gitignore() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::write(t.workspace.join(".gitignore"), "ignored.txt\n").unwrap();
    std::fs::write(t.workspace.join("ignored.txt"), "needle\n").unwrap();
    std::fs::write(t.workspace.join("visible.txt"), "needle\n").unwrap();

    t.provider.push_sse(fixture("deepseek_glob_call.sse"));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, glob_events) = t.run_turn("find every file").await;
    let glob_result = &tool_responses(&glob_events)[0];
    assert!(!glob_result.2);
    assert!(glob_result.1.contains("visible.txt"));
    assert!(!glob_result.1.contains("ignored.txt"));

    t.provider.push_sse(fixture("deepseek_grep_call.sse"));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, grep_events) = t.run_turn("find needle").await;
    let grep_result = &tool_responses(&grep_events)[0];
    assert!(!grep_result.2);
    assert!(grep_result.1.contains("visible.txt"));
    assert!(!grep_result.1.contains("ignored.txt"));
}

#[tokio::test]
async fn qwen_assembles_a_tool_call_despite_the_empty_continuation_id() {
    let t = TestHarness::open(Provider::Qwen).await;
    std::fs::write(t.workspace.join("hello.txt"), "hi\n").unwrap();
    t.provider
        .push_sse(fixture("qwen_read_call.sse").replace("__PATH__", "hello.txt"));
    t.provider.push_sse(fixture("qwen_plain.sse"));

    let (_session_id, events) = t.run_turn("what does hello.txt say?").await;

    let calls = tool_calls(&events);
    assert_eq!(calls.len(), 1);
    // The id must survive the second chunk's `id: ""`, and the arguments split across two
    // chunks must still parse as one JSON object.
    assert_eq!(calls[0].0, "call_qwen_1");
    assert_eq!(calls[0].2, r#"{"path": "hello.txt"}"#);

    let responses = tool_responses(&events);
    assert!(!responses[0].2);
    assert_eq!(responses[0].1, numbered(&["hi"]));
}
