use std::collections::VecDeque;
use std::io::{self, Read};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::{ToolContext, ToolResult};

pub const DESCRIPTION: &str = "Run a shell command in the workspace (via `bash -lc`) and return its combined \
stdout/stderr, prefixed with an `exit code: N` line. It is killed after `timeout_secs` (default 120, at most 600). \
Dangerous commands (sudo, git push, rm -rf /, piping downloads into a shell, ...) are blocked.";

const MAX_TIMEOUT_SECS: u64 = 600;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// How long to wait for the output pipe to close once the process group is gone; a process
/// that detached into its own session could otherwise hold it open forever.
const DRAIN_GRACE: Duration = Duration::from_secs(1);

pub fn parameters() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "The command to run" },
            "timeout_secs": { "type": "integer", "description": "Seconds before the command is killed (max 600)" },
        },
        "required": ["command"],
    })
}

pub fn call(ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
    let Some(command) = arguments.get("command").and_then(Value::as_str) else {
        return ToolResult::error("bash requires a string \"command\" argument");
    };
    if let Err(result) =
        crate::map_verdict(ctx.permission.check_command(command, ctx.ask_overrides))
    {
        return result;
    }
    let timeout_secs = arguments
        .get("timeout_secs")
        .and_then(Value::as_u64)
        .unwrap_or(ctx.tools.bash_timeout_secs)
        .clamp(1, MAX_TIMEOUT_SECS);
    match run(ctx, command, timeout_secs) {
        Ok(output) => ToolResult::ok(output),
        Err(e) => ToolResult::error(format!("running bash: {e}")),
    }
}

fn run(ctx: &ToolContext<'_>, command: &str, timeout_secs: u64) -> io::Result<String> {
    // One pipe for both streams, so the output interleaves the way a terminal would show it.
    let (mut reader, writer) = io::pipe()?;
    let mut cmd = Command::new("bash");
    cmd.arg("-lc")
        .arg(command)
        .current_dir(ctx.workspace)
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer)
        .process_group(0);
    for name in ctx.hidden_env {
        cmd.env_remove(name);
    }
    let mut child = cmd.spawn()?;
    // Drops the parent's copies of the write end, so the reader sees EOF once the group exits.
    drop(cmd);

    let capture = Arc::new(Mutex::new(Capture::new(ctx.tools.output_max_bytes)));
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

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    // Also on a normal exit: background jobs left in the group would otherwise outlive the call
    // and keep the output pipe open.
    kill_group(child.id());
    if status.is_none() {
        child.wait()?;
    }
    let _ = done_rx.recv_timeout(DRAIN_GRACE);

    let header = match status {
        None => format!("timed out after {timeout_secs}s; process group killed"),
        Some(status) => match (status.code(), status.signal()) {
            (Some(code), _) => format!("exit code: {code}"),
            (None, Some(signal)) => format!("killed by signal {signal}"),
            (None, None) => "exit code: unknown".to_string(),
        },
    };
    let output = lock(&capture).render();
    Ok(format!("{header}\n{output}"))
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

fn lock(capture: &Mutex<Capture>) -> std::sync::MutexGuard<'_, Capture> {
    capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Keeps the first and last half of the byte cap and counts what falls in between, so memory
/// stays bounded however much a command prints.
struct Capture {
    half: usize,
    head: Vec<u8>,
    tail: VecDeque<u8>,
    dropped: usize,
}

impl Capture {
    fn new(max_bytes: usize) -> Self {
        Self {
            half: max_bytes / 2,
            head: Vec::new(),
            tail: VecDeque::new(),
            dropped: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        let room = self.half.saturating_sub(self.head.len()).min(bytes.len());
        let (head, rest) = bytes.split_at(room);
        self.head.extend_from_slice(head);
        self.tail.extend(rest);
        let excess = self.tail.len().saturating_sub(self.half);
        self.tail.drain(..excess);
        self.dropped += excess;
    }

    fn render(&self) -> String {
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
