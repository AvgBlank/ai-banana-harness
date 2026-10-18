//! How a `bash` command is run: the [`CommandRunner`] seam, its host implementation and the
//! [`CancelToken`] that stops a command that is already running.

use std::collections::VecDeque;
use std::io::{self, Read};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// How long to wait for the output pipe to close once the process group is gone; a process
/// that detached into its own session could otherwise hold it open forever.
pub(crate) const DRAIN_GRACE: Duration = Duration::from_secs(1);

/// Stops the tool call that holds a clone of it. A plain shared flag, so it needs no async
/// runtime and the owner of the turn can set it from any thread.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// One command to run.
pub struct CommandRequest<'a> {
    pub command: &'a str,
    pub workspace: &'a Path,
    /// Environment variables the command must not see, e.g. the provider key.
    pub hidden_env: &'a [&'a str],
    pub timeout: Duration,
    /// The output cap; the middle of longer output is dropped.
    pub output_max_bytes: usize,
    pub cancel: &'a CancelToken,
}

/// How a command ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Exited(i32),
    Signaled(i32),
    /// The process ended without a code or a signal.
    Unknown,
    TimedOut,
    Cancelled,
    /// The command was killed for passing the memory limit it was run under.
    OutOfMemory,
}

#[derive(Debug)]
pub struct CommandOutput {
    pub finish: Finish,
    /// What a timeout or a cancel kills, for the message: "process group" or "container".
    pub kill_scope: &'static str,
    /// Stdout and stderr interleaved, as a terminal would show them.
    pub output: String,
}

/// Runs a command to its end, its timeout or its cancellation, and leaves nothing of it
/// running when it returns.
pub trait CommandRunner: Send + Sync {
    fn run(&self, request: &CommandRequest<'_>) -> io::Result<CommandOutput>;
}

/// Runs the command on this machine: `bash -lc` in the workspace, in a process group of its own.
pub struct HostRunner;

impl CommandRunner for HostRunner {
    fn run(&self, request: &CommandRequest<'_>) -> io::Result<CommandOutput> {
        // A command already cancelled is never started.
        if request.cancel.is_cancelled() {
            return Ok(CommandOutput {
                finish: Finish::Cancelled,
                kill_scope: "process group",
                output: String::new(),
            });
        }
        // One pipe for both streams, so the output interleaves the way a terminal would show it.
        let (mut reader, writer) = io::pipe()?;
        let mut cmd = Command::new("bash");
        cmd.arg("-lc")
            .arg(request.command)
            .current_dir(request.workspace)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer)
            .process_group(0);
        for name in request.hidden_env {
            cmd.env_remove(name);
        }
        let mut child = cmd.spawn()?;
        // Drops the parent's copies of the write end, so the reader sees EOF once the group exits.
        drop(cmd);

        let capture = Arc::new(Mutex::new(Capture::new(request.output_max_bytes)));
        let (done_tx, done_rx) = mpsc::channel();
        let sink = Arc::clone(&capture);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                lock(&sink).push(&buf[..n]);
            }
            let _ = done_tx.send(());
        });

        let deadline = Instant::now() + request.timeout;
        // Order matters when several things are true at once: a process that has already ended
        // wins, then a cancel, then the timeout.
        let finish = loop {
            match child.try_wait() {
                Ok(Some(status)) => break finish_of(status),
                Ok(None) => {}
                Err(error) => {
                    // The child cannot be inspected; do not leave its group behind.
                    kill_group(child.id());
                    return Err(error);
                }
            }
            if request.cancel.is_cancelled() {
                break Finish::Cancelled;
            }
            if Instant::now() >= deadline {
                break Finish::TimedOut;
            }
            std::thread::sleep(POLL_INTERVAL);
        };
        // Also on a normal exit: background jobs left in the group would otherwise outlive the
        // call and keep the output pipe open.
        kill_group(child.id());
        if matches!(finish, Finish::TimedOut | Finish::Cancelled) {
            child.wait()?;
        }
        let _ = done_rx.recv_timeout(DRAIN_GRACE);
        let output = lock(&capture).render();
        Ok(CommandOutput {
            finish,
            kill_scope: "process group",
            output,
        })
    }
}

fn finish_of(status: ExitStatus) -> Finish {
    match (status.code(), status.signal()) {
        (Some(code), _) => Finish::Exited(code),
        (None, Some(signal)) => Finish::Signaled(signal),
        (None, None) => Finish::Unknown,
    }
}

fn kill_group(pid: u32) {
    let Ok(pgid) = libc::pid_t::try_from(pid) else {
        return;
    };
    // SAFETY: kill(2) takes no pointers; a negative pid signals the child's own process group,
    // which `process_group(0)` created, so nothing outside the command is touched.
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

pub(crate) fn lock(capture: &Mutex<Capture>) -> std::sync::MutexGuard<'_, Capture> {
    capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Keeps the first and last half of the byte cap and counts what falls in between, so memory
/// stays bounded however much a command prints.
pub(crate) struct Capture {
    half: usize,
    head: Vec<u8>,
    tail: VecDeque<u8>,
    dropped: usize,
}

impl Capture {
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            half: max_bytes / 2,
            head: Vec::new(),
            tail: VecDeque::new(),
            dropped: 0,
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) {
        let room = self.half.saturating_sub(self.head.len()).min(bytes.len());
        let (head, rest) = bytes.split_at(room);
        self.head.extend_from_slice(head);
        self.tail.extend(rest);
        let excess = self.tail.len().saturating_sub(self.half);
        self.tail.drain(..excess);
        self.dropped += excess;
    }

    pub(crate) fn render(&self) -> String {
        let mut tail = Vec::with_capacity(self.tail.len());
        tail.extend(self.tail.iter());
        if self.dropped == 0 {
            let mut all = self.head.clone();
            all.extend(tail);
            return String::from_utf8_lossy(&all).into_owned();
        }
        format!(
            "{}\n[... {} bytes truncated ...]\n{}",
            String::from_utf8_lossy(&self.head),
            self.dropped,
            String::from_utf8_lossy(&tail)
        )
    }
}
