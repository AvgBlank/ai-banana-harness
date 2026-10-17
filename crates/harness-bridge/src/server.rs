//! The server loop: one reader, one writer task, one task per request.

use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use harness_core::{APP_NAME, Redactor, Store};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use crate::frame::{self, Line};
use crate::log;
use crate::methods::{self, Context};
use crate::workspaces::Workspaces;

/// The longest accepted line; a longer one is discarded up to its newline.
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
/// Responses and events share this queue; a full queue pauses the producers, never drops.
const QUEUE_FRAMES: usize = 1024;
/// How long shutdown waits for in-flight requests.
const DRAIN: Duration = Duration::from_secs(3);

pub struct BridgeOptions {
    /// Production: `default_db_path()`.
    pub db_path: PathBuf,
    /// Production: `user_config_path()`.
    pub user_config: Option<PathBuf>,
    /// Production: `api_key_from_env()`, or empty when none is set.
    pub api_key: String,
}

/// Serves protocol v1 on `reader` and `writer` until stdin ends, a `shutdown` request, a write
/// failure (the client is gone) or `shutdown` resolves (a signal, in the binary). A startup
/// failure is sent as a `fatal` frame and returned.
pub async fn serve<R, W, S>(reader: R, writer: W, options: BridgeOptions, shutdown: S) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Send + Unpin + 'static,
    S: Future<Output = ()>,
{
    let (tx, rx) = mpsc::channel::<Line>(QUEUE_FRAMES);
    let (gone_tx, mut gone) = oneshot::channel::<()>();
    let writer_task = tokio::spawn(write_frames(writer, rx, gone_tx));

    let redactor = Redactor::new(&options.api_key);
    let store = match Store::open(&options.db_path) {
        Ok(store) => store,
        Err(e) => {
            let message = redactor.apply(&format!("{e:#}"));
            let _ = tx.send(frame::fatal("startup_failed", &message)).await;
            drop(tx);
            let _ = writer_task.await;
            return Err(e);
        }
    };
    let context = Arc::new(Context {
        store: Arc::new(store),
        redactor,
        workspaces: Workspaces::default(),
        db_path: options.db_path.clone(),
        user_config: options.user_config.clone(),
        api_key: options.api_key.clone(),
        forwarders: std::sync::Mutex::new(JoinSet::new()),
    });
    let ready = frame::ready(
        APP_NAME,
        env!("CARGO_PKG_VERSION"),
        !options.api_key.trim().is_empty(),
    );
    let _ = tx.send(ready).await;

    let in_flight = Arc::new(Mutex::new(HashSet::<String>::new()));
    let (exit_tx, mut exit_rx) = mpsc::channel::<()>(1);
    let mut tasks = JoinSet::new();
    let mut lines = LineReader::new(reader);
    tokio::pin!(shutdown);

    loop {
        while tasks.try_join_next().is_some() {}
        let next = tokio::select! {
            () = &mut shutdown => { log("bridge: stopping on a signal"); break; }
            _ = &mut gone => { log("bridge: client stopped reading; stopping"); break; }
            _ = exit_rx.recv() => { log("bridge: shutdown requested"); break; }
            next = lines.next() => next,
        };
        let line = match next {
            Ok(Some(line)) => line,
            Ok(None) => {
                log("bridge: stdin closed; stopping");
                break;
            }
            Err(e) => {
                log(&format!("bridge: reading stdin failed: {e}"));
                break;
            }
        };
        let request = match line {
            Incoming::TooLong => {
                let message = format!("line is longer than {MAX_LINE_BYTES} bytes");
                let _ = tx
                    .send(frame::error(None, "invalid_request", &message, None))
                    .await;
                continue;
            }
            Incoming::Text(bytes) => match frame::parse_request(&bytes) {
                Ok(request) => request,
                Err((id, e)) => {
                    log(&format!("bridge: rejected a frame: {}", e.code()));
                    let _ = tx
                        .send(frame::error(id.as_deref(), e.code(), &e.to_string(), None))
                        .await;
                    continue;
                }
            },
        };
        if !in_flight
            .lock()
            .is_ok_and(|mut set| set.insert(request.id.clone()))
        {
            let _ = tx
                .send(frame::error(
                    Some(&request.id),
                    "invalid_request",
                    "request id is already in flight",
                    None,
                ))
                .await;
            continue;
        }
        tasks.spawn(handle(
            Arc::clone(&context),
            request,
            tx.clone(),
            Arc::clone(&in_flight),
            exit_tx.clone(),
        ));
    }

    // Let started requests answer, then flush every queued frame before returning.
    let _ = tokio::time::timeout(DRAIN, async { while tasks.join_next().await.is_some() {} }).await;
    tasks.abort_all();
    context.shutdown_turns().await;
    drop(tx);
    let _ = writer_task.await;
    Ok(())
}

async fn handle(
    context: Arc<Context>,
    request: frame::Request,
    tx: mpsc::Sender<Line>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    exit: mpsc::Sender<()>,
) {
    let stopping = request.method == "shutdown";
    let outcome = methods::dispatch(&context, &request.method, request.params, &tx).await;
    let line = match outcome {
        Ok(result) => frame::ok(&request.id, result),
        Err(e) => {
            let message = context.redactor.apply(&e.to_string());
            // The method name is client text, so it is redacted like any other.
            log(&context.redactor.apply(&format!(
                "bridge: {} {} failed: {}",
                request.method,
                request.id,
                e.code()
            )));
            frame::error(Some(&request.id), e.code(), &message, e.data())
        }
    };
    let _ = tx.send(line).await;
    if let Ok(mut set) = in_flight.lock() {
        set.remove(&request.id);
    }
    if stopping {
        let _ = exit.send(()).await;
    }
}

async fn write_frames<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut rx: mpsc::Receiver<Line>,
    gone: oneshot::Sender<()>,
) {
    while let Some(Line(text)) = rx.recv().await {
        let written = async {
            writer.write_all(text.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await
        }
        .await;
        if written.is_err() {
            let _ = gone.send(());
            return;
        }
    }
}

enum Incoming {
    Text(Vec<u8>),
    TooLong,
}

/// Splits input into lines with a size cap. Its state lives in the struct, so a `next` call
/// that is dropped half way (by `select!`) loses nothing.
struct LineReader<R> {
    inner: BufReader<R>,
    line: Vec<u8>,
    discarding: bool,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    fn new(reader: R) -> Self {
        Self {
            inner: BufReader::new(reader),
            line: Vec::new(),
            discarding: false,
        }
    }

    /// The next line without its newline, or `None` at end of input.
    async fn next(&mut self) -> std::io::Result<Option<Incoming>> {
        loop {
            let chunk = self.inner.fill_buf().await?;
            if chunk.is_empty() {
                if self.line.is_empty() && !self.discarding {
                    return Ok(None);
                }
                // An unterminated last line still counts.
                return Ok(Some(self.finish()));
            }
            let newline = chunk.iter().position(|&b| b == b'\n');
            let take = newline.unwrap_or(chunk.len());
            if !self.discarding {
                if self.line.len() + take > MAX_LINE_BYTES {
                    self.discarding = true;
                    self.line = Vec::new();
                } else {
                    self.line.extend_from_slice(&chunk[..take]);
                }
            }
            self.inner.consume(take + usize::from(newline.is_some()));
            if newline.is_some() {
                return Ok(Some(self.finish()));
            }
        }
    }

    fn finish(&mut self) -> Incoming {
        let line = std::mem::take(&mut self.line);
        if std::mem::take(&mut self.discarding) {
            Incoming::TooLong
        } else {
            Incoming::Text(line)
        }
    }
}
