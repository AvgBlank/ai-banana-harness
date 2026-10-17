//! The real binary against the fake provider, under the isolation recipe: poisoned proxies, a
//! temp home and XDG roots, and only the fixed test key.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use harness_testkit::{FakeProvider, Scenario, TEST_API_KEY, TestEnv, TestEnvOptions};
use serde_json::Value;
use tokio::process::Command;

struct Run {
    env: TestEnv,
    stub: FakeProvider,
}

async fn prepare(scenario: &str) -> Run {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../harness-testkit/scenarios")
        .join(format!("{scenario}.json"));
    let scenario = Scenario::load(&path).unwrap();
    let stub = FakeProvider::start().await;
    scenario.script(&stub).unwrap();
    let env = TestEnv::new(&TestEnvOptions {
        workspace_template: Some(&scenario.workspace_template()),
        stubs: BTreeMap::from([("baseline".to_string(), stub.base_url.clone())]),
    })
    .unwrap();
    Run { env, stub }
}

struct Outcome {
    success: bool,
    events: Vec<Value>,
    stdout: String,
    stderr: String,
    elapsed: Duration,
}

async fn run_binary(env: &TestEnv, task: &str) -> Outcome {
    let mut command = Command::from(env.command(env!("CARGO_BIN_EXE_banana-harness")));
    command
        .args(["run", "--provider", "deepseek", "--json", task])
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let started = Instant::now();
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .expect("the binary did not finish within 60 s")
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let events = stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Outcome {
        success: output.status.success(),
        events,
        stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        elapsed: started.elapsed(),
    }
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|e| e["type"].as_str().unwrap_or("?"))
        .collect()
}

/// Every file under `dir` with its size and modification time.
fn listing(dir: &Path) -> Vec<(PathBuf, u64, Option<std::time::SystemTime>)> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let meta = entry.metadata().unwrap();
            if meta.is_dir() {
                pending.push(entry.path());
            } else {
                found.push((entry.path(), meta.len(), meta.modified().ok()));
            }
        }
    }
    found.sort();
    found
}

fn real_dirs() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    vec![
        home.join(".local/share/banana-harness"),
        home.join(".config/banana-harness"),
        home.join("Library/Application Support/banana-harness"),
    ]
}

#[tokio::test]
async fn a_run_under_poisoned_proxies_reaches_the_stub_and_matches_the_verified_sequence() {
    let run = prepare("plain-answer").await;
    assert_eq!(run.env.vars()["HTTP_PROXY"], "http://127.0.0.1:9");

    let outcome = run_binary(&run.env, "say hi").await;
    assert!(outcome.success, "{}\n{}", outcome.stdout, outcome.stderr);

    let scenario_events = Scenario::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../harness-testkit/scenarios/plain-answer.json"),
    )
    .unwrap()
    .expect
    .events;
    assert_eq!(kinds(&outcome.events), scenario_events);

    let requests = run.stub.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].auth_ok, "the stub did not get the test key");
    assert_eq!(requests[0].path, "/chat/completions");
    run.stub.assert_drained();
}

#[tokio::test]
async fn a_non_loopback_request_fails_fast_and_never_reaches_the_stub() {
    let run = prepare("plain-answer").await;
    // 203.0.113.0/24 is documentation space: only the dead proxy can answer, and it refuses.
    let config = Path::new(&run.env.vars()["XDG_CONFIG_HOME"]).join("banana-harness/config.toml");
    std::fs::write(
        config,
        "[providers.deepseek]\nbase_url = \"http://203.0.113.7:81\"\n",
    )
    .unwrap();

    let outcome = run_binary(&run.env, "say hi").await;
    assert!(
        outcome.elapsed < Duration::from_secs(20),
        "took {:?}: the request was not stopped at the dead proxy",
        outcome.elapsed
    );
    assert!(!outcome.success || kinds(&outcome.events).last() != Some(&"turn.done"));
    assert!(run.stub.requests().is_empty());
}

#[tokio::test]
async fn state_lands_under_the_temp_roots_and_the_real_ones_are_untouched() {
    let before: Vec<_> = real_dirs().iter().map(|d| listing(d)).collect();
    let run = prepare("plain-answer").await;
    let outcome = run_binary(&run.env, "say hi").await;
    assert!(outcome.success, "{}", outcome.stderr);

    let db = Path::new(&run.env.vars()["XDG_DATA_HOME"]).join("banana-harness/harness.db");
    assert!(db.is_file(), "no database at {}", db.display());
    assert!(db.starts_with(run.env.root()));
    let after: Vec<_> = real_dirs().iter().map(|d| listing(d)).collect();
    assert_eq!(before, after, "a real data or config directory changed");
}

#[tokio::test]
async fn the_hosts_key_never_reaches_the_child() {
    // Without a test key of its own, the child must have none: the host's must not fill the gap.
    let run = prepare("plain-answer").await;
    let env = run.env.without_key();
    let seen = String::from_utf8_lossy(&env.command("env").output().unwrap().stdout).into_owned();
    assert!(
        !seen.lines().any(|l| l.starts_with("AI_API_KEY=")),
        "{seen}"
    );
    if let Ok(host_key) = std::env::var("AI_API_KEY")
        && !host_key.is_empty()
    {
        assert!(
            !seen.contains(&host_key),
            "the host key reached the child environment"
        );
    }
}

#[tokio::test]
async fn the_test_key_never_appears_in_the_binarys_output_or_the_stub_log() {
    let run = prepare("plain-answer").await;
    let outcome = run_binary(&run.env, "say hi").await;
    assert!(!outcome.stdout.contains(TEST_API_KEY));
    assert!(!outcome.stderr.contains(TEST_API_KEY));
    assert!(
        !serde_json::to_string(&run.stub.requests())
            .unwrap()
            .contains(TEST_API_KEY)
    );
}
