#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{TestHarness, sse_text, sse_tool_calls};
use harness_core::{Event, EventKind, Provider, TurnState};
use serde_json::Value;

/// Slow enough that overlapping child requests are observable at the fake provider.
const CHILD_DELAY: Duration = Duration::from_millis(40);

fn spawn_args(name: &str, input: &str) -> String {
    serde_json::json!({ "name": name, "input": input }).to_string()
}

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

fn tool_response<'a>(events: &'a [Event], id: &str) -> &'a str {
    events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::ToolResponse {
                tool_call_id,
                content,
                ..
            } if tool_call_id == id => Some(content.as_str()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no tool.response for {id}: {events:?}"))
}

fn position(events: &[Event], pred: impl Fn(&EventKind) -> bool) -> usize {
    events.iter().position(|e| pred(&e.kind)).unwrap()
}

fn tool_names(request_body: &Value) -> Vec<String> {
    request_body["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .map(|t| t["function"]["name"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn the_root_spawns_two_concurrent_children_and_answers_with_their_results() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(sse_tool_calls(&[
        (
            "call_a",
            "create_sub_agent",
            &spawn_args("alpha", "Task alpha: count the apples"),
        ),
        (
            "call_b",
            "create_sub_agent",
            &spawn_args("beta", "Task beta: count the pears"),
        ),
    ]));
    t.provider
        .push_sse_matching("Task alpha", sse_text("three apples"), CHILD_DELAY);
    t.provider
        .push_sse_matching("Task beta", sse_text("five pears"), CHILD_DELAY);
    t.provider
        .push_sse(sse_text("Three apples and five pears."));

    let (session_id, events) = t.run_turn("count the fruit").await;
    assert_turn_done(&events);

    // Both children were in flight at once.
    assert_eq!(t.provider.max_in_flight(), 2);

    // Each child's answer is its own spawning call's result.
    assert_eq!(tool_response(&events, "call_a"), "three apples");
    assert_eq!(tool_response(&events, "call_b"), "five pears");

    // Children are announced with their parent and title, and finish before the parent
    // gets their results (the parent waits while it has live children).
    let root_thread = events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::ModelMessage { thread_id, .. } => Some(thread_id.clone()),
            _ => None,
        })
        .unwrap();
    let created: Vec<(String, String, String)> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ThreadCreated {
                thread_id,
                parent,
                title,
            } => Some((thread_id.clone(), parent.clone(), title.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(created.len(), 2);
    let mut titles: Vec<_> = created.iter().map(|c| c.2.as_str()).collect();
    titles.sort_unstable();
    assert_eq!(titles, ["alpha", "beta"]);
    assert!(
        created
            .iter()
            .all(|c| c.1 == root_thread && c.0 != root_thread)
    );

    let last_child_done = events
        .iter()
        .rposition(|e| matches!(e.kind, EventKind::ThreadDone { .. }))
        .unwrap();
    let first_result = position(&events, |k| matches!(k, EventKind::ToolResponse { .. }));
    assert!(last_child_done < first_result, "{events:?}");

    // Child events carry the child's thread id, fanned into the one stream.
    for (child, ..) in &created {
        assert!(events.iter().any(|e| matches!(&e.kind,
            EventKind::ModelMessage { thread_id, message: Some(_), .. } if thread_id == child)));
    }

    // Then the parent answers, on its own thread, with the children's results in context.
    let final_answer = events
        .iter()
        .rev()
        .find_map(|e| match &e.kind {
            EventKind::ModelMessage {
                thread_id,
                message: Some(m),
                ..
            } => Some((thread_id.clone(), m.content.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        final_answer,
        (
            root_thread.clone(),
            "Three apples and five pears.".to_string()
        )
    );
    let requests = t.provider.requests();
    let last = &requests.last().unwrap().body["messages"];
    let tool_messages: Vec<_> = last
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| (m["tool_call_id"].clone(), m["content"].clone()))
        .collect();
    assert_eq!(
        tool_messages,
        [
            ("call_a".into(), "three apples".into()),
            ("call_b".into(), "five pears".into())
        ]
    );

    // A child's request is its subagent identity prompt plus its task, nothing else.
    let child = requests
        .iter()
        .find(|r| {
            r.body.to_string().contains("Task alpha")
                && r.body["messages"].as_array().unwrap().len() == 2
        })
        .expect("child alpha's request");
    let messages = child.body["messages"].as_array().unwrap();
    assert!(
        messages[0]["content"]
            .as_str()
            .unwrap()
            .contains("sub-agent")
    );
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[1]["content"], "Task alpha: count the apples");

    // Each child gets its own thread row under the root, titled by its name.
    let store = t.reopen_store();
    let threads = store.threads(&session_id).unwrap();
    assert_eq!(threads.len(), 3);
    let children: Vec<_> = threads.iter().filter(|th| th.parent_id.is_some()).collect();
    assert_eq!(children.len(), 2);
    assert!(
        children
            .iter()
            .all(|th| th.parent_id.as_deref() == Some(root_thread.as_str()))
    );
    assert!(
        children
            .iter()
            .any(|th| th.title.as_deref() == Some("alpha"))
    );
    // The session still resumes from the root thread.
    assert_eq!(
        store.root_thread(&session_id).unwrap().unwrap().id,
        root_thread
    );
}

#[tokio::test]
async fn at_most_five_children_run_at_once() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let ids: Vec<String> = (0..7).map(|i| format!("call_{i}")).collect();
    let args: Vec<String> = (0..7)
        .map(|i| spawn_args(&format!("worker {i}"), &format!("Task {i}")))
        .collect();
    let calls: Vec<(&str, &str, &str)> = ids
        .iter()
        .zip(&args)
        .map(|(id, a)| (id.as_str(), "create_sub_agent", a.as_str()))
        .collect();
    t.provider.push_sse(sse_tool_calls(&calls));
    for _ in 0..7 {
        t.provider
            .push_sse_matching("Task ", sse_text("done"), CHILD_DELAY);
    }
    t.provider.push_sse(sse_text("All seven done."));

    let (_, events) = t.run_turn("fan out").await;
    assert_turn_done(&events);

    assert_eq!(t.provider.max_in_flight(), 5);
    let done = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::ThreadDone { .. }))
        .count();
    assert_eq!(done, 7);
    for id in &ids {
        assert_eq!(tool_response(&events, id), "done");
    }
}

#[tokio::test]
async fn a_child_cannot_spawn_subagents_or_ask_questions() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(sse_tool_calls(&[(
        "call_root",
        "create_sub_agent",
        &spawn_args("gamma", "Task gamma"),
    )]));
    // The child tries anyway, with both root-only tools.
    t.provider.push_sse_matching(
        "Task gamma",
        sse_tool_calls(&[
            (
                "call_nested",
                "create_sub_agent",
                &spawn_args("nested", "Task nested"),
            ),
            (
                "call_ask",
                "ask_user_question",
                r#"{"question":"which?","options":[]}"#,
            ),
        ]),
        Duration::ZERO,
    );
    t.provider
        .push_sse_matching("Task gamma", sse_text("gamma done"), Duration::ZERO);
    t.provider.push_sse(sse_text("ok"));

    let (_, events) = t.run_turn("delegate").await;
    assert_turn_done(&events);

    let requests = t.provider.requests();
    assert!(tool_names(&requests[0].body).contains(&"create_sub_agent".to_string()));
    let child_tools = tool_names(&requests[1].body);
    assert!(child_tools.contains(&"read_file".to_string()));
    assert!(!child_tools.contains(&"create_sub_agent".to_string()));
    assert!(!child_tools.contains(&"ask_user_question".to_string()));

    // The calls fail like any unknown tool, and no grandchild is created.
    assert!(tool_response(&events, "call_nested").contains("unknown tool"));
    assert!(tool_response(&events, "call_ask").contains("unknown tool"));
    let created = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::ThreadCreated { .. }))
        .count();
    assert_eq!(created, 1);
    assert_eq!(tool_response(&events, "call_root"), "gamma done");
}

#[tokio::test]
async fn a_failing_child_becomes_an_error_result_and_the_parent_continues() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(sse_tool_calls(&[(
        "call_root",
        "create_sub_agent",
        &spawn_args("delta", "Task delta"),
    )]));
    t.provider.push_sse_matching(
        "Task delta",
        common::fixture("deepseek_length.sse"),
        Duration::ZERO,
    );
    t.provider.push_sse(sse_text("the subagent failed"));

    let (_, events) = t.run_turn("delegate").await;
    assert_turn_done(&events);

    let result: Value = serde_json::from_str(tool_response(&events, "call_root")).unwrap();
    assert_eq!(result["error"], "max_tokens breached");
    assert!(events.iter().any(|e| matches!(&e.kind,
        EventKind::ThreadDone { error: Some(error), .. } if error == "max_tokens breached")));
}
