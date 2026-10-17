//! Turns, events, required actions and undo, driven by the testkit's scenarios and fake provider.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::time::Duration;

use common::{API_KEY, Run, golden, scenario};
use harness_testkit::{ReplySpec, Scenario, SseSpec, ToolCallSpec, run_scenario};
use serde_json::{Value, json};

fn gated_text(gate: &str) -> ReplySpec {
    ReplySpec {
        sse: Some(SseSpec {
            text: Some("slow".to_string()),
            tool_calls: Vec::new(),
        }),
        gate: Some(gate.to_string()),
        gate_after_parts: Some(2),
        ..ReplySpec::default()
    }
}

fn tool_call(id: &str, name: &str, arguments: Value) -> ReplySpec {
    ReplySpec {
        sse: Some(SseSpec {
            text: None,
            tool_calls: vec![ToolCallSpec {
                id: id.to_string(),
                name: name.to_string(),
                arguments,
            }],
        }),
        ..ReplySpec::default()
    }
}

fn text(content: &str) -> ReplySpec {
    ReplySpec {
        sse: Some(SseSpec {
            text: Some(content.to_string()),
            tool_calls: Vec::new(),
        }),
        ..ReplySpec::default()
    }
}

#[tokio::test]
async fn a_second_turn_is_refused_while_one_runs_and_the_next_can_start_at_once() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub.push_spec(&gated_text("hold")).unwrap();
    let session = run.open_and_create().await;
    assert_eq!(run.active_turn().await, Value::Null);

    assert_eq!(run.start_turn(&session, "first").await["ok"], true);
    let created = run.event("turn.created").await;
    let turn_id = created["event"]["turn_id"].as_str().unwrap().to_string();
    assert_eq!(created["turn_id"], turn_id.as_str());

    let refused = run.start_turn(&session, "second").await;
    assert_eq!(refused["error"]["code"], "turn_in_progress");
    assert_eq!(refused["error"]["data"]["session_id"], session.as_str());
    assert_eq!(
        run.active_turn().await,
        json!({"session_id": session, "turn_id": turn_id, "pending_action_ids": []})
    );

    run.stub.release("hold");
    assert_eq!(run.done_state().await, "done");
    // The closing frame has been seen, so the workspace is already free.
    for n in 0..10 {
        run.stub.push_sse(harness_testkit::sse_text("again"));
        assert_eq!(
            run.start_turn(&session, &format!("turn {n}")).await["ok"],
            true,
            "round {n}"
        );
        assert_eq!(run.done_state().await, "done");
    }
    assert_eq!(run.active_turn().await, Value::Null);
}

#[tokio::test]
async fn cancel_ends_the_turn_cancelled_and_reports_whether_it_found_one() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub.push_spec(&gated_text("never")).unwrap();
    let session = run.open_and_create().await;
    let workspace = run.workspace();
    let params = json!({"workspace": workspace, "session_id": session});

    assert_eq!(
        run.call("turn.cancel", params.clone()).await["result"],
        json!({"cancelled": false})
    );
    run.start_turn(&session, "go").await;
    run.event("turn.created").await;
    run.wait_for_requests(1).await;
    assert_eq!(
        run.call("turn.cancel", params).await["result"],
        json!({"cancelled": true})
    );
    assert_eq!(run.done_state().await, "cancelled");
    assert_eq!(run.active_turn().await, Value::Null);

    // A cancelled turn is never resent: the next turn is a new request with new input.
    run.stub.push_sse(harness_testkit::sse_text("fresh"));
    run.start_turn(&session, "new input").await;
    assert_eq!(run.done_state().await, "done");
    assert_eq!(run.stub.requests().len(), 2);
}

#[tokio::test]
async fn pending_actions_are_tracked_validated_and_denied_by_cancel() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub
        .push_spec(&tool_call(
            "c1",
            "write_file",
            json!({"path": "a.txt", "content": "x"}),
        ))
        .unwrap();
    let session = run.open_and_create().await;
    let workspace = run.workspace();
    let reply = |action: &str| json!({"workspace": workspace, "session_id": session, "action_id": action, "answer": "allow"});

    assert_eq!(
        run.call("action.reply", reply("nope")).await["error"]["code"],
        "no_active_turn"
    );
    run.start_turn(&session, "write it").await;
    let approval = run.event("tool.approval_required").await;
    let action = approval["event"]["action_id"].as_str().unwrap().to_string();
    assert_eq!(
        run.active_turn().await["pending_action_ids"],
        json!([action])
    );

    assert_eq!(
        run.call("action.reply", reply("not-pending")).await["error"]["code"],
        "unknown_action"
    );
    let bad = run
        .call("action.reply", json!({"workspace": workspace, "session_id": session, "action_id": action, "answer": "maybe"}))
        .await;
    assert_eq!(bad["error"]["code"], "invalid_params");

    run.call(
        "turn.cancel",
        json!({"workspace": workspace, "session_id": session}),
    )
    .await;
    let after = run.active_turn().await;
    assert!(
        after.is_null() || after["pending_action_ids"] == json!([]),
        "{after}"
    );
    assert_eq!(run.done_state().await, "cancelled");
    assert!(
        !run.env.workspace().join("a.txt").exists(),
        "a cancelled approval ran the tool"
    );
    // Answering after the turn ended finds nothing running.
    assert_eq!(
        run.call("action.reply", reply(&action)).await["error"]["code"],
        "no_active_turn"
    );
}

#[tokio::test]
async fn allow_for_session_skips_the_prompt_next_time() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub
        .push_spec(&tool_call(
            "c1",
            "write_file",
            json!({"path": "a.txt", "content": "a"}),
        ))
        .unwrap();
    run.stub
        .push_spec(&tool_call(
            "c2",
            "write_file",
            json!({"path": "b.txt", "content": "b"}),
        ))
        .unwrap();
    run.stub.push_spec(&text("both written")).unwrap();
    let session = run.open_and_create().await;
    let workspace = run.workspace();

    run.start_turn(&session, "write two files").await;
    let approval = run.event("tool.approval_required").await;
    run.call(
        "action.reply",
        json!({"workspace": workspace, "session_id": session, "action_id": approval["event"]["action_id"], "answer": "allow_for_session"}),
    )
    .await;
    // No second prompt: the turn runs to its end.
    assert_eq!(run.done_state().await, "done");
    assert_eq!(
        std::fs::read_to_string(run.env.workspace().join("b.txt")).unwrap(),
        "b"
    );
}

#[tokio::test]
async fn undo_is_refused_while_a_turn_runs_and_reports_nothing_to_undo() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub.push_spec(&gated_text("hold")).unwrap();
    let session = run.open_and_create().await;
    let workspace = run.workspace();
    let undo = json!({"workspace": workspace, "session_id": session});

    assert_eq!(
        run.call("session.undo", undo.clone()).await["result"],
        json!({"outcome": "nothing_to_undo"})
    );
    run.start_turn(&session, "go").await;
    run.event("turn.created").await;
    let refused = run.call("session.undo", undo.clone()).await;
    assert_eq!(refused["error"]["code"], "turn_in_progress");
    assert_eq!(refused["error"]["data"]["session_id"], session.as_str());
    run.stub.release("hold");
    run.done_state().await;
    assert_eq!(
        run.call("session.undo", undo).await["result"]["outcome"],
        "nothing_to_undo"
    );
}

#[tokio::test]
async fn live_events_and_the_replayed_transcript_agree_including_the_undo_marker() {
    let (mut run, scenario) = Run::for_scenario("edit-and-undo").await;
    let workspace = run.env.workspace().to_path_buf();
    let outcome = run_scenario(&mut run.client, &scenario, &run.stub, &workspace)
        .await
        .unwrap();
    let session = outcome.events[0]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let ws = run.workspace();
    let resumed = run
        .call(
            "session.resume",
            json!({"workspace": ws, "session_id": session}),
        )
        .await;
    let turn = &resumed["result"]["turns"][0];
    assert_eq!(turn["state"], "done");

    // The stored events are exactly the live ones that are durable, in order, plus the undo marker.
    let live: Vec<&str> = outcome
        .events
        .iter()
        .filter(|f| {
            let t = f["event"]["type"].as_str().unwrap();
            t != "model.message.delta"
                && !(t == "model.message" && f["event"].get("message").is_none())
        })
        .map(|f| f["event"]["id"].as_str().unwrap())
        .collect();
    let stored: Vec<&str> = turn["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    let (marker, stored_live) = stored.split_last().unwrap();
    assert_eq!(&live, stored_live);
    assert_eq!(
        turn["events"].as_array().unwrap().last().unwrap()["type"],
        "turn.undone"
    );
    assert!(!marker.is_empty());
    assert_eq!(
        std::fs::read_to_string(run.env.workspace().join("hello.txt")).unwrap(),
        "hello world\n"
    );
}

#[tokio::test]
async fn closing_a_workspace_cancels_only_its_turn() {
    let (mut first, _) = Run::for_scenario("plain-answer").await;
    first.stub.reset();
    first.stub.push_spec(&gated_text("a")).unwrap();
    let session_a = first.open_and_create().await;
    let workspace_a = first.workspace();

    // A second workspace in the same bridge, served by the same stub.
    let other = first.client.dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let workspace_b = other.canonicalize().unwrap().display().to_string();
    first.stub.push_spec(&gated_text("b")).unwrap();
    first
        .call("workspace.open", json!({"path": workspace_b}))
        .await;
    let created = first
        .call("session.create", json!({"workspace": workspace_b}))
        .await;
    let session_b = created["result"]["session"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    first.start_turn(&session_a, "in a").await;
    first.event("turn.created").await;
    first.wait_for_requests(1).await;
    first
        .call(
            "turn.start",
            json!({"workspace": workspace_b, "session_id": session_b, "input": "in b"}),
        )
        .await;
    first.event("turn.created").await;
    first.wait_for_requests(2).await;

    first
        .call("workspace.close", json!({"workspace": workspace_a}))
        .await;
    // A's turn ends cancelled and is routed to A; B is untouched and still running.
    let done = first.event("turn.done").await;
    assert_eq!(done["workspace"], workspace_a.as_str());
    assert_eq!(done["event"]["state"], "cancelled");
    let listed = first.call("workspace.list", json!({})).await;
    let open = listed["result"]["workspaces"].as_array().unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0]["workspace"], workspace_b.as_str());
    assert_eq!(open[0]["active_turn"]["session_id"], session_b.as_str());

    first.stub.release("b");
    let done = first.event("turn.done").await;
    assert_eq!(
        (
            done["workspace"].as_str().unwrap(),
            done["event"]["state"].as_str().unwrap()
        ),
        (workspace_b.as_str(), "done")
    );
}

#[tokio::test]
async fn shutdown_cancels_running_turns_and_flushes_their_closing_frames() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub.push_spec(&gated_text("never")).unwrap();
    let session = run.open_and_create().await;
    run.start_turn(&session, "go").await;
    run.event("turn.created").await;

    run.client.send("bye", "shutdown", json!({})).await;
    let mut saw_done = false;
    let started = std::time::Instant::now();
    while let Some(frame) = run.client.try_next().await {
        if frame["kind"] == "event" && frame["event"]["type"] == "turn.done" {
            assert_eq!(frame["event"]["state"], "cancelled");
            saw_done = true;
        }
    }
    assert!(saw_done, "the cancelled turn's closing frame was lost");
    assert!(started.elapsed() < Duration::from_secs(5));
    run.client.finish().await.unwrap();
}

#[tokio::test]
async fn an_untrusted_workspace_config_cannot_start_an_mcp_command_during_a_turn() {
    let mut results = Vec::new();
    for trusted in [false, true] {
        let (mut run, _) = Run::for_scenario("plain-answer").await;
        run.stub.reset();
        run.stub.push_sse(harness_testkit::sse_text("done"));
        let marker = run.env.marker();
        let dir = run.env.workspace().join(".banana-harness");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            format!(
                "[mcp.evil]\ntype = \"local\"\npreload = true\ncommand = [\"sh\", \"-c\", \"touch {}\"]\n",
                marker.display()
            ),
        )
        .unwrap();
        let workspace = run.workspace();
        run.call(
            "workspace.open",
            json!({"path": workspace, "trust_workspace_config": trusted}),
        )
        .await;
        let created = run
            .call("session.create", json!({"workspace": workspace}))
            .await;
        let session = created["result"]["session"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        run.start_turn(&session, "hello").await;
        run.done_state().await;
        results.push(marker.exists());
    }
    // The trusted run proves the check can see a started command at all.
    assert_eq!(results, [false, true]);
}

#[tokio::test]
async fn the_key_never_reaches_a_frame_or_the_database_even_when_a_tool_echoes_it() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub
        .push_spec(&tool_call(
            "c1",
            "bash",
            json!({"command": format!("echo {API_KEY}")}),
        ))
        .unwrap();
    run.stub.push_spec(&text("echoed")).unwrap();
    let session = run.open_and_create().await;
    let workspace = run.workspace();
    run.start_turn(&session, &format!("my key is {API_KEY}"))
        .await;
    let approval = run.event("tool.approval_required").await;
    run.call(
        "action.reply",
        json!({"workspace": workspace, "session_id": session, "action_id": approval["event"]["action_id"], "answer": "allow"}),
    )
    .await;
    run.done_state().await;

    let received: Vec<&Value> = run
        .client
        .log
        .iter()
        .filter(|f| f["kind"] != "request")
        .collect();
    assert!(
        !serde_json::to_string(&received).unwrap().contains(API_KEY),
        "the key reached a frame"
    );
    assert!(
        serde_json::to_string(&received)
            .unwrap()
            .contains("[REDACTED]")
    );
    for name in ["harness.db", "harness.db-wal"] {
        if let Ok(bytes) = std::fs::read(run.client.db_path.with_file_name(name)) {
            assert!(
                !bytes
                    .windows(API_KEY.len())
                    .any(|w| w == API_KEY.as_bytes()),
                "the key is in {name}"
            );
        }
    }
}

#[tokio::test]
async fn turn_requests_are_validated() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    let session = run.open_and_create().await;
    let workspace = run.workspace();
    for (params, code) in [
        (
            json!({"workspace": workspace, "session_id": session, "input": "  "}),
            "invalid_params",
        ),
        (
            json!({"workspace": workspace, "session_id": "missing", "input": "hi"}),
            "session_not_found",
        ),
        (
            json!({"workspace": "/not/open", "session_id": session, "input": "hi"}),
            "workspace_not_open",
        ),
        (
            json!({"workspace": workspace, "input": "hi"}),
            "invalid_params",
        ),
    ] {
        assert_eq!(run.call("turn.start", params).await["error"]["code"], code);
    }
    assert!(
        run.stub.requests().is_empty(),
        "a rejected turn reached the provider"
    );
    assert_eq!(
        run.call(
            "turn.cancel",
            json!({"workspace": "/not/open", "session_id": "s"})
        )
        .await["error"]["code"],
        "workspace_not_open"
    );
}

#[tokio::test]
async fn golden_frames_for_turns_approvals_and_undo() {
    let mut frames = Vec::new();
    for name in [
        "plain-answer",
        "edit-and-approve",
        "question-and-answer",
        "subagents",
        "edit-and-undo",
    ] {
        let (mut run, scenario) = Run::for_scenario(name).await;
        let workspace = run.env.workspace().to_path_buf();
        run_scenario(&mut run.client, &scenario, &run.stub, &workspace)
            .await
            .unwrap();
        frames.extend(normalise_run(&run, name));
    }

    // One subagent, not the parallel pair, so thread events arrive in a fixed order.
    let mut scenario = scenario("plain-answer");
    scenario.name = "single-subagent".to_string();
    scenario.expect = harness_testkit::Expect::default();
    scenario.replies = vec![
        tool_call(
            "call_a",
            "create_sub_agent",
            json!({"name": "alpha", "input": "task-alpha: say alpha"}),
        ),
        ReplySpec {
            matching: Some("task-alpha: say alpha".to_string()),
            ..text("alpha done")
        },
        text("The subagent finished."),
    ];
    let mut run = Run::new(&scenario.workspace_template()).await;
    scenario.script(&run.stub).unwrap();
    let workspace = run.env.workspace().to_path_buf();
    run_scenario(&mut run.client, &scenario, &run.stub, &workspace)
        .await
        .unwrap();
    frames.extend(normalise_run(&run, "single-subagent"));

    // A small context window makes the first call compact the context before the answer.
    let scenario = Scenario {
        name: "compaction".to_string(),
        replies: vec![text("summary of the work so far"), text("All done.")],
        expect: harness_testkit::Expect::default(),
        ..self::scenario("plain-answer")
    };
    let mut run = Run::new(&scenario.workspace_template()).await;
    let config = Path::new(&run.env.vars()["XDG_CONFIG_HOME"]).join("banana-harness/config.toml");
    let text_config = std::fs::read_to_string(&config).unwrap().replacen(
        "[providers.deepseek]\n",
        "[providers.deepseek]\ncontext_length = 1000\nmax_output = 1\n",
        1,
    );
    std::fs::write(&config, text_config).unwrap();
    scenario.script(&run.stub).unwrap();
    let workspace = run.env.workspace().to_path_buf();
    run_scenario(&mut run.client, &scenario, &run.stub, &workspace)
        .await
        .unwrap();
    assert!(
        run.client
            .log
            .iter()
            .any(|f| f["event"]["type"] == "context.compacted")
    );
    frames.extend(normalise_run(&run, "compaction"));
    golden("turns", &frames);
}

/// One run's frames, minus ready and the replaceable parts: temp paths, ids and times.
fn normalise_run(run: &Run, name: &str) -> Vec<Value> {
    use std::collections::HashMap;
    // Ids are numbered in order of first appearance, so the file is stable run to run.
    fn walk(value: &mut Value, ids: &mut HashMap<String, String>, paths: &[(String, &str)]) {
        match value {
            Value::String(text) => {
                for (needle, replacement) in paths {
                    *text = text.replace(needle.as_str(), replacement);
                }
                let looks_like_id =
                    text.len() == 26 && text.chars().all(|c| c.is_ascii_alphanumeric());
                if looks_like_id {
                    let next = ids.len();
                    *text = ids
                        .entry(text.clone())
                        .or_insert_with(|| format!("<id-{next}>"))
                        .clone();
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| walk(v, ids, paths)),
            Value::Object(map) => {
                for (key, v) in map.iter_mut() {
                    if matches!(key.as_str(), "created_at" | "updated_at") {
                        *v = json!("<time>");
                    } else {
                        walk(v, ids, paths);
                    }
                }
            }
            _ => {}
        }
    }
    let paths = [
        (run.workspace(), "<workspace>"),
        (run.env.root().display().to_string(), "<root>"),
    ];
    let mut ids = HashMap::new();
    let mut out = vec![json!({"scenario": name})];
    for frame in run.client.log.iter().skip(1) {
        let mut frame = frame.clone();
        // Request ids are the driver's own counter; keep them, but number everything else.
        walk(&mut frame, &mut ids, &paths);
        out.push(frame);
    }
    out
}

#[tokio::test]
async fn golden_frames_for_turn_errors_and_cancel() {
    let (mut run, _) = Run::for_scenario("plain-answer").await;
    run.stub.reset();
    run.stub.push_spec(&gated_text("hold")).unwrap();
    let session = run.open_and_create().await;
    let workspace = run.workspace();
    run.start_turn(&session, "slow one").await;
    run.event("turn.created").await;
    run.wait_for_requests(1).await;
    run.start_turn(&session, "overlapping").await;
    run.call(
        "action.reply",
        json!({"workspace": workspace, "session_id": session, "action_id": "nope", "answer": "allow"}),
    )
    .await;
    run.call(
        "session.undo",
        json!({"workspace": workspace, "session_id": session}),
    )
    .await;
    run.call(
        "turn.cancel",
        json!({"workspace": workspace, "session_id": session}),
    )
    .await;
    run.done_state().await;
    // Streaming events arrive at timing-dependent points between the responses; leave them out.
    run.client.log.retain(|f| {
        !f["event"]["type"]
            .as_str()
            .is_some_and(|t| t.starts_with("model.message"))
    });
    golden("turn-errors", &normalise_run(&run, "turn-errors"));
}
