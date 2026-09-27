//! Shared seam-1 test support: a fake OpenAI-compatible provider and a harness wired to it.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use harness_core::{
    Config, ConfigPaths, Event, EventKind, Harness, HarnessOptions, Provider, Store,
};
use serde_json::Value;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A key that tests can search for in persisted rows to prove redaction.
pub const TEST_API_KEY: &str = "sk-test-0123456789abcdef";

/// Reads `tests/fixtures/<name>`.
pub fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// One request the fake provider received.
#[derive(Clone, Debug)]
pub struct Request {
    pub path: String,
    /// Lowercased header names.
    pub headers: HashMap<String, String>,
    pub body: Value,
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
    /// A pause before writing each SSE part, so a test can reliably interleave with it.
    chunk_delay: Duration,
}

#[derive(Default)]
struct State {
    replies: VecDeque<Reply>,
    requests: Vec<Request>,
}

/// A minimal OpenAI-compatible HTTP server. Each request pops the next queued reply;
/// with an empty queue it answers 500 so a missing fixture fails loudly.
#[derive(Clone)]
pub struct FakeProvider {
    pub base_url: String,
    state: Arc<Mutex<State>>,
}

impl FakeProvider {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let server_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(serve(socket, Arc::clone(&server_state)));
            }
        });
        Self { base_url, state }
    }

    /// Queues a 200 `text/event-stream` reply, e.g. a fixture from [`fixture`].
    pub fn push_sse(&self, body: impl Into<String>) {
        self.push(Reply {
            status: 200,
            content_type: "text/event-stream",
            body: body.into(),
            chunk_delay: Duration::ZERO,
        });
    }

    /// Like [`Self::push_sse`], but pausing before each SSE part so a test can reliably
    /// interleave with the stream (e.g. cancel it mid-flight).
    pub fn push_sse_slow(&self, body: impl Into<String>, chunk_delay: Duration) {
        self.push(Reply {
            status: 200,
            content_type: "text/event-stream",
            body: body.into(),
            chunk_delay,
        });
    }

    /// Queues an error reply with a JSON body.
    pub fn push_error(&self, status: u16, body: impl Into<String>) {
        self.push(Reply {
            status,
            content_type: "application/json",
            body: body.into(),
            chunk_delay: Duration::ZERO,
        });
    }

    fn push(&self, reply: Reply) {
        self.state.lock().unwrap().replies.push_back(reply);
    }

    pub fn requests(&self) -> Vec<Request> {
        self.state.lock().unwrap().requests.clone()
    }
}

async fn serve(mut socket: TcpStream, state: Arc<Mutex<State>>) {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 8192];
    let head_end = loop {
        let n = socket.read(&mut chunk).await.unwrap();
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.lines();
    let path = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_string();
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < head_end + length {
        let n = socket.read(&mut chunk).await.unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null);

    let reply = {
        let mut state = state.lock().unwrap();
        state.requests.push(Request {
            path,
            headers,
            body,
        });
        state.replies.pop_front().unwrap_or(Reply {
            status: 500,
            content_type: "application/json",
            body: r#"{"error":{"message":"fake provider: no reply queued"}}"#.to_string(),
            chunk_delay: Duration::ZERO,
        })
    };
    let head = format!(
        "HTTP/1.1 {} FAKE\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    socket.write_all(head.as_bytes()).await.unwrap();
    // Write one SSE event at a time so the client parses a genuinely incremental stream.
    for part in reply.body.split_inclusive("\n\n") {
        if !reply.chunk_delay.is_zero() {
            tokio::time::sleep(reply.chunk_delay).await;
        }
        socket.write_all(part.as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
    }
    let _ = socket.shutdown().await;
}

/// A harness whose provider base URL points at a [`FakeProvider`], with a temp workspace and database.
pub struct TestHarness {
    pub harness: Harness,
    pub provider: FakeProvider,
    pub db_path: PathBuf,
    pub workspace: PathBuf,
    _dir: TempDir,
}

impl TestHarness {
    pub async fn open(provider: Provider) -> Self {
        Self::open_configured(provider, |_| {}).await
    }

    /// Like [`open`](Self::open), but lets the test adjust the embedded default config (e.g.
    /// `skills.paths`) before the harness is built.
    pub async fn open_configured(provider: Provider, configure: impl FnOnce(&mut Config)) -> Self {
        let mut config = Config::embedded_default().unwrap();
        // The committed default's skills paths include the real user's config directory;
        // tests must not depend on (or accidentally pick up) whatever is on the host machine.
        config.skills.paths.clear();
        configure(&mut config);
        Self::open_with(provider, config).await
    }

    /// Like [`open`](Self::open), but layers `user_toml` and `workspace_toml` through
    /// [`harness_core::Config::load`] before opening, to exercise config-layering precedence
    /// end to end (e.g. the model name that reaches the fake server's request body).
    pub async fn open_layered(
        provider: Provider,
        user_toml: Option<&str>,
        workspace_toml: Option<&str>,
    ) -> Self {
        let overlay_dir = tempfile::tempdir().unwrap();
        let user_path = user_toml.map(|text| {
            let path = overlay_dir.path().join("user-config.toml");
            std::fs::write(&path, text).unwrap();
            path
        });
        let workspace_path = workspace_toml.map(|text| {
            let path = overlay_dir.path().join("workspace-config.toml");
            std::fs::write(&path, text).unwrap();
            path
        });
        let mut config = Config::load(ConfigPaths {
            user: user_path.as_deref(),
            workspace: workspace_path.as_deref(),
        })
        .unwrap();
        config.skills.paths.clear();
        Self::open_with(provider, config).await
    }

    async fn open_with(provider: Provider, mut config: Config) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        // `ignore::WalkBuilder` only honours `.gitignore` files inside an actual git repo
        // (`require_git` defaults to true), and the tool guardrail tests want a real workspace
        // to canonicalize paths against.
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&workspace)
                .status()
                .unwrap()
                .success(),
            "git init failed in the test workspace"
        );
        let db_path = dir.path().join("harness.db");
        let fake = FakeProvider::start().await;
        config.providers.deepseek.base_url = fake.base_url.clone();
        config.providers.qwen.base_url = fake.base_url.clone();
        let harness = Harness::open(HarnessOptions {
            config,
            store: Store::open(&db_path).unwrap(),
            provider,
            api_key: TEST_API_KEY.to_string(),
            workspace: workspace.clone(),
        })
        .unwrap();
        Self {
            harness,
            provider: fake,
            db_path,
            workspace,
            _dir: dir,
        }
    }

    /// Runs one turn in a new session and collects its events through `turn.done`.
    pub async fn run_turn(&self, input: &str) -> (String, Vec<Event>) {
        let session_id = self.harness.create_session().unwrap();
        let events = collect(self.harness.start_turn(&session_id, input.to_string())).await;
        (session_id, events)
    }

    /// A second connection to the same database, as a later process would open it.
    pub fn reopen_store(&self) -> Store {
        Store::open(&self.db_path).unwrap()
    }
}

/// Drains a turn's stream, failing the test if it doesn't end with `turn.done` within 10 s.
pub async fn collect(mut turn: harness_core::TurnHandle) -> Vec<Event> {
    let mut events = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = turn.events.recv().await {
            let done = matches!(event.kind, EventKind::TurnDone { .. });
            events.push(event);
            if done {
                break;
            }
        }
    })
    .await
    .expect("turn did not finish within 10 s");
    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(EventKind::TurnDone { .. })
        ),
        "stream ended without turn.done: {events:?}"
    );
    events
}
