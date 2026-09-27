#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::TestHarness;
use harness_core::{Event, EventKind, Provider, TurnState, Usage};
use serde_json::{Value, json};

/// One DeepSeek-style SSE chunk.
fn chunk(delta: Value, finish_reason: Option<&str>, usage: Option<Value>) -> String {
    let body = json!({
        "id": "long-runs",
        "object": "chat.completion.chunk",
        "model": "deepseek-flash",
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish_reason }],
        "usage": usage,
    });
    format!("data: {body}\n\n")
}

fn usage(prompt_tokens: u64, completion_tokens: u64) -> Value {
    json!({
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": prompt_tokens + completion_tokens,
    })
}

/// A plain answer reporting the given usage.
fn answer_sse(content: &str, prompt_tokens: u64, completion_tokens: u64) -> String {
    chunk(
        json!({ "role": "assistant", "content": content }),
        None,
        None,
    ) + &chunk(
        json!({}),
        Some("stop"),
        Some(usage(prompt_tokens, completion_tokens)),
    ) + "data: [DONE]\n\n"
}

/// One reply requesting `(id, name, arguments)` tool calls, reporting the given usage.
fn calls_sse(calls: &[(&str, &str, &str)], prompt_tokens: u64) -> String {
    let mut sse = chunk(json!({ "role": "assistant", "content": "" }), None, None);
    for (index, (id, name, arguments)) in calls.iter().enumerate() {
        sse += &chunk(
            json!({ "tool_calls": [{
                "index": index,
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": arguments },
            }] }),
            None,
            None,
        );
    }
    sse + &chunk(
        json!({}),
        Some("tool_calls"),
        Some(usage(prompt_tokens, 10)),
    ) + "data: [DONE]\n\n"
}

fn read_call(id: &'static str, path: &str) -> (&'static str, &'static str, String) {
    (id, "read_file", json!({ "path": path }).to_string())
}

fn push_calls(t: &TestHarness, calls: &[(&str, &str, String)], prompt_tokens: u64) {
    let calls: Vec<(&str, &str, &str)> = calls
        .iter()
        .map(|(id, name, args)| (*id, *name, args.as_str()))
        .collect();
    t.provider.push_sse(calls_sse(&calls, prompt_tokens));
}

/// Writes a one-line file whose `read_file` output (`"     1\t" + line`) is `chars` long.
fn write_sized(t: &TestHarness, name: &str, chars: usize) -> String {
    let line: String = (0..chars - 7)
        .map(|i| char::from(b'a' + (i % 26) as u8))
        .collect();
    std::fs::write(t.workspace.join(name), &line).unwrap();
    format!("{:>6}\t{line}", 1)
}

fn tool_responses(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolResponse { content, .. } => Some(content.clone()),
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
        "expected turn.done{{state: done}}, got {:?}",
        events.last()
    );
}

fn messages(request: &common::Request) -> Vec<Value> {
    request.body["messages"].as_array().unwrap().clone()
}

/// Asserts `content` is a spill pointer whose file holds exactly `original`, and returns the path.
fn assert_spilled(t: &TestHarness, content: &str, original: &str) -> String {
    let path = content
        .strip_prefix("Content too large. Result saved to: ")
        .and_then(|rest| rest.split_once(".\n\n"))
        .map(|(path, _)| path.to_string())
        .unwrap_or_else(|| panic!("not a spill pointer: {content}"));
    assert!(path.starts_with(".banana-harness/tool-output/read_file_"));
    assert_eq!(
        std::fs::read_to_string(t.workspace.join(&path)).unwrap(),
        original
    );
    assert!(content.contains("read_file"));
    assert!(content.contains("grep"));
    let head: String = original.chars().take(100).collect();
    let tail: String = original
        .chars()
        .skip(original.chars().count() - 100)
        .collect();
    assert!(content.ends_with(&format!(
        "Preview (first and last 100 chars):\n{head}\n...\n{tail}"
    )));
    path
}

#[tokio::test]
async fn compaction_replaces_the_context_with_a_summary_once_usage_crosses_the_threshold() {
    let t = TestHarness::open_configured(Provider::DeepSeek, |config| {
        config.providers.deepseek.context_length = None;
    })
    .await;
    std::fs::write(t.workspace.join("notes.txt"), "remember the milk\n").unwrap();
    // 49,100 + 931 reaches the 50,000 fallback threshold, so the next LLM call compacts first.
    push_calls(&t, &[read_call("call_1", "notes.txt")], 49_090);
    t.provider.push_sse(answer_sse(
        "1. Primary Request: read notes.txt",
        49_200,
        300,
    ));
    t.provider.push_sse(answer_sse("Done.", 400, 5));

    let (_, events) = t.run_turn("what do my notes say?").await;
    assert_done(&events);

    let requests = t.provider.requests();
    assert_eq!(requests.len(), 3);

    // The summary call: the serialised transcript, then the summary prompt, and no tools.
    let summary_request = messages(&requests[1]);
    assert_eq!(summary_request.len(), 2);
    let transcript = summary_request[0]["content"].as_str().unwrap();
    assert!(transcript.starts_with("<0><user>:what do my notes say?<1><assistant>:"));
    assert!(transcript.contains(r#"<tool-call id=call_1 name=read_file>:{"path":"notes.txt"}"#));
    assert!(transcript.contains("<2><tool-response id=call_1>:"));
    assert!(transcript.contains("remember the milk"));
    let prompt = summary_request[1]["content"].as_str().unwrap();
    assert!(prompt.contains("1. Primary Request and Intent:"));
    assert!(prompt.contains("8. Optional Next Step:"));
    assert!(requests[1].body.get("tools").is_none());

    // The next model request holds only the system prompt, the summary and the continuation.
    let next = messages(&requests[2]);
    assert_eq!(next.len(), 3);
    assert_eq!(next[0]["role"], "system");
    assert_eq!(next[1]["role"], "assistant");
    assert_eq!(next[1]["content"], "1. Primary Request: read notes.txt");
    assert_eq!(next[2]["role"], "user");
    let continuation = next[2]["content"].as_str().unwrap();
    assert!(continuation.starts_with("<bh-internal>The above is the summarised context"));
    assert!(continuation.ends_with("</bh-internal>"));

    let compacted: Vec<&Usage> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ContextCompacted { usage, .. } => Some(usage),
            _ => None,
        })
        .collect();
    assert_eq!(compacted.len(), 1);
    assert_eq!(compacted[0].prompt_tokens, 300 + 42);

    // It is durable, like the other thread events.
    let store = t.reopen_store();
    let session = &store.sessions().unwrap()[0];
    let turn_id = &store.turns(&session.id).unwrap()[0].id;
    let kinds: Vec<_> = store
        .events(turn_id)
        .unwrap()
        .into_iter()
        .map(|r| r.kind)
        .collect();
    assert!(kinds.contains(&"context.compacted".to_string()));
}

#[tokio::test]
async fn the_threshold_is_the_smaller_of_80_percent_and_the_context_minus_max_output() {
    let t = TestHarness::open_configured(Provider::DeepSeek, |config| {
        config.providers.deepseek.context_length = Some(10_000);
        config.providers.deepseek.max_output = Some(4_000);
    })
    .await;
    std::fs::write(t.workspace.join("a.txt"), "a\n").unwrap();
    // 5,100 + 931 is under 80% (8,000) but reaches 10,000 - 4,000.
    push_calls(&t, &[read_call("call_1", "a.txt")], 5_090);
    t.provider.push_sse(answer_sse("summary", 5_200, 50));
    t.provider.push_sse(answer_sse("Done.", 100, 5));

    let (_, events) = t.run_turn("go").await;
    assert_done(&events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind, EventKind::ContextCompacted { .. }))
    );
    assert_eq!(messages(&t.provider.requests()[2])[1]["content"], "summary");
}

#[tokio::test]
async fn a_huge_tool_result_is_spilled_to_a_file_and_the_model_gets_its_path_and_a_preview() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let original = write_sized(&t, "big.txt", 40_000);
    push_calls(&t, &[read_call("call_1", "big.txt")], 50);
    t.provider.push_sse(answer_sse("Done.", 100, 5));

    let (_, events) = t.run_turn("read big.txt").await;
    assert_done(&events);

    let responses = tool_responses(&events);
    assert_spilled(&t, &responses[0], &original);
    // The model sees exactly what the event reported.
    let next = messages(&t.provider.requests()[1]);
    assert_eq!(next.last().unwrap()["content"], responses[0]);
}

#[tokio::test]
async fn a_spilled_result_redacts_the_api_key_before_writing_the_file() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let source = format!("{}{}", "a".repeat(30_000), common::TEST_API_KEY);
    std::fs::write(t.workspace.join("big.txt"), &source).unwrap();
    push_calls(&t, &[read_call("call_1", "big.txt")], 50);
    t.provider.push_sse(answer_sse("Done.", 100, 5));

    let (_, events) = t.run_turn("read big.txt").await;
    assert_done(&events);
    let responses = tool_responses(&events);
    let expected = format!("{:>6}\t{}[REDACTED]", 1, "a".repeat(30_000));
    let path = assert_spilled(&t, &responses[0], &expected);
    assert!(
        !std::fs::read_to_string(t.workspace.join(path))
            .unwrap()
            .contains(common::TEST_API_KEY)
    );
    assert!(
        !t.provider.requests()[1]
            .body
            .to_string()
            .contains(common::TEST_API_KEY)
    );
}

#[tokio::test]
async fn a_spill_leaves_git_status_clean_apart_from_the_agents_own_changes() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write_sized(&t, "big.txt", 40_000);
    push_calls(&t, &[read_call("call_1", "big.txt")], 50);
    t.provider.push_sse(answer_sse("Done.", 100, 5));

    let (_, events) = t.run_turn("read big.txt").await;
    assert_done(&events);
    assert!(t.workspace.join(".banana-harness/tool-output").is_dir());

    let status = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(&t.workspace)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(status.stdout).unwrap(), "?? big.txt\n");
}

#[tokio::test]
async fn a_mixed_batch_blocks_oversized_results_then_the_largest_until_under_10000_tokens() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    // ~7,100, ~5,000, ~4,500 and ~500 tokens.
    let a = write_sized(&t, "a.txt", 28_400);
    let b = write_sized(&t, "b.txt", 20_000);
    let c = write_sized(&t, "c.txt", 18_000);
    let d = write_sized(&t, "d.txt", 2_000);
    push_calls(
        &t,
        &[
            read_call("call_a", "a.txt"),
            read_call("call_b", "b.txt"),
            read_call("call_c", "c.txt"),
            read_call("call_d", "d.txt"),
        ],
        50,
    );
    t.provider.push_sse(answer_sse("Done.", 100, 5));

    let (_, events) = t.run_turn("read them all").await;
    assert_done(&events);

    let responses = tool_responses(&events);
    // `a` is over 6,000 on its own; blocking it still leaves ~10,100, so the largest of the
    // rest (`b`) goes too, and the total is then under 10,000.
    assert!(assert_spilled(&t, &responses[0], &a).contains("read_file_0_"));
    assert!(assert_spilled(&t, &responses[1], &b).contains("read_file_1_"));
    assert_eq!(responses[2], c);
    assert_eq!(responses[3], d);
}

#[tokio::test]
async fn a_batch_with_no_oversized_result_under_10000_tokens_passes_untouched() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    // ~4,000 + ~4,000: over 6,000 in total, but nothing is 6,000 alone and the total is
    // already under 10,000.
    let a = write_sized(&t, "a.txt", 16_000);
    let b = write_sized(&t, "b.txt", 16_000);
    push_calls(
        &t,
        &[read_call("call_a", "a.txt"), read_call("call_b", "b.txt")],
        50,
    );
    t.provider.push_sse(answer_sse("Done.", 100, 5));

    let (_, events) = t.run_turn("read both").await;
    assert_done(&events);
    assert_eq!(tool_responses(&events), [a, b]);
    assert!(!t.workspace.join(".banana-harness").exists());
}

#[tokio::test]
async fn a_huge_failure_is_truncated_to_500_characters_instead_of_spilled() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let name = "x".repeat(30_000);
    t.provider
        .push_sse(calls_sse(&[("call_1", &name, "{}")], 50));
    t.provider.push_sse(answer_sse("Done.", 100, 5));

    let (_, events) = t.run_turn("call it").await;
    assert_done(&events);

    let original = json!({ "error": format!("unknown tool \"{name}\"") }).to_string();
    let responses = tool_responses(&events);
    assert_eq!(responses[0], original.chars().take(500).collect::<String>());
    assert!(!t.workspace.join(".banana-harness").exists());
}
