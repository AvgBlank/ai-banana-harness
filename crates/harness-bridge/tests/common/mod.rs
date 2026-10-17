//! An in-process client: the bridge served over `tokio::io::duplex`, driven by frames.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use harness_bridge::{BridgeOptions, serve};
use harness_core::{Config, Harness, HarnessOptions, Provider, Store};
use harness_testkit::{FakeProvider, Scenario, TestEnv, TestEnvOptions};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::task::JoinHandle;

pub const API_KEY: &str = "sk-test-0123456789abcdef";

pub struct Client {
    pub dir: TempDir,
    pub db_path: PathBuf,
    to_server: DuplexStream,
    from_server: BufReader<DuplexStream>,
    pub server: JoinHandle<anyhow::Result<()>>,
    /// Every frame sent and received so far, in order, for golden files.
    pub log: Vec<Value>,
}

impl Client {
    pub async fn start() -> Self {
        Self::start_with(API_KEY, 1 << 16).await
    }

    /// `pipe` is the server-to-client buffer: a small one makes a stalled reader bite quickly.
    pub async fn start_with(api_key: &str, pipe: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("harness.db");
        Self::start_at(dir, db_path, api_key, pipe, None)
    }

    pub fn start_at(
        dir: TempDir,
        db_path: PathBuf,
        api_key: &str,
        pipe: usize,
        user_config: Option<PathBuf>,
    ) -> Self {
        let (to_server, server_in) = tokio::io::duplex(1 << 16);
        let (server_out, from_server) = tokio::io::duplex(pipe);
        let options = BridgeOptions {
            db_path: db_path.clone(),
            user_config,
            api_key: api_key.to_string(),
        };
        let server = tokio::spawn(serve(
            server_in,
            server_out,
            options,
            std::future::pending(),
        ));
        Self {
            dir,
            db_path,
            to_server,
            from_server: BufReader::new(from_server),
            server,
            log: Vec::new(),
        }
    }

    pub async fn send_raw(&mut self, bytes: &[u8]) {
        self.to_server.write_all(bytes).await.unwrap();
    }

    pub async fn send_line(&mut self, line: &str) {
        if let Ok(frame) = serde_json::from_str(line) {
            self.log.push(frame);
        }
        self.send_raw(format!("{line}\n").as_bytes()).await;
    }

    pub async fn send(&mut self, id: &str, method: &str, params: Value) {
        let frame = json!({"kind": "request", "id": id, "method": method, "params": params});
        self.send_line(&frame.to_string()).await;
    }

    /// The next frame, failing the test if none arrives within 10 s.
    pub async fn next(&mut self) -> Value {
        self.try_next().await.expect("the server closed the stream")
    }

    pub async fn try_next(&mut self) -> Option<Value> {
        let mut line = String::new();
        let read = tokio::time::timeout(
            Duration::from_secs(10),
            self.from_server.read_line(&mut line),
        )
        .await;
        let Ok(read) = read else {
            let tail: Vec<String> = self
                .log
                .iter()
                .rev()
                .take(6)
                .map(|f| f.to_string().chars().take(200).collect())
                .collect();
            panic!("no frame within 10 s; the last frames, newest first: {tail:#?}");
        };
        let n = read.unwrap();
        if n == 0 {
            return None;
        }
        assert!(line.ends_with('\n'), "a frame without a newline: {line:?}");
        let frame: Value =
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line:?}"));
        self.log.push(frame.clone());
        Some(frame)
    }

    /// Sends a request and returns its response, which must be the next frame.
    pub async fn call(&mut self, id: &str, method: &str, params: Value) -> Value {
        self.send(id, method, params).await;
        self.next().await
    }

    /// Stops listening: the server's next write fails.
    pub fn hang_up(&mut self) {
        self.from_server = BufReader::new(tokio::io::duplex(1).1);
    }

    /// Closes the client's write side (stdin EOF).
    pub async fn close_input(&mut self) {
        self.to_server.shutdown().await.unwrap();
    }

    pub async fn finish(self) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(10), self.server)
            .await
            .expect("serve did not return")
            .unwrap()
    }
}

/// Creates a session in `workspace_name` (a new directory under `base`) through the core API.
pub async fn create_session(db_path: &Path, base: &Path, workspace_name: &str) -> String {
    let workspace = base.join(workspace_name);
    std::fs::create_dir_all(&workspace).unwrap();
    let mut config = Config::embedded_default().unwrap();
    config.skills.paths.clear();
    let harness = Harness::open(HarnessOptions {
        config,
        store: Store::open(db_path).unwrap(),
        provider: Provider::DeepSeek,
        api_key: API_KEY.to_string(),
        workspace,
        headless: false,
    })
    .unwrap();
    harness.create_session().unwrap()
}

/// Compares `frames` with `tests/fixtures/<name>.ndjson`, or rewrites it when
/// `BH_UPDATE_GOLDEN=1`. Callers normalise ids, times and paths first.
pub fn golden(name: &str, frames: &[Value]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.ndjson"));
    let actual: String = frames.iter().map(|f| format!("{f}\n")).collect();
    if std::env::var("BH_UPDATE_GOLDEN").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "reading {}: {e} (BH_UPDATE_GOLDEN=1 writes it)",
            path.display()
        )
    });
    assert_eq!(actual, expected, "golden frames {name} are out of date");
}

/// Replaces the parts of a `session.list` response that differ per run.
pub fn normalise_sessions(frame: &mut Value, base: &Path) {
    let base = base.display().to_string();
    for (n, session) in frame["result"]["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        session["id"] = json!(format!("<session-{n}>"));
        session["created_at"] = json!("<time>");
        session["updated_at"] = json!("<time>");
        let workspace = session["workspace"]
            .as_str()
            .unwrap()
            .replace(&base, "<base>");
        session["workspace"] = json!(workspace);
    }
}

impl harness_testkit::Transport for Client {
    async fn send(&mut self, frame: Value) -> anyhow::Result<()> {
        self.send_line(&frame.to_string()).await;
        Ok(())
    }

    async fn next(&mut self) -> anyhow::Result<Value> {
        self.try_next()
            .await
            .ok_or_else(|| anyhow::anyhow!("the server closed the stream"))
    }
}

pub struct Run {
    pub client: Client,
    pub env: TestEnv,
    pub stub: FakeProvider,
    /// Events that arrived while a call waited for its response, not yet looked at.
    pub stash: std::collections::VecDeque<Value>,
}

pub fn scenario(name: &str) -> Scenario {
    Scenario::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../harness-testkit/scenarios")
            .join(format!("{name}.json")),
    )
    .unwrap()
}

impl Run {
    /// A bridge whose user config sends both providers to a fresh stub, over a copy of the
    /// scenario's workspace. The stub is left unscripted.
    pub async fn new(workspace_template: &Path) -> Self {
        let stub = FakeProvider::start().await;
        let env = TestEnv::new(&TestEnvOptions {
            workspace_template: Some(workspace_template),
            stubs: BTreeMap::from([("baseline".to_string(), stub.base_url.clone())]),
        })
        .unwrap();
        let user_config: PathBuf =
            Path::new(&env.vars()["XDG_CONFIG_HOME"]).join("banana-harness/config.toml");
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("harness.db");
        let mut client = Client::start_at(dir, db_path, API_KEY, 1 << 16, Some(user_config));
        client.next().await;
        Self {
            client,
            env,
            stub,
            stash: std::collections::VecDeque::new(),
        }
    }

    pub async fn for_scenario(name: &str) -> (Self, Scenario) {
        let scenario = scenario(name);
        let run = Self::new(&scenario.workspace_template()).await;
        scenario.script(&run.stub).unwrap();
        (run, scenario)
    }

    pub fn workspace(&self) -> String {
        self.env.workspace().display().to_string()
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Value {
        // Events may arrive first; the response is the frame with this call's id.
        self.client.send("c", method, params).await;
        loop {
            let frame = self.client.next().await;
            if frame["kind"] == "response" {
                return frame;
            }
            self.stash.push_back(frame);
        }
    }

    pub async fn open_and_create(&mut self) -> String {
        let workspace = self.workspace();
        self.call("workspace.open", json!({"path": workspace}))
            .await;
        let created = self
            .call("session.create", json!({"workspace": workspace}))
            .await;
        created["result"]["session"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    pub async fn start_turn(&mut self, session: &str, input: &str) -> Value {
        let workspace = self.workspace();
        self.call(
            "turn.start",
            json!({"workspace": workspace, "session_id": session, "input": input}),
        )
        .await
    }

    /// Reads frames until an event of this type; returns its frame.
    pub async fn event(&mut self, event_type: &str) -> Value {
        loop {
            let frame = match self.stash.pop_front() {
                Some(frame) => frame,
                None => self.client.next().await,
            };
            if frame["kind"] == "event" && frame["event"]["type"] == event_type {
                return frame;
            }
        }
    }

    /// Waits until the provider has received `count` requests, so a cancel lands mid-request.
    pub async fn wait_for_requests(&self, count: usize) {
        for _ in 0..500 {
            if self.stub.requests().len() >= count {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the provider never got request {count}");
    }

    pub async fn done_state(&mut self) -> String {
        let frame = self.event("turn.done").await;
        frame["event"]["state"].as_str().unwrap().to_string()
    }

    pub async fn active_turn(&mut self) -> Value {
        let listed = self.call("workspace.list", json!({})).await;
        listed["result"]["workspaces"][0]["active_turn"].clone()
    }
}
