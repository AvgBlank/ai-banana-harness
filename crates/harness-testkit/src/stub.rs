//! The fake OpenAI-compatible provider: queued replies, recorded requests, failure modes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

use crate::{TEST_API_KEY, control, fixture, sse_text, sse_tool_calls};

const REDACTED: &str = "[redacted]";

/// One request the fake provider received.
#[derive(Clone, Debug, Serialize)]
pub struct Request {
    pub path: String,
    /// Lowercased header names. `authorization` is always `[redacted]`; see `auth_ok`.
    pub headers: HashMap<String, String>,
    pub body: Value,
    /// The request carried `Authorization: Bearer <expected key>`.
    pub auth_ok: bool,
    /// No queued reply fit this request, so it was answered 500.
    pub unmatched: bool,
}

/// What a scenario or the control API says a reply is: generated or fixture SSE, or an error.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplySpec {
    /// Generated SSE (`text` or `tool_calls`).
    pub sse: Option<SseSpec>,
    /// A raw fixture under the testkit's `fixtures/`.
    pub file: Option<String>,
    /// A non-200 status turns the reply into a JSON error (`error` is its body).
    pub status: Option<u16>,
    pub error: Option<String>,
    /// Only a request whose body contains this text is served this reply.
    #[serde(rename = "match")]
    pub matching: Option<String>,
    /// A pause before each SSE part.
    pub delay_ms: Option<u64>,
    /// Pause at this gate until `release`, after `gate_after_parts` parts (default 0).
    pub gate: Option<String>,
    pub gate_after_parts: Option<usize>,
    /// Close the connection after this many parts, mid-stream.
    pub abort_after_parts: Option<usize>,
    /// Accept the request and never answer; only the watchdog or the client ends it.
    #[serde(default)]
    pub hang: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SseSpec {
    pub text: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolCallSpec {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl ReplySpec {
    fn build(&self) -> Result<Reply, String> {
        let mut reply = Reply {
            status: self.status.unwrap_or(200),
            content_type: "text/event-stream",
            body: String::new(),
            chunk_delay: Duration::from_millis(self.delay_ms.unwrap_or(0)),
            matcher: self.matching.clone(),
            gate: self
                .gate
                .clone()
                .map(|name| (name, self.gate_after_parts.unwrap_or(0))),
            abort_after_parts: self.abort_after_parts,
            hang: self.hang,
        };
        if reply.status != 200 {
            reply.content_type = "application/json";
            reply.body = self.error.clone().unwrap_or_else(|| {
                r#"{"error":{"message":"fake provider: scripted error"}}"#.to_string()
            });
            return Ok(reply);
        }
        reply.body = match (&self.sse, &self.file) {
            (Some(sse), None) if sse.tool_calls.is_empty() => {
                sse_text(sse.text.as_deref().unwrap_or_default())
            }
            (Some(sse), None) => {
                let calls: Vec<(String, String, String)> = sse
                    .tool_calls
                    .iter()
                    .map(|c| (c.id.clone(), c.name.clone(), c.arguments.to_string()))
                    .collect();
                let refs: Vec<(&str, &str, &str)> = calls
                    .iter()
                    .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
                    .collect();
                sse_tool_calls(&refs)
            }
            (None, Some(name)) => fixture(name),
            _ if self.hang => String::new(),
            _ => return Err("a reply needs exactly one of `sse` or `file`".to_string()),
        };
        Ok(reply)
    }
}

pub(crate) struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
    chunk_delay: Duration,
    matcher: Option<String>,
    /// A gate's name and the number of parts written before waiting at it.
    gate: Option<(String, usize)>,
    abort_after_parts: Option<usize>,
    hang: bool,
}

impl Reply {
    fn sse(body: String, chunk_delay: Duration, matcher: Option<String>) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            body,
            chunk_delay,
            matcher,
            gate: None,
            abort_after_parts: None,
            hang: false,
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    replies: VecDeque<Reply>,
    requests: Vec<Request>,
    consumed: usize,
    watchdog_aborts: usize,
    released: HashSet<String>,
    /// Requests whose reply is still being written, and the most seen at once.
    in_flight: usize,
    max_in_flight: usize,
}

/// `GET /__fake/status`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Status {
    pub queued: usize,
    pub consumed: usize,
    pub unmatched: usize,
    pub watchdog_aborts: usize,
}

/// What was left over when a test ended: replies never served, requests nothing answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leftover {
    pub queued: usize,
    pub unmatched: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StubOptions {
    /// A reply still unfinished this long after it started is aborted, so a scripting mistake
    /// cannot hang a test for the client's 660 s read timeout.
    pub watchdog: Duration,
    /// The key `auth_ok` is computed against.
    pub expected_key: String,
}

impl Default for StubOptions {
    fn default() -> Self {
        Self {
            watchdog: Duration::from_secs(30),
            expected_key: TEST_API_KEY.to_string(),
        }
    }
}

pub(crate) struct Shared {
    state: Mutex<State>,
    gates: Notify,
    options: StubOptions,
}

/// A minimal OpenAI-compatible HTTP server. Each request pops the first queued reply that fits
/// it; with none queued it answers 500 so a missing script fails loudly. Control requests under
/// `/__fake/` are answered by [`control`] and are not recorded.
#[derive(Clone)]
pub struct FakeProvider {
    pub base_url: String,
    shared: Arc<Shared>,
}

impl FakeProvider {
    pub async fn start() -> Self {
        Self::start_with(StubOptions::default()).await
    }

    pub async fn start_with(options: StubOptions) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            gates: Notify::new(),
            options,
        });
        let server = Arc::clone(&shared);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let shared = Arc::clone(&server);
                tokio::spawn(async move {
                    // A client that hangs up mid-reply is a normal outcome, not a test failure.
                    let _ = serve(socket, shared).await;
                });
            }
        });
        Self { base_url, shared }
    }

    /// Queues a 200 `text/event-stream` reply, e.g. a fixture from [`fixture`].
    pub fn push_sse(&self, body: impl Into<String>) {
        self.push(Reply::sse(body.into(), Duration::ZERO, None));
    }

    /// Queues a slow SSE reply served only to a request whose body contains `needle`, so
    /// concurrent threads each get their own answer regardless of arrival order.
    pub fn push_sse_matching(
        &self,
        needle: impl Into<String>,
        body: impl Into<String>,
        chunk_delay: Duration,
    ) {
        self.push(Reply::sse(body.into(), chunk_delay, Some(needle.into())));
    }

    /// Like [`Self::push_sse`], but pausing before each SSE part so a test can reliably
    /// interleave with the stream (e.g. cancel it mid-flight).
    pub fn push_sse_slow(&self, body: impl Into<String>, chunk_delay: Duration) {
        self.push(Reply::sse(body.into(), chunk_delay, None));
    }

    /// Queues an error reply with a JSON body.
    pub fn push_error(&self, status: u16, body: impl Into<String>) {
        self.push(Reply {
            status,
            content_type: "application/json",
            body: body.into(),
            ..Reply::sse(String::new(), Duration::ZERO, None)
        });
    }

    /// Queues a reply described by a [`ReplySpec`], the form scenarios and the control API use.
    pub fn push_spec(&self, spec: &ReplySpec) -> Result<(), String> {
        self.shared.push_spec(spec)
    }

    fn push(&self, reply: Reply) {
        self.shared.state.lock().unwrap().replies.push_back(reply);
    }

    /// Opens a gate: replies waiting at it continue, and later ones pass it at once.
    pub fn release(&self, gate: &str) {
        self.shared.release(gate);
    }

    /// The most requests that were being answered at the same time.
    pub fn max_in_flight(&self) -> usize {
        self.shared.state.lock().unwrap().max_in_flight
    }

    pub fn requests(&self) -> Vec<Request> {
        self.shared.requests()
    }

    pub fn status(&self) -> Status {
        self.shared.status()
    }

    /// Drops queued replies, recorded requests and released gates.
    pub fn reset(&self) {
        self.shared.reset();
    }

    /// Replies never served and requests that found no reply; a test should expect none, which
    /// catches a script that drifted from the product's behaviour.
    pub fn leftover(&self) -> Leftover {
        let state = self.shared.state.lock().unwrap();
        Leftover {
            queued: state.replies.len(),
            unmatched: state
                .requests
                .iter()
                .filter(|r| r.unmatched)
                .map(|r| r.path.clone())
                .collect(),
        }
    }

    /// Fails the test if [`Self::leftover`] is not empty.
    pub fn assert_drained(&self) {
        let leftover = self.leftover();
        assert!(
            leftover.queued == 0 && leftover.unmatched.is_empty(),
            "fake provider leftovers: {leftover:?}"
        );
    }
}

impl Shared {
    pub(crate) fn push_spec(&self, spec: &ReplySpec) -> Result<(), String> {
        let reply = spec.build()?;
        self.state.lock().unwrap().replies.push_back(reply);
        Ok(())
    }

    pub(crate) fn requests(&self) -> Vec<Request> {
        self.state.lock().unwrap().requests.clone()
    }

    pub(crate) fn status(&self) -> Status {
        let state = self.state.lock().unwrap();
        Status {
            queued: state.replies.len(),
            consumed: state.consumed,
            unmatched: state.requests.iter().filter(|r| r.unmatched).count(),
            watchdog_aborts: state.watchdog_aborts,
        }
    }

    pub(crate) fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.replies.clear();
        state.requests.clear();
        state.released.clear();
        state.consumed = 0;
        state.watchdog_aborts = 0;
    }

    pub(crate) fn release(&self, gate: &str) {
        self.state.lock().unwrap().released.insert(gate.to_string());
        self.gates.notify_waiters();
    }

    async fn wait_at(&self, gate: &str) {
        loop {
            let notified = self.gates.notified();
            tokio::pin!(notified);
            // Registered before the check, so a release between the two is not missed.
            notified.as_mut().enable();
            if self.state.lock().unwrap().released.contains(gate) {
                return;
            }
            notified.await;
        }
    }
}

/// Counts a request as in flight until dropped, including when the client hangs up mid-reply.
struct InFlight<'a>(&'a Mutex<State>);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.in_flight -= 1;
        }
    }
}

struct Incoming {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body_bytes: Vec<u8>,
}

async fn read_request(socket: &mut TcpStream) -> io::Result<Option<Incoming>> {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 8192];
    let head_end = loop {
        let n = socket.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_string();
    let path = request_line.next().unwrap_or_default().to_string();
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < head_end + length {
        let n = socket.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(Some(Incoming {
        method,
        path,
        headers,
        body_bytes: buf.split_off(head_end),
    }))
}

async fn serve(mut socket: TcpStream, shared: Arc<Shared>) -> io::Result<()> {
    let Some(incoming) = read_request(&mut socket).await? else {
        return Ok(());
    };
    if incoming.path.starts_with("/__fake/") {
        let (status, body) = control::handle(
            &shared,
            &incoming.method,
            &incoming.path,
            &incoming.body_bytes,
        );
        let head = format!(
            "HTTP/1.1 {status} FAKE\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await?;
        socket.write_all(body.as_bytes()).await?;
        return socket.shutdown().await;
    }

    let body = serde_json::from_slice(&incoming.body_bytes).unwrap_or(Value::Null);
    let auth_ok = incoming.headers.get("authorization").map(String::as_str)
        == Some(format!("Bearer {}", shared.options.expected_key).as_str());
    let mut headers = incoming.headers;
    if let Some(value) = headers.get_mut("authorization") {
        *value = REDACTED.to_string();
    }

    let (reply, _in_flight) = {
        let mut state = shared.state.lock().unwrap();
        let body_text = body.to_string();
        let eligible = state.replies.iter().position(|reply| {
            reply
                .matcher
                .as_ref()
                .is_none_or(|needle| body_text.contains(needle.as_str()))
        });
        let reply = eligible.and_then(|i| state.replies.remove(i));
        state.requests.push(Request {
            path: incoming.path,
            headers,
            body,
            auth_ok,
            unmatched: reply.is_none(),
        });
        state.consumed += usize::from(reply.is_some());
        state.in_flight += 1;
        state.max_in_flight = state.max_in_flight.max(state.in_flight);
        drop(state);
        let reply = reply.unwrap_or(Reply {
            status: 500,
            content_type: "application/json",
            body: r#"{"error":{"message":"fake provider: no reply queued"}}"#.to_string(),
            ..Reply::sse(String::new(), Duration::ZERO, None)
        });
        (reply, InFlight(&shared.state))
    };

    let outcome = tokio::time::timeout(
        shared.options.watchdog,
        write_reply(&mut socket, &shared, &reply),
    )
    .await;
    match outcome {
        Ok(result) => result,
        Err(_elapsed) => {
            shared.state.lock().unwrap().watchdog_aborts += 1;
            Ok(())
        }
    }
}

async fn write_reply(socket: &mut TcpStream, shared: &Shared, reply: &Reply) -> io::Result<()> {
    if reply.hang {
        // Held open until the watchdog drops this future or the client goes away.
        let mut probe = [0_u8; 1];
        let _ = socket.read(&mut probe).await?;
        return Ok(());
    }
    let head = format!(
        "HTTP/1.1 {} FAKE\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    socket.write_all(head.as_bytes()).await?;
    // Write one SSE event at a time so the client parses a genuinely incremental stream.
    for (index, part) in reply.body.split_inclusive("\n\n").enumerate() {
        if reply.abort_after_parts == Some(index) {
            // Dropping the socket with the declared length unmet is a mid-stream abort.
            return Ok(());
        }
        if let Some((gate, after)) = &reply.gate
            && *after == index
        {
            shared.wait_at(gate).await;
        }
        if !reply.chunk_delay.is_zero() {
            tokio::time::sleep(reply.chunk_delay).await;
        }
        socket.write_all(part.as_bytes()).await?;
        socket.flush().await?;
    }
    socket.shutdown().await
}
