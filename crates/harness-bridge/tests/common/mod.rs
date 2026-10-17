//! An in-process client: the bridge served over `tokio::io::duplex`, driven by frames.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use harness_bridge::{BridgeOptions, serve};
use harness_core::{Config, Harness, HarnessOptions, Provider, Store};
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
        let n = tokio::time::timeout(
            Duration::from_secs(10),
            self.from_server.read_line(&mut line),
        )
        .await
        .expect("no frame within 10 s")
        .unwrap();
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
