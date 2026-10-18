//! A harness opened with a container runner: `bash` goes through it (a fake `docker`, no daemon),
//! guardrails still come first, cancellation kills the container, and opening sweeps leftovers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::process::Command;
use std::time::{Duration, Instant};

use common::{TestHarness, collect};
use harness_core::{ContainerConfig, EventKind, Provider, TurnState};
use harness_testkit::{FakeDocker, fixture};
use serde_json::{Value, json};

const IMAGE: &str = "agent-image:1";

fn tool_call(name: &str, arguments: &Value) -> String {
    let escaped = serde_json::to_string(&arguments.to_string()).unwrap();
    fixture("deepseek_tool_call.sse")
        .replace("__NAME__", name)
        .replace("__ARGS__", escaped.trim_matches('"'))
}

fn config(docker: &FakeDocker) -> ContainerConfig {
    let mut config = ContainerConfig::new(IMAGE);
    config.docker = docker.executable().to_path_buf();
    config
}

fn fake() -> FakeDocker {
    let docker = FakeDocker::new();
    docker.add_image(IMAGE);
    docker
}

fn responses(events: &[harness_core::Event]) -> Vec<(String, bool)> {
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

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[tokio::test]
async fn a_bash_call_runs_through_the_container_runner() {
    let docker = fake();
    let t = TestHarness::open_container(Provider::DeepSeek, config(&docker)).await;
    t.provider.push_sse(tool_call(
        "bash",
        &json!({"command": "echo out; echo err >&2; exit 3"}),
    ));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, events) = t.run_turn("go").await;

    let (content, blocked) = responses(&events).remove(0);
    assert!(!blocked);
    assert!(content.starts_with("exit code: 3\n"), "{content}");
    assert!(content.ends_with("out\nerr\n"), "{content}");
    let create = docker.calls_of("create").remove(0);
    assert_eq!(
        &create[create.len() - 4..],
        [IMAGE, "bash", "-lc", "echo out; echo err >&2; exit 3"]
    );
    assert!(create.windows(2).any(|w| w == ["--network", "none"]));
    assert_eq!(docker.containers(), Vec::<String>::new());
}

#[tokio::test]
async fn the_api_key_is_not_in_the_containers_environment_or_arguments() {
    let docker = fake();
    let t = TestHarness::open_container(Provider::DeepSeek, config(&docker)).await;
    t.provider.push_sse(tool_call(
        "bash",
        &json!({"command": "echo \"[${AI_API_KEY-unset}]\""}),
    ));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, events) = t.run_turn("go").await;
    let (content, _) = responses(&events).remove(0);
    assert!(content.ends_with("[unset]\n"), "{content}");
    for call in docker.calls() {
        assert!(
            !call
                .iter()
                .any(|a| a.contains(harness_testkit::TEST_API_KEY))
        );
    }
}

#[tokio::test]
async fn a_blocked_command_never_reaches_docker() {
    let docker = fake();
    let t = TestHarness::open_container(Provider::DeepSeek, config(&docker)).await;
    t.provider
        .push_sse(tool_call("bash", &json!({"command": "sudo true"})));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, events) = t.run_turn("go").await;
    let (content, blocked) = responses(&events).remove(0);
    assert!(blocked, "{content}");
    // Only the sweep when the harness opened: nothing for the command.
    assert!(
        docker.calls().iter().all(|call| call[0] == "ps"),
        "{:?}",
        docker.calls()
    );
}

#[tokio::test]
async fn a_missing_image_fails_the_command_without_running_it_anywhere() {
    let docker = FakeDocker::new();
    let t = TestHarness::open_container(Provider::DeepSeek, config(&docker)).await;
    t.provider
        .push_sse(tool_call("bash", &json!({"command": "touch ran"})));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, events) = t.run_turn("go").await;
    let (content, _) = responses(&events).remove(0);
    assert!(content.contains("docker pull agent-image:1"), "{content}");
    assert!(
        !t.workspace.join("ran").exists(),
        "the command ran on the host"
    );
    assert!(docker.calls_of("pull").is_empty());
}

#[tokio::test]
async fn cancelling_the_turn_kills_the_container_and_leaves_nothing() {
    let docker = fake();
    let t = TestHarness::open_container(Provider::DeepSeek, config(&docker)).await;
    t.provider.push_sse(tool_call(
        "bash",
        &json!({"command": "echo $$ > shell.pid; sleep 30 & echo $! > job.pid; wait"}),
    ));
    let session_id = t.harness.create_session().unwrap();
    let turn = t.harness.start_turn(&session_id, "go".to_string());
    let deadline = Instant::now() + Duration::from_secs(20);
    while !t.workspace.join("job.pid").exists() {
        assert!(Instant::now() < deadline, "the command never started");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let cancelled_at = Instant::now();
    turn.cancel();
    let events = collect(turn).await;
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(15),
        "{:?}",
        cancelled_at.elapsed()
    );
    assert!(matches!(
        events.last().map(|e| &e.kind),
        Some(EventKind::TurnDone {
            state: TurnState::Cancelled,
            ..
        })
    ));
    assert_eq!(docker.calls_of("kill").len(), 1);
    assert_eq!(docker.containers(), Vec::<String>::new());
    for name in ["shell.pid", "job.pid"] {
        let pid: u32 = std::fs::read_to_string(t.workspace.join(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while alive(pid) {
            assert!(
                Instant::now() < deadline,
                "process {pid} survived the cancel"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

#[tokio::test]
async fn opening_removes_leftovers_of_a_dead_sidecar_and_spares_a_live_ones() {
    let docker = fake();
    docker.add_leftover("banana-harness-cmd-dead", true, 4_000_000);
    docker.add_leftover("banana-harness-cmd-live", true, std::process::id());
    docker.add_leftover("someone-elses", false, 4_000_000);
    let _t = TestHarness::open_container(Provider::DeepSeek, config(&docker)).await;
    assert_eq!(
        docker.containers(),
        ["banana-harness-cmd-live", "someone-elses"]
    );
}

#[tokio::test]
async fn opening_without_a_container_never_calls_docker() {
    // The fake is on no one's path here, so a call would be an error; nothing can have reached it.
    let docker = fake();
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider
        .push_sse(tool_call("bash", &json!({"command": "echo host"})));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (_, events) = t.run_turn("go").await;
    let (content, _) = responses(&events).remove(0);
    assert!(content.ends_with("host\n"), "{content}");
    assert!(docker.calls().is_empty());
}
