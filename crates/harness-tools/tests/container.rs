//! `ContainerRunner` against a fake `docker` (no daemon): the calls it makes, how commands end,
//! and that nothing of a command or its container is left behind.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use harness_testkit::FakeDocker;
use harness_tools::{
    COMMAND_LABEL, CancelToken, CommandOutput, CommandRequest, CommandRunner, ContainerConfig,
    ContainerNetwork, ContainerRunner, Finish, OWNER_LABEL,
};

const IMAGE: &str = "agent-image:1";

struct Setup {
    docker: FakeDocker,
    runner: ContainerRunner,
    workspace: tempfile::TempDir,
}

fn setup() -> Setup {
    setup_with(|_| {})
}

fn setup_with(configure: impl FnOnce(&mut ContainerConfig)) -> Setup {
    let docker = FakeDocker::new();
    docker.add_image(IMAGE);
    let mut config = ContainerConfig::new(IMAGE);
    config.docker = docker.executable().to_path_buf();
    configure(&mut config);
    Setup {
        runner: ContainerRunner::new(config),
        docker,
        workspace: tempfile::tempdir().unwrap(),
    }
}

fn run(
    s: &Setup,
    command: &str,
    timeout: Duration,
    cancel: &CancelToken,
) -> io::Result<(CommandOutput, Duration)> {
    run_hiding(s, command, timeout, &[], cancel)
}

use std::io;

fn run_hiding(
    s: &Setup,
    command: &str,
    timeout: Duration,
    hidden_env: &[&str],
    cancel: &CancelToken,
) -> io::Result<(CommandOutput, Duration)> {
    let started = Instant::now();
    let done = s.runner.run(&CommandRequest {
        command,
        workspace: s.workspace.path(),
        hidden_env,
        timeout,
        output_max_bytes: 30_000,
        cancel,
    })?;
    Ok((done, started.elapsed()))
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn pid_in(workspace: &Path, name: &str) -> Option<u32> {
    std::fs::read_to_string(workspace.join(name))
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

fn assert_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while alive(pid) {
        assert!(Instant::now() < deadline, "process {pid} is still running");
        thread::sleep(Duration::from_millis(20));
    }
}

const LONG: &str = "echo $$ > shell.pid; sleep 30 & echo $! > job.pid; wait";

fn assert_nothing_left(s: &Setup) {
    for name in ["shell.pid", "job.pid"] {
        if let Some(pid) = pid_in(s.workspace.path(), name) {
            assert_gone(pid);
        }
    }
    assert_eq!(
        s.docker.containers(),
        Vec::<String>::new(),
        "a container was left"
    );
}

fn after(delay: Duration, cancel: &CancelToken) -> thread::JoinHandle<()> {
    let cancel = cancel.clone();
    thread::spawn(move || {
        thread::sleep(delay);
        cancel.cancel();
    })
}

fn when_started(workspace: &Path, marker: &str, cancel: &CancelToken) -> thread::JoinHandle<()> {
    let (marker, cancel) = (workspace.join(marker), cancel.clone());
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(50));
        cancel.cancel();
    })
}

#[test]
fn a_command_is_one_container_made_started_inspected_and_removed_in_that_order() {
    let s = setup();
    let (done, _) = run(&s, "echo hi", Duration::from_secs(30), &CancelToken::new()).unwrap();
    assert_eq!(done.finish, Finish::Exited(0));
    assert_eq!(done.output, "hi\n");
    let order: Vec<String> = s.docker.calls().iter().map(|c| c[0].clone()).collect();
    assert_eq!(order, ["image", "create", "start", "inspect", "rm"]);
    assert_eq!(s.docker.containers(), Vec::<String>::new());
}

#[test]
fn the_container_is_made_with_the_approved_hardening_and_nothing_else() {
    let s = setup();
    run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap();
    let create = s.docker.calls_of("create").remove(0);
    let has = |pair: &[&str]| create.windows(pair.len()).any(|w| w == pair);
    assert!(has(&["--network", "none"]), "{create:?}");
    assert!(create.contains(&"--read-only".to_string()));
    assert!(
        create
            .iter()
            .any(|a| a.starts_with("/tmp:rw,") && a.contains("size=256m"))
    );
    assert!(has(&["--cap-drop", "ALL"]));
    assert!(has(&["--security-opt", "no-new-privileges"]));
    assert!(has(&["--memory", "2g"]));
    assert!(has(&["--memory-swap", "2g"]));
    assert!(has(&["--pids-limit", "256"]));
    assert!(create.contains(&"--user".to_string()));
    assert!(create.contains(&"--init".to_string()));
    // Labelled so a sweep can find it, named, and run as `bash -lc` in the workspace.
    let name = &create[create.iter().position(|a| a == "--name").unwrap() + 1];
    assert!(create.contains(&format!("{COMMAND_LABEL}={name}")));
    let workspace = s.workspace.path().to_str().unwrap();
    assert!(has(&["-w", workspace]));
    assert!(create.contains(&format!("type=bind,source={workspace},target={workspace}")));
    assert_eq!(&create[create.len() - 4..], [IMAGE, "bash", "-lc", "true"]);
    // The only environment variable, and no socket, no host home, no other mount.
    let env: Vec<&String> = create
        .windows(2)
        .filter(|w| w[0] == "-e")
        .map(|w| &w[1])
        .collect();
    assert_eq!(env, ["HOME=/tmp"]);
    assert_eq!(create.iter().filter(|a| *a == "--mount").count(), 1);
    assert!(
        !create
            .iter()
            .any(|a| a.contains("docker.sock") || a.contains("-v"))
    );
    assert!(!create.contains(&"--privileged".to_string()));
}

#[test]
fn the_network_is_on_only_when_it_is_asked_for() {
    let s = setup_with(|c| c.network = ContainerNetwork::Bridge);
    run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap();
    let create = s.docker.calls_of("create").remove(0);
    assert!(create.windows(2).any(|w| w == ["--network", "bridge"]));
}

#[test]
fn limits_and_the_image_come_from_the_configuration() {
    let s = setup_with(|c| {
        c.memory = "512m".into();
        c.pids_limit = 64;
        c.tmp_size = "16m".into();
        c.user = None;
    });
    run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap();
    let create = s.docker.calls_of("create").remove(0);
    assert!(create.windows(2).any(|w| w == ["--memory", "512m"]));
    assert!(create.windows(2).any(|w| w == ["--pids-limit", "64"]));
    assert!(create.iter().any(|a| a.contains("size=16m")));
    assert!(!create.contains(&"--user".to_string()));
}

#[test]
fn the_host_environment_and_the_hidden_variables_never_reach_the_container() {
    let s = setup();
    // The fake runs the command with an empty environment, as a container has.
    let (done, _) = run_hiding(
        &s,
        "echo \"[${AI_API_KEY-unset}] [${USER-unset}] [$HOME]\"",
        Duration::from_secs(30),
        &["AI_API_KEY"],
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(done.output, "[unset] [unset] [/tmp]\n");
    // Everything but the command text itself, which here names the variable it checks.
    let create = s.docker.calls_of("create").remove(0);
    assert!(
        !create[..create.len() - 1]
            .iter()
            .any(|a| a.contains("AI_API_KEY") || a.contains("sk-"))
    );
}

#[test]
fn exit_codes_both_streams_and_the_output_cap_match_the_host_runner() {
    let s = setup();
    let (done, _) = run(
        &s,
        "echo out; echo err >&2; exit 3",
        Duration::from_secs(30),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(done.finish, Finish::Exited(3));
    assert_eq!(done.output, "out\nerr\n");
    assert_eq!(done.kill_scope, "container");

    let mut request_cap = s.runner.run(&CommandRequest {
        command: "head -c 50000 /dev/zero | tr '\\0' x",
        workspace: s.workspace.path(),
        hidden_env: &[],
        timeout: Duration::from_secs(30),
        output_max_bytes: 1_000,
        cancel: &CancelToken::new(),
    });
    let done = request_cap.as_mut().unwrap();
    assert!(done.output.contains("bytes truncated"));
    assert!(done.output.len() < 1_200);
}

#[test]
fn a_timeout_kills_the_container_itself_and_removes_it() {
    let s = setup();
    let (done, took) = run(&s, LONG, Duration::from_secs(2), &CancelToken::new()).unwrap();
    assert_eq!(done.finish, Finish::TimedOut);
    assert!(took < Duration::from_secs(10), "took {took:?}");
    let kill = s.docker.calls_of("kill").remove(0);
    assert_eq!(&kill[..3], ["kill", "--signal", "KILL"]);
    assert_nothing_left(&s);
}

#[test]
fn cancelling_kills_the_container_promptly_and_leaves_nothing() {
    let s = setup();
    let cancel = CancelToken::new();
    let canceller = when_started(s.workspace.path(), "job.pid", &cancel);
    let started = Instant::now();
    let (done, _) = run(&s, LONG, Duration::from_secs(60), &cancel).unwrap();
    canceller.join().unwrap();
    assert_eq!(done.finish, Finish::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(s.docker.calls_of("kill").len(), 1);
    assert_nothing_left(&s);
}

#[test]
fn a_command_cancelled_before_anything_starts_makes_no_docker_call() {
    let s = setup();
    let cancel = CancelToken::new();
    cancel.cancel();
    let (done, _) = run(&s, "touch started", Duration::from_secs(30), &cancel).unwrap();
    assert_eq!(done.finish, Finish::Cancelled);
    assert!(s.docker.calls().is_empty());
    assert!(!s.workspace.path().join("started").exists());
}

#[test]
fn cancelling_during_startup_never_leaves_a_container_or_a_process() {
    for delay_ms in [0u64, 20, 60, 120, 200, 300, 450, 600, 800] {
        let s = setup();
        s.docker.set_flag("slow_create");
        let cancel = CancelToken::new();
        let canceller = after(Duration::from_millis(delay_ms), &cancel);
        let (done, took) = run(&s, LONG, Duration::from_secs(60), &cancel).unwrap();
        canceller.join().unwrap();
        assert_eq!(done.finish, Finish::Cancelled, "delay {delay_ms} ms");
        assert!(
            took < Duration::from_secs(20),
            "delay {delay_ms} ms took {took:?}"
        );
        assert_nothing_left(&s);
    }
}

#[test]
fn cancelling_kills_the_container_even_when_the_docker_client_is_stopped() {
    let s = setup();
    s.docker.set_flag("client_stops");
    let cancel = CancelToken::new();
    let canceller = when_started(s.workspace.path(), "job.pid", &cancel);
    let started = Instant::now();
    let (done, _) = run(&s, LONG, Duration::from_secs(60), &cancel).unwrap();
    canceller.join().unwrap();
    assert_eq!(done.finish, Finish::Cancelled);
    // The stopped client is killed after its grace, not waited for.
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        s.docker.calls_of("kill").len(),
        1,
        "the container was not killed"
    );
    assert_nothing_left(&s);
}

#[test]
fn a_timeout_with_a_stopped_client_still_stops_the_workload() {
    let s = setup();
    s.docker.set_flag("client_stops");
    let (done, took) = run(&s, LONG, Duration::from_secs(3), &CancelToken::new()).unwrap();
    assert_eq!(done.finish, Finish::TimedOut);
    assert!(took < Duration::from_secs(20), "took {took:?}");
    assert_nothing_left(&s);
}

#[test]
fn a_container_killed_for_memory_is_named_as_such() {
    let s = setup();
    s.docker.set_flag("oom");
    let (done, _) = run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap();
    assert_eq!(done.finish, Finish::OutOfMemory);
    assert_eq!(s.docker.containers(), Vec::<String>::new());
}

#[test]
fn a_missing_image_fails_with_the_pull_command_and_pulls_nothing() {
    let s = setup_with(|c| c.image = "absent:1".into());
    let error = run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap_err();
    let text = error.to_string();
    assert!(text.contains("docker pull absent:1"), "{text}");
    assert!(text.contains("never pulls"), "{text}");
    assert!(s.docker.calls_of("pull").is_empty());
    assert!(s.docker.calls_of("create").is_empty());
}

#[test]
fn a_stopped_daemon_fails_the_command_and_never_runs_it_on_the_host() {
    let s = setup();
    s.docker.set_flag("daemon_down");
    let error = run(
        &s,
        "touch ran",
        Duration::from_secs(30),
        &CancelToken::new(),
    )
    .unwrap_err();
    let text = error.to_string();
    assert!(text.contains("Docker is not running"), "{text}");
    assert!(text.contains("never run on the host"), "{text}");
    assert!(!s.workspace.path().join("ran").exists());
}

#[test]
fn a_missing_docker_client_says_so() {
    let s = setup_with(|c| c.docker = "/no/such/docker".into());
    let error = run(
        &s,
        "touch ran",
        Duration::from_secs(30),
        &CancelToken::new(),
    )
    .unwrap_err();
    let text = error.to_string();
    assert!(text.contains("`docker` command was not found"), "{text}");
    assert!(!s.workspace.path().join("ran").exists());
}

#[test]
fn a_workspace_docker_cannot_mount_is_explained() {
    let s = setup();
    s.docker.set_flag("mount_denied");
    let error = run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap_err();
    let text = error.to_string();
    assert!(text.contains("could not mount the workspace"), "{text}");
    assert!(text.contains("File sharing"), "{text}");
    assert_eq!(s.docker.containers(), Vec::<String>::new());
}

#[test]
fn a_workspace_path_with_a_comma_is_refused_before_docker_is_called() {
    let s = setup();
    let dir = s.workspace.path().join("a,b");
    std::fs::create_dir(&dir).unwrap();
    let error = s
        .runner
        .run(&CommandRequest {
            command: "true",
            workspace: &dir,
            hidden_env: &[],
            timeout: Duration::from_secs(30),
            output_max_bytes: 1_000,
            cancel: &CancelToken::new(),
        })
        .unwrap_err();
    assert!(error.to_string().contains("comma"));
    assert!(s.docker.calls().is_empty());
}

#[test]
fn a_container_that_fails_to_start_is_removed_and_reported() {
    let s = setup();
    s.docker.set_flag("start_fails");
    let error = run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap_err();
    assert!(error.to_string().contains("docker start failed"), "{error}");
    assert_eq!(s.docker.containers(), Vec::<String>::new());
}

#[test]
fn a_removal_that_fails_is_reported_with_the_manual_command() {
    let s = setup();
    s.docker.set_flag("rm_fails");
    let (done, _) = run(&s, "echo hi", Duration::from_secs(30), &CancelToken::new()).unwrap();
    assert_eq!(done.finish, Finish::Exited(0));
    assert!(done.output.starts_with("hi\n"), "{}", done.output);
    assert!(
        done.output.contains("could not be removed"),
        "{}",
        done.output
    );
    assert!(
        done.output.contains("docker rm -f banana-harness-cmd-"),
        "{}",
        done.output
    );
}

const DEAD_OWNER: u32 = 4_000_000;

#[test]
fn the_sweep_removes_leftovers_of_dead_owners_and_spares_everything_else() {
    let s = setup();
    s.docker
        .add_leftover("banana-harness-cmd-old-1", true, DEAD_OWNER);
    s.docker
        .add_leftover("banana-harness-cmd-old-2", true, DEAD_OWNER);
    // A command in progress of this very process, or of another sidecar that still runs.
    s.docker
        .add_leftover("banana-harness-cmd-live", true, std::process::id());
    s.docker.add_leftover("someone-elses", false, DEAD_OWNER);
    assert_eq!(s.runner.sweep_leftovers().unwrap(), 2);
    assert_eq!(
        s.docker.containers(),
        ["banana-harness-cmd-live", "someone-elses"]
    );
    let ps = s.docker.calls_of("ps").remove(0);
    assert!(ps.contains(&format!("label={COMMAND_LABEL}")));
    assert_eq!(s.runner.sweep_leftovers().unwrap(), 0);
}

#[test]
fn a_container_is_labelled_with_the_process_that_made_it() {
    let s = setup();
    run(&s, "true", Duration::from_secs(30), &CancelToken::new()).unwrap();
    let create = s.docker.calls_of("create").remove(0);
    assert!(create.contains(&format!("{OWNER_LABEL}={}", std::process::id())));
}

#[test]
fn the_sweep_reports_a_stopped_daemon_instead_of_pretending() {
    let s = setup();
    s.docker.set_flag("daemon_down");
    assert!(
        s.runner
            .sweep_leftovers()
            .unwrap_err()
            .to_string()
            .contains("not running")
    );
}
