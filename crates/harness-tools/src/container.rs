//! `ContainerRunner`: a `bash` command in a container of its own, through the `docker`
//! command-line client (no Docker library, no socket handed to anything).
//!
//! A command is `docker create`, `docker start -a`, `docker inspect`, `docker rm -f`. Killing the
//! `docker` client does not stop what runs in the container, so a timeout or a cancel runs
//! `docker kill` on the container itself. Nothing falls back to the host: if Docker or the image
//! is missing the command fails with what to do about it.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::runner::{
    CommandOutput, CommandRequest, CommandRunner, DRAIN_GRACE, Finish, POLL_INTERVAL, lock,
};

/// The label every container this runner makes carries, so leftovers can be found.
pub const COMMAND_LABEL: &str = "banana-harness.command";
/// The process that made the container; a sweep leaves containers whose owner still runs, since
/// they belong to a command in progress (another workspace's, or another sidecar's).
pub const OWNER_LABEL: &str = "banana-harness.pid";

/// How long the `docker` client may take to answer the quick calls, and to create a container.
const QUICK: Duration = Duration::from_secs(15);
const CREATE: Duration = Duration::from_secs(60);
/// How long the `docker start` client gets to notice its container was killed before it is
/// killed too.
const CLIENT_GRACE: Duration = Duration::from_secs(3);

/// Whether the container can reach the network. Off unless the user turns it on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContainerNetwork {
    #[default]
    None,
    Bridge,
}

/// What a container is made with.
#[derive(Clone, Debug)]
pub struct ContainerConfig {
    /// The `docker` executable.
    pub docker: PathBuf,
    /// The image to run: explicitly chosen, already on this machine; never pulled.
    pub image: String,
    pub network: ContainerNetwork,
    /// Memory limit (swap included), in Docker's notation.
    pub memory: String,
    pub pids_limit: u32,
    /// Size of the writable `/tmp`, in Docker's notation.
    pub tmp_size: String,
    /// `uid:gid` the command runs as; `None` leaves the image's own user.
    pub user: Option<String>,
}

impl ContainerConfig {
    /// The approved defaults: no network, 2 GiB, 256 processes, a 256 MiB `/tmp`, the host's
    /// user and group.
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            docker: PathBuf::from("docker"),
            image: image.into(),
            network: ContainerNetwork::None,
            memory: "2g".to_string(),
            pids_limit: 256,
            tmp_size: "256m".to_string(),
            user: Some(host_user()),
        }
    }
}

/// Whether the process named by an owner label is still running. An unreadable label counts as
/// not running: the container is not one of a live command.
fn owner_runs(owner: &str) -> bool {
    let Ok(pid) = owner.parse::<libc::pid_t>() else {
        return false;
    };
    // SAFETY: kill(2) with signal 0 only checks that the process exists; no signal is sent.
    let found = unsafe { libc::kill(pid, 0) };
    found == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn host_user() -> String {
    // SAFETY: getuid(2) and getgid(2) take no arguments and cannot fail.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    format!("{uid}:{gid}")
}

/// Runs each command in its own container.
pub struct ContainerRunner {
    config: ContainerConfig,
}

impl ContainerRunner {
    pub fn new(config: ContainerConfig) -> Self {
        Self { config }
    }

    /// Whether a command could start now: the `docker` client answers and the image is here. The
    /// error says what to do about it, as a failed command would.
    pub fn check(&self) -> io::Result<()> {
        let image = &self.config.image;
        let probe = self.docker(&["image", "inspect", "--format", "{{.Id}}", image], QUICK)?;
        if probe.ok {
            Ok(())
        } else {
            Err(self.explain("image inspect", &probe.stderr, None))
        }
    }

    /// Removes containers an earlier sidecar left behind (it was killed outright, which gives it
    /// no chance to clean up). Only containers carrying [`COMMAND_LABEL`] whose owner process
    /// ([`OWNER_LABEL`]) no longer runs are touched, so a command in progress is never swept.
    /// Returns how many were removed. By hand, for everything this ever left:
    /// `docker rm -f $(docker ps -aq --filter label=banana-harness.command)`.
    pub fn sweep_leftovers(&self) -> io::Result<usize> {
        let filter = format!("label={COMMAND_LABEL}");
        let format = format!("{{{{.ID}}}} {{{{.Label \"{OWNER_LABEL}\"}}}}");
        let listed = self.docker(
            &["ps", "-a", "--filter", &filter, "--format", &format],
            QUICK,
        )?;
        if !listed.ok {
            return Err(self.explain("ps", &listed.stderr, None));
        }
        let ids: Vec<&str> = listed
            .stdout
            .lines()
            .filter_map(|line| {
                let (id, owner) = line.split_once(' ')?;
                (!owner_runs(owner.trim())).then_some(id)
            })
            .collect();
        if ids.is_empty() {
            return Ok(0);
        }
        let mut args = vec!["rm", "-f"];
        args.extend(&ids);
        let removed = self.docker(&args, CREATE)?;
        if !removed.ok {
            return Err(self.explain("rm", &removed.stderr, None));
        }
        Ok(ids.len())
    }

    /// One call of the `docker` client, bounded by `limit`.
    fn docker(&self, args: &[&str], limit: Duration) -> io::Result<Reply> {
        let mut child = Command::new(&self.config.docker)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| self.not_available(&error))?;
        let (stdout, stderr) = (
            take_text(child.stdout.take()),
            take_text(child.stderr.take()),
        );
        let deadline = Instant::now() + limit;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::other(format!(
                    "docker {} did not answer within {}s; is Docker running and responsive?",
                    args.first().copied().unwrap_or(""),
                    limit.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        Ok(Reply {
            ok: status.success(),
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        })
    }

    fn not_available(&self, error: &io::Error) -> io::Error {
        if error.kind() == io::ErrorKind::NotFound {
            io::Error::other(format!(
                "Docker is not available: the `docker` command was not found ({}). The command \
                 was not run, and it is never run on the host as a fallback. Install Docker and \
                 try again.",
                self.config.docker.display()
            ))
        } else {
            io::Error::other(format!("Docker could not be started: {error}"))
        }
    }

    /// What went wrong with a `docker` call, in words that say what to do. Nothing here falls
    /// back to the host.
    fn explain(&self, step: &str, stderr: &str, workspace: Option<&Path>) -> io::Error {
        let image = &self.config.image;
        let text = stderr.trim();
        let message = if text.contains("Cannot connect to the Docker daemon")
            || text.contains("docker daemon running")
        {
            "Docker is not running. The command was not run, and it is never run on the host as \
             a fallback. Start Docker Desktop (or the docker service) and try again."
                .to_string()
        } else if text.contains("No such image") {
            format!(
                "The container image {image} is not available locally. Banana Harness never \
                 pulls images; run: docker pull {image}"
            )
        } else if text.contains("executable file not found") {
            format!(
                "The container image {image} has no bash, which commands need (they run as \
                 `bash -lc`). Use an image that includes bash."
            )
        } else if text.contains("bind source path does not exist")
            || text.contains("mounts denied")
            || text.contains("is not shared")
            || text.contains("invalid mount config")
        {
            let place = workspace
                .map(|w| w.display().to_string())
                .unwrap_or_default();
            format!(
                "Docker could not mount the workspace {place} into the container: {text}. On \
                 Docker Desktop, add the folder under Settings > Resources > File sharing."
            )
        } else {
            let shown: String = text.chars().take(400).collect();
            format!("docker {step} failed: {shown}")
        };
        // What Docker itself said stays visible after the advice.
        let said: String = text.chars().take(300).collect();
        io::Error::other(if said.is_empty() || message.contains(&said) {
            message
        } else {
            format!("{message} (docker said: {said})")
        })
    }

    fn kill(&self, name: &str) {
        // A container that already ended answers "is not running"; there is nothing to do then.
        let _ = self.docker(&["kill", "--signal", "KILL", name], QUICK);
    }

    fn remove(&self, name: &str) -> io::Result<()> {
        let reply = self.docker(&["rm", "-f", name], CREATE)?;
        if reply.ok || reply.stderr.contains("No such container") {
            Ok(())
        } else {
            Err(io::Error::other(reply.stderr.trim().to_string()))
        }
    }

    fn create_args(
        &self,
        name: &str,
        request: &CommandRequest<'_>,
        workspace: &str,
    ) -> Vec<String> {
        let c = &self.config;
        let mut args: Vec<String> = ["create", "--name", name, "--label"]
            .map(String::from)
            .into();
        args.push(format!("{COMMAND_LABEL}={name}"));
        args.extend([
            "--label".to_string(),
            format!("{OWNER_LABEL}={}", std::process::id()),
        ]);
        let network = match c.network {
            ContainerNetwork::None => "none",
            ContainerNetwork::Bridge => "bridge",
        };
        args.extend(
            [
                "--init",
                "--network",
                network,
                "--read-only",
                "--tmpfs",
                &format!("/tmp:rw,nosuid,nodev,mode=1777,size={}", c.tmp_size),
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--memory",
                &c.memory,
                "--memory-swap",
                &c.memory,
                "--pids-limit",
                &c.pids_limit.to_string(),
            ]
            .map(String::from),
        );
        if let Some(user) = &c.user {
            args.extend(["--user".to_string(), user.clone()]);
        }
        // The only environment the command gets: nothing of the host's, so no key.
        args.extend(["-e".to_string(), "HOME=/tmp".to_string()]);
        args.extend([
            "--mount".to_string(),
            format!("type=bind,source={workspace},target={workspace}"),
            "-w".to_string(),
            workspace.to_string(),
            c.image.clone(),
            "bash".to_string(),
            "-lc".to_string(),
            request.command.to_string(),
        ]);
        args
    }
}

struct Reply {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn take_text<R: Read + Send + 'static>(stream: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stream) = stream {
            let _ = stream.read_to_string(&mut text);
        }
        text
    })
}

/// A container that is removed when this goes out of scope, on every path, a panic included.
struct Container<'a> {
    runner: &'a ContainerRunner,
    name: String,
    /// Set when the container may exist (before `create` is asked), cleared once it is removed.
    exists: bool,
}

impl Container<'_> {
    /// Removes it and says if that failed, so the output can name the manual command.
    fn remove(&mut self) -> Option<String> {
        if !self.exists {
            return None;
        }
        self.exists = false;
        self.runner
            .remove(&self.name)
            .err()
            .map(|error| error.to_string())
    }
}

impl Drop for Container<'_> {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

/// What a finished container reports.
struct State {
    exit_code: i32,
    oom_killed: bool,
    status: String,
    error: String,
}

enum Flow {
    Ended(ExitStatus),
    Cancelled,
    TimedOut,
}

fn unique_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!(
        "banana-harness-cmd-{}-{}-{nanos:x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn cancelled(output: String) -> CommandOutput {
    CommandOutput {
        finish: Finish::Cancelled,
        kill_scope: "container",
        output,
    }
}

impl CommandRunner for ContainerRunner {
    fn run(&self, request: &CommandRequest<'_>) -> io::Result<CommandOutput> {
        if request.cancel.is_cancelled() {
            return Ok(cancelled(String::new()));
        }
        let workspace = request
            .workspace
            .to_str()
            .ok_or_else(|| io::Error::other("the workspace path is not valid UTF-8"))?;
        if workspace.contains(',') {
            return Err(io::Error::other(format!(
                "the workspace path {workspace} contains a comma, which Docker's --mount \
                 cannot take; move the workspace to a path without one"
            )));
        }

        // The image must already be here; this never pulls.
        self.check()?;
        if request.cancel.is_cancelled() {
            return Ok(cancelled(String::new()));
        }

        let name = unique_name();
        // From here the container may exist, so it is removed on every way out.
        let mut container = Container {
            runner: self,
            name: name.clone(),
            exists: true,
        };
        let args = self.create_args(&name, request, workspace);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let created = self.docker(&args, CREATE)?;
        if !created.ok {
            return Err(self.explain("create", &created.stderr, Some(request.workspace)));
        }
        if request.cancel.is_cancelled() {
            return Ok(cancelled(String::new()));
        }

        // One pipe for both streams, as a terminal would interleave them.
        let (mut reader, writer) = io::pipe()?;
        let mut client = Command::new(&self.config.docker)
            .args(["start", "-a", &name])
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer)
            .spawn()
            .map_err(|error| self.not_available(&error))?;
        let capture = Arc::new(Mutex::new(crate::runner::Capture::new(
            request.output_max_bytes,
        )));
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
        let flow = loop {
            match client.try_wait() {
                Ok(Some(status)) => break Flow::Ended(status),
                Ok(None) => {}
                Err(error) => {
                    self.kill(&name);
                    let _ = client.kill();
                    let _ = client.wait();
                    return Err(error);
                }
            }
            if request.cancel.is_cancelled() {
                break Flow::Cancelled;
            }
            if Instant::now() >= deadline {
                break Flow::TimedOut;
            }
            std::thread::sleep(POLL_INTERVAL);
        };

        if !matches!(flow, Flow::Ended(_)) {
            // The container itself is killed: that is what stops the command. The client, even a
            // stopped or hung one, is then given a moment and killed.
            self.kill(&name);
            let grace = Instant::now() + CLIENT_GRACE;
            while client.try_wait()?.is_none() && Instant::now() < grace {
                std::thread::sleep(POLL_INTERVAL);
            }
            if client.try_wait()?.is_none() {
                let _ = client.kill();
                let _ = client.wait();
            }
        }
        let _ = done_rx.recv_timeout(DRAIN_GRACE);
        let mut output = lock(&capture).render();

        let finish = match flow {
            Flow::Cancelled => Finish::Cancelled,
            Flow::TimedOut => Finish::TimedOut,
            Flow::Ended(status) => match self.state(&name) {
                Some(state) if state.status == "created" || !state.error.is_empty() => {
                    let said = if state.error.is_empty() {
                        output.as_str()
                    } else {
                        state.error.as_str()
                    };
                    return Err(self.explain("start", said, Some(request.workspace)));
                }
                Some(state) if state.oom_killed => Finish::OutOfMemory,
                Some(state) => Finish::Exited(state.exit_code),
                // The state could not be read: the client's own status is the best there is.
                None => status.code().map_or(Finish::Unknown, Finish::Exited),
            },
        };

        if let Some(problem) = container.remove() {
            output.push_str(&format!(
                "\n[warning: the container {name} could not be removed ({problem}); remove it \
                 with: docker rm -f {name}]\n"
            ));
        }
        Ok(CommandOutput {
            finish,
            kill_scope: "container",
            output,
        })
    }
}

impl ContainerRunner {
    /// The container's exit code, whether it was killed for memory, its status and its error.
    fn state(&self, name: &str) -> Option<State> {
        let format = "{{.State.ExitCode}} {{.State.OOMKilled}} {{.State.Status}} {{.State.Error}}";
        let reply = self
            .docker(&["inspect", "--format", format, name], QUICK)
            .ok()?;
        if !reply.ok {
            return None;
        }
        let mut parts = reply.stdout.trim_end().splitn(4, ' ');
        Some(State {
            exit_code: parts.next()?.parse().ok()?,
            oom_killed: parts.next()? == "true",
            status: parts.next()?.to_string(),
            error: parts.next().unwrap_or("").trim().to_string(),
        })
    }
}
