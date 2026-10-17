//! The real `banana-harness serve --stdio` process, run under the testkit's isolated environment.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use harness_testkit::{TEST_API_KEY, TestEnv, TestEnvOptions};
use serde_json::{Value, json};

struct Sidecar {
    env: TestEnv,
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr_path: std::path::PathBuf,
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && matches!(self.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Sidecar {
    fn start(env: TestEnv) -> Self {
        let stderr_path = env.root().join("sidecar.stderr");
        let mut command = Command::new(env!("CARGO_BIN_EXE_banana-harness"));
        command
            .args(["serve", "--stdio"])
            .env_clear()
            .envs(env.vars())
            .current_dir(env.workspace())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(&stderr_path).unwrap());
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            env,
            child,
            stdin,
            stdout,
            stderr_path,
        }
    }

    fn plain() -> Self {
        Self::start(TestEnv::new(&TestEnvOptions::default()).unwrap())
    }

    fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    /// The next stdout line, which must be one JSON frame.
    fn frame(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).unwrap();
        assert!(n > 0, "stdout ended; stderr: {}", self.stderr());
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line:?}"))
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.stderr_path).unwrap_or_default()
    }

    fn wait(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "the sidecar did not exit; stderr: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Everything left on stdout after the process exited.
    fn rest_of_stdout(&mut self) -> String {
        let mut rest = String::new();
        self.stdout.read_to_string(&mut rest).unwrap();
        rest
    }
}

fn assert_all_frames(text: &str) {
    for line in text.lines() {
        let frame: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("not NDJSON ({e}): {line:?}"));
        assert!(frame["kind"].is_string(), "{line}");
    }
}

#[test]
fn it_greets_lists_sessions_and_shuts_down_with_only_frames_on_stdout() {
    let mut sidecar = Sidecar::plain();
    let ready = sidecar.frame();
    assert_eq!(ready["kind"], "ready");
    assert_eq!(ready["protocol"], 1);
    assert_eq!(ready["app"], "Banana Harness");
    assert_eq!(ready["api_key_present"], true);

    sidecar.send(r#"{"kind":"request","id":"1","method":"session.list","params":{}}"#);
    assert_eq!(sidecar.frame()["result"], json!({"sessions": []}));
    sidecar.send("this is not json");
    assert_eq!(sidecar.frame()["error"]["code"], "invalid_request");
    sidecar.send(r#"{"kind":"request","id":"2","method":"shutdown"}"#);
    assert_eq!(sidecar.frame()["ok"], true);

    assert!(sidecar.wait().success());
    assert_all_frames(&sidecar.rest_of_stdout());
    let db = std::path::Path::new(&sidecar.env.vars()["XDG_DATA_HOME"])
        .join("banana-harness/harness.db");
    assert!(db.is_file(), "the database is not under the temp data dir");
}

#[test]
fn stdin_eof_exits_zero() {
    let mut sidecar = Sidecar::plain();
    sidecar.frame();
    drop(sidecar.stdin.take());
    assert!(sidecar.wait().success(), "{}", sidecar.stderr());
    assert_all_frames(&sidecar.rest_of_stdout());
}

#[test]
fn sigterm_exits_zero() {
    let mut sidecar = Sidecar::plain();
    sidecar.frame();
    let pid = sidecar.child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    assert!(sidecar.wait().success(), "{}", sidecar.stderr());
    assert_all_frames(&sidecar.rest_of_stdout());
}

#[test]
fn a_missing_key_is_reported_not_fatal() {
    let env = TestEnv::new(&TestEnvOptions::default())
        .unwrap()
        .without_key();
    let mut sidecar = Sidecar::start(env);
    assert_eq!(sidecar.frame()["api_key_present"], false);
}

#[test]
fn an_unusable_database_is_a_fatal_frame_and_exit_one() {
    let env = TestEnv::new(&TestEnvOptions::default()).unwrap();
    // A file where the data directory must be.
    let data = std::path::Path::new(&env.vars()["XDG_DATA_HOME"]).to_path_buf();
    std::fs::remove_dir_all(&data).unwrap();
    std::fs::write(&data, "not a directory").unwrap();
    let mut sidecar = Sidecar::start(env);
    let frame = sidecar.frame();
    assert_eq!(frame["kind"], "fatal", "{frame}");
    assert_eq!(sidecar.wait().code(), Some(1));
    assert!(sidecar.rest_of_stdout().is_empty());
}

#[test]
fn the_key_never_appears_on_stdout_or_stderr() {
    let mut sidecar = Sidecar::plain();
    sidecar.frame();
    sidecar.send(&format!(
        r#"{{"kind":"request","id":"k","method":"echo-{TEST_API_KEY}"}}"#
    ));
    let response = sidecar.frame();
    assert!(!response.to_string().contains(TEST_API_KEY));
    drop(sidecar.stdin.take());
    sidecar.wait();
    assert!(!sidecar.rest_of_stdout().contains(TEST_API_KEY));
    assert!(
        !sidecar.stderr().contains(TEST_API_KEY),
        "{}",
        sidecar.stderr()
    );
}

#[test]
fn serve_requires_stdio() {
    let output = Command::new(env!("CARGO_BIN_EXE_banana-harness"))
        .arg("serve")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--stdio"));
}

#[test]
fn workspace_open_uses_the_user_config_from_the_isolated_xdg_dir() {
    // TestEnv writes the user config pointing both providers at the stub's address.
    let env = TestEnv::new(&TestEnvOptions {
        workspace_template: None,
        stubs: std::collections::BTreeMap::from([(
            "baseline".to_string(),
            "http://127.0.0.1:4343".to_string(),
        )]),
    })
    .unwrap();
    let workspace = env.workspace().display().to_string();
    let mut sidecar = Sidecar::start(env);
    sidecar.frame();
    let open =
        json!({"kind":"request","id":"o","method":"workspace.open","params":{"path": workspace}});
    sidecar.send(&open.to_string());
    let opened = sidecar.frame();
    assert_eq!(opened["ok"], true, "{opened}");
    assert_eq!(opened["result"]["workspace"], workspace.as_str());
    let list = json!({"kind":"request","id":"p","method":"provider.list","params":{"workspace": workspace}});
    sidecar.send(&list.to_string());
    let providers = sidecar.frame();
    assert_eq!(
        providers["result"]["providers"][0]["host"],
        "127.0.0.1:4343"
    );
    assert_eq!(providers["result"]["providers"][0]["scheme"], "http");
    assert!(!sidecar.stderr().contains(TEST_API_KEY));
}

/// The real sidecar behind the testkit's `Transport`, so scenarios run over its stdio.
struct Process {
    child: tokio::process::Child,
    stdin: Option<tokio::process::ChildStdin>,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
    stderr_path: std::path::PathBuf,
    /// Every stdout line of every process started, for the key check.
    raw: String,
    vars: std::collections::BTreeMap<String, String>,
    workspace: std::path::PathBuf,
}

impl Process {
    async fn start(env: &TestEnv) -> Self {
        let stderr_path = env.root().join("scenario.stderr");
        let (child, stdin, stdout) = Self::spawn(env.vars(), env.workspace(), &stderr_path);
        let mut process = Self {
            child,
            stdin,
            stdout,
            stderr_path,
            raw: String::new(),
            vars: env.vars().clone(),
            workspace: env.workspace().to_path_buf(),
        };
        process.expect_ready().await;
        process
    }

    fn spawn(
        vars: &std::collections::BTreeMap<String, String>,
        workspace: &std::path::Path,
        stderr_path: &std::path::Path,
    ) -> (
        tokio::process::Child,
        Option<tokio::process::ChildStdin>,
        tokio::io::BufReader<tokio::process::ChildStdout>,
    ) {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_banana-harness"));
        command
            .args(["serve", "--stdio"])
            .env_clear()
            .envs(vars)
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(stderr_path)
                    .unwrap(),
            )
            .kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = tokio::io::BufReader::new(child.stdout.take().unwrap());
        (child, stdin, stdout)
    }

    /// The first frame is `ready`, within 10 s.
    async fn expect_ready(&mut self) {
        use harness_testkit::Transport;
        let ready = tokio::time::timeout(Duration::from_secs(10), self.next())
            .await
            .expect("no ready frame within 10 s")
            .unwrap();
        assert_eq!(ready["kind"], "ready");
        assert_eq!(ready["protocol"], 1);
    }

    /// Asks the sidecar to `shutdown`, requires the reply and an exit 0 within 5 s, and checks
    /// that nothing but frames was left on stdout. Returns everything stderr held.
    async fn finish(mut self) -> String {
        use harness_testkit::Transport;
        self.send(json!({"kind":"request","id":"bye","method":"shutdown","params":{}}))
            .await
            .unwrap();
        loop {
            let frame = self.next().await.unwrap();
            if frame["id"] == "bye" {
                assert_eq!(frame["ok"], true);
                break;
            }
        }
        let status = tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("the sidecar did not exit within 5 s of shutdown")
            .unwrap();
        assert!(status.success(), "{status}");
        let mut rest = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut self.stdout, &mut rest)
            .await
            .unwrap();
        self.raw.push_str(&rest);
        for line in rest.lines() {
            let frame: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("stdout is not NDJSON ({e}): {line:?}"));
            assert!(frame["kind"].is_string());
        }
        // Opened for append, so a replaced process's output is still in the file.
        std::fs::read_to_string(&self.stderr_path).unwrap_or_default()
    }
}

impl harness_testkit::Transport for Process {
    async fn send(&mut self, frame: Value) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("stdin is closed"))?;
        stdin.write_all(format!("{frame}\n").as_bytes()).await?;
        Ok(stdin.flush().await?)
    }

    async fn next(&mut self) -> anyhow::Result<Value> {
        use tokio::io::AsyncBufReadExt;
        let mut line = String::new();
        if self.stdout.read_line(&mut line).await? == 0 {
            anyhow::bail!("the sidecar closed stdout");
        }
        self.raw.push_str(&line);
        Ok(serde_json::from_str(&line)?)
    }

    async fn kill_and_restart(&mut self) -> anyhow::Result<()> {
        // SIGKILL: no shutdown, no cleanup, as when the process crashes.
        self.child.kill().await?;
        let (child, stdin, stdout) = Self::spawn(&self.vars, &self.workspace, &self.stderr_path);
        self.child = child;
        self.stdin = stdin;
        self.stdout = stdout;
        self.expect_ready().await;
        Ok(())
    }
}

#[tokio::test]
async fn every_scenario_runs_over_the_real_sidecar_with_only_frames_on_stdout() {
    let mut ran: Vec<String> = Vec::new();
    for path in harness_testkit::Scenario::bundled_paths().unwrap() {
        let scenario = harness_testkit::Scenario::load(&path).unwrap();
        let stub = harness_testkit::FakeProvider::start().await;
        scenario.script(&stub).unwrap();
        let env = TestEnv::new(&TestEnvOptions {
            workspace_template: Some(&scenario.workspace_template()),
            stubs: std::collections::BTreeMap::from([(
                "baseline".to_string(),
                stub.base_url.clone(),
            )]),
        })
        .unwrap();
        let mut process = Process::start(&env).await;
        let workspace = env.workspace().to_path_buf();
        harness_testkit::run_scenario(&mut process, &scenario, &stub, &workspace)
            .await
            .unwrap_or_else(|e| panic!("{}: {e:#}", scenario.name));

        let raw = process.raw.clone();
        let stderr = process.finish().await;
        // `key-never-leaks` makes a tool print the key from the sidecar's real environment.
        assert!(
            !raw.contains(TEST_API_KEY),
            "{}: the key reached stdout",
            scenario.name
        );
        assert!(
            !stderr.contains(TEST_API_KEY),
            "{}: the key reached stderr",
            scenario.name
        );
        ran.push(scenario.name);
    }
    // All of them, the process-only `kill-mid-turn` included.
    assert_eq!(ran.len(), 11, "{ran:?}");
    assert!(ran.iter().any(|name| name == "kill-mid-turn"));
}

#[tokio::test]
async fn sigterm_while_a_turn_runs_cancels_it_and_exits_zero() {
    let stub = harness_testkit::FakeProvider::start().await;
    stub.push_spec(&harness_testkit::ReplySpec {
        sse: Some(harness_testkit::SseSpec {
            text: Some("slow".into()),
            tool_calls: Vec::new(),
        }),
        gate: Some("never".into()),
        gate_after_parts: Some(2),
        ..harness_testkit::ReplySpec::default()
    })
    .unwrap();
    let env = TestEnv::new(&TestEnvOptions {
        workspace_template: None,
        stubs: std::collections::BTreeMap::from([("baseline".to_string(), stub.base_url.clone())]),
    })
    .unwrap();
    let mut process = Process::start(&env).await;
    let call = |id: &str, method: &str, params: Value| json!({"kind":"request","id":id,"method":method,"params":params});
    let workspace = env.workspace().display().to_string();
    use harness_testkit::Transport;
    process
        .send(call("1", "workspace.open", json!({"path": workspace})))
        .await
        .unwrap();
    process.next().await.unwrap();
    process
        .send(call("2", "session.create", json!({"workspace": workspace})))
        .await
        .unwrap();
    let created = process.next().await.unwrap();
    let session = created["result"]["session"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    process
        .send(call(
            "3",
            "turn.start",
            json!({"workspace": workspace, "session_id": session, "input": "go"}),
        ))
        .await
        .unwrap();
    // The turn is running once the provider has the request.
    while stub.requests().is_empty() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let pid = process.child.id().unwrap().to_string();
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    let mut cancelled = false;
    while let Ok(frame) = process.next().await {
        if frame["event"]["type"] == "turn.done" {
            cancelled = frame["event"]["state"] == "cancelled";
        }
    }
    assert!(
        cancelled,
        "SIGTERM left the turn without a cancelled turn.done"
    );
    let status = process.child.wait().await.unwrap();
    assert!(status.success(), "{status}");
}
