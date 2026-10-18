//! A running `bash` command and the turn's cancellation, through a real turn.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use common::{TestHarness, collect};
use harness_core::{EventKind, Provider, TurnState};
use harness_testkit::fixture;
use serde_json::json;

fn tool_call(name: &str, arguments: &serde_json::Value) -> String {
    let escaped = serde_json::to_string(&arguments.to_string()).unwrap();
    fixture("deepseek_tool_call.sse")
        .replace("__NAME__", name)
        .replace("__ARGS__", escaped.trim_matches('"'))
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

async fn pid_when_written(workspace: &Path, name: &str) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(pid) = std::fs::read_to_string(workspace.join(name))
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "{name} was never written");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn assert_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while alive(pid) {
        assert!(Instant::now() < deadline, "process {pid} is still running");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

const LONG: &str = "echo $$ > shell.pid; sleep 30 & echo $! > job.pid; wait";

#[tokio::test]
async fn cancelling_a_turn_stops_the_running_command_and_ends_the_turn_promptly() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider
        .push_sse(tool_call("bash", &json!({ "command": LONG })));
    let session_id = t.harness.create_session().unwrap();
    let turn = t.harness.start_turn(&session_id, "go".to_string());

    // The command is really running before the turn is cancelled.
    let shell = pid_when_written(&t.workspace, "shell.pid").await;
    let job = pid_when_written(&t.workspace, "job.pid").await;
    let cancelled_at = Instant::now();
    turn.cancel();
    let events = collect(turn).await;

    // Without the cancel plumbing the turn waited for the command's 30 s.
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(10),
        "took {:?}",
        cancelled_at.elapsed()
    );
    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(EventKind::TurnDone {
                state: TurnState::Cancelled,
                error: None
            })
        ),
        "{:?}",
        events.last()
    );
    assert_gone(shell).await;
    assert_gone(job).await;
}

#[tokio::test]
async fn an_uncancelled_command_still_reports_its_exit_code_and_output() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(tool_call(
        "bash",
        &json!({ "command": "echo out; echo err >&2; exit 3" }),
    ));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, events) = t.run_turn("go").await;
    let response = events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::ToolResponse { content, .. } => Some(content.clone()),
            _ => None,
        })
        .unwrap();
    assert!(response.starts_with("exit code: 3\n"), "{response}");
    assert!(response.ends_with("out\nerr\n"), "{response}");
}
