//! The `fake-provider` binary, run as a separate process.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    info: Value,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Closing stdin lets the process remove its root; kill only if it does not exit.
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && matches!(self.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn scenario(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenarios")
        .join(format!("{name}.json"))
}

fn start(extra: &[&str], keep: bool) -> Running {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fake-provider"));
    command
        .arg("serve")
        .arg("--scenario")
        .arg(scenario("plain-answer"))
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if keep {
        command.env("BH_TEST_KEEP", "1");
    }
    let mut child = command.spawn().unwrap();
    let stdin = child.stdin.take();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let info = serde_json::from_str(&line).unwrap_or_else(|e| panic!("first line {line:?}: {e}"));
    Running { child, stdin, info }
}

fn wait_exit(running: &mut Running, within: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + within;
    loop {
        if let Some(status) = running.child.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "the process did not exit within {within:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn root_of(running: &Running) -> PathBuf {
    PathBuf::from(running.info["root"].as_str().unwrap())
}

#[test]
fn it_prints_one_description_and_serves_the_scenario() {
    let running = start(&[], false);
    let info = &running.info;
    assert!(
        Path::new(info["workspace"].as_str().unwrap())
            .join("hello.txt")
            .is_file()
    );
    assert_eq!(info["env"]["AI_API_KEY"], "sk-test-0123456789abcdef");
    let provider = info["stubs"]["baseline"]["provider_url"].as_str().unwrap();
    let control = info["stubs"]["baseline"]["control_url"].as_str().unwrap();
    assert_eq!(control, format!("{provider}/__fake"));

    let status = block_on(common::request(provider, "GET", "/__fake/status", &[], ""));
    let status: Value = serde_json::from_str(&status.body).unwrap();
    assert_eq!(status["queued"], 1);

    let reply = block_on(common::chat(provider, "{}"));
    assert!(reply.body.contains("Hello"));
}

#[test]
fn closing_stdin_stops_it_and_removes_the_root() {
    let mut running = start(&[], false);
    let root = root_of(&running);
    assert!(root.exists());
    drop(running.stdin.take());
    let status = wait_exit(&mut running, Duration::from_secs(5));
    assert!(status.success());
    assert!(!root.exists(), "the root survived");
}

#[test]
fn sigterm_stops_it_and_removes_the_root() {
    let mut running = start(&[], false);
    let root = root_of(&running);
    let pid = running.child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    let status = wait_exit(&mut running, Duration::from_secs(5));
    assert!(status.success());
    assert!(!root.exists());
}

#[test]
fn bh_test_keep_leaves_the_root_for_debugging() {
    let mut running = start(&[], true);
    let root = root_of(&running);
    drop(running.stdin.take());
    wait_exit(&mut running, Duration::from_secs(5));
    assert!(root.exists(), "BH_TEST_KEEP=1 was ignored");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn several_stubs_get_their_own_ports_and_only_the_first_is_scripted() {
    let running = start(&["--stub", "baseline", "--stub", "evil"], false);
    let url = |name: &str| {
        running.info["stubs"][name]["provider_url"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_ne!(url("baseline"), url("evil"));
    let queued = |name: &str| {
        let body = block_on(common::request(
            &url(name),
            "GET",
            "/__fake/status",
            &[],
            "",
        ))
        .body;
        serde_json::from_str::<Value>(&body).unwrap()["queued"]
            .as_u64()
            .unwrap()
    };
    assert_eq!((queued("baseline"), queued("evil")), (1, 0));
    let config = std::fs::read_to_string(
        Path::new(running.info["env"]["XDG_CONFIG_HOME"].as_str().unwrap())
            .join("banana-harness/config.toml"),
    )
    .unwrap();
    assert!(config.contains(&url("baseline")) && !config.contains(&url("evil")));
}

#[test]
fn a_missing_scenario_fails_fast_with_a_message() {
    let output = Command::new(env!("CARGO_BIN_EXE_fake-provider"))
        .args(["serve", "--scenario", "/nonexistent/scenario.json"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reading scenario"));
}
