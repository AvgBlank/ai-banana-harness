//! `ContainerRunner` against a real Docker daemon. Every test here is ignored unless asked for:
//!
//! ```text
//! BANANA_TEST_DOCKER_IMAGE=debian:bookworm-slim \
//!   cargo test -p harness-tools --test container_docker -- --ignored
//! ```
//!
//! The image must already be on the machine (the runner never pulls) and have bash. A richer
//! image (git, make, python3, npm) makes the "ordinary tools under a read-only root" test check
//! more. The ownership test is what settles uid mapping: macOS Docker Desktop maps ownership
//! itself, a Linux daemon does not, so it must run on Linux CI before that is claimed.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use harness_tools::{
    COMMAND_LABEL, CancelToken, CommandRequest, CommandRunner, ContainerConfig, ContainerRunner,
    Finish, OWNER_LABEL,
};

/// The sweep and the leftover checks look at every container this runner labels, so these tests
/// run one at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn image() -> String {
    std::env::var("BANANA_TEST_DOCKER_IMAGE")
        .expect("set BANANA_TEST_DOCKER_IMAGE to a local image with bash")
}

fn runner(configure: impl FnOnce(&mut ContainerConfig)) -> ContainerRunner {
    let mut config = ContainerConfig::new(image());
    configure(&mut config);
    ContainerRunner::new(config)
}

fn run(
    runner: &ContainerRunner,
    workspace: &Path,
    command: &str,
    timeout: Duration,
    cancel: &CancelToken,
) -> std::io::Result<harness_tools::CommandOutput> {
    runner.run(&CommandRequest {
        command,
        workspace,
        hidden_env: &["AI_API_KEY"],
        timeout,
        output_max_bytes: 30_000,
        cancel,
    })
}

fn docker(args: &[&str]) -> String {
    let out = Command::new("docker").args(args).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn leftovers() -> Vec<String> {
    docker(&["ps", "-aq", "--filter", &format!("label={COMMAND_LABEL}")])
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn workspace() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn output_exit_codes_and_the_working_directory() {
    let _serial = serial();
    let dir = workspace();
    let r = runner(|_| {});
    let done = run(
        &r,
        dir.path(),
        "pwd; echo out; echo err >&2; exit 3",
        Duration::from_secs(60),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(done.finish, Finish::Exited(3));
    let expected = dir.path().to_str().unwrap();
    assert_eq!(
        done.output,
        format!("{expected}\nout\nerr\n"),
        "mounted at its own path"
    );
    assert!(leftovers().is_empty());
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn the_root_is_read_only_and_only_tmp_and_the_workspace_are_writable() {
    let _serial = serial();
    let dir = workspace();
    let r = runner(|_| {});
    let done = run(
        &r,
        dir.path(),
        "touch /rootfile 2>&1 | head -1; touch /tmp/ok && echo tmp-ok; touch in-workspace && echo workspace-ok; echo \"home=$HOME\"",
        Duration::from_secs(60),
        &CancelToken::new(),
    )
    .unwrap();
    assert!(
        done.output.contains("Read-only file system"),
        "{}",
        done.output
    );
    assert!(done.output.contains("tmp-ok"), "{}", done.output);
    assert!(done.output.contains("workspace-ok"), "{}", done.output);
    assert!(done.output.contains("home=/tmp"), "{}", done.output);
    assert!(dir.path().join("in-workspace").exists());
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn files_written_in_the_workspace_belong_to_the_host_user() {
    let _serial = serial();
    let dir = workspace();
    let r = runner(|_| {});
    let done = run(
        &r,
        dir.path(),
        "id -u; touch owned",
        Duration::from_secs(60),
        &CancelToken::new(),
    )
    .unwrap();
    let uid = std::fs::metadata(dir.path()).unwrap().uid();
    let me = Command::new("id").arg("-u").output().unwrap().stdout;
    let me = String::from_utf8_lossy(&me).trim().to_string();
    // The command runs as the host's uid (a Linux daemon applies it literally); what it wrote
    // is owned by that user on the host (Docker Desktop maps ownership, a Linux daemon does not).
    assert_eq!(done.output.lines().next().unwrap(), me, "{}", done.output);
    assert_eq!(
        std::fs::metadata(dir.path().join("owned"))
            .unwrap()
            .uid()
            .to_string(),
        me
    );
    assert_eq!(uid.to_string(), me);
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn there_is_no_network_by_default_and_no_key_in_the_environment() {
    let _serial = serial();
    let dir = workspace();
    let r = runner(|_| {});
    let done = run(
        &r,
        dir.path(),
        "ls /sys/class/net | tr '\\n' ' '; echo; (exec 3<>/dev/tcp/1.1.1.1/53) 2>&1 | head -1; env | grep -c AI_API_KEY",
        Duration::from_secs(60),
        &CancelToken::new(),
    )
    .unwrap();
    // The VM's kernel lists unconfigured tunnel devices; what must be missing is a real interface.
    let interfaces = done.output.lines().next().unwrap();
    assert!(
        interfaces.split_whitespace().any(|i| i == "lo"),
        "{}",
        done.output
    );
    assert!(
        !interfaces.split_whitespace().any(|i| i.starts_with("eth")),
        "{}",
        done.output
    );
    assert!(
        done.output.contains("Network is unreachable")
            || done.output.contains("No route")
            || done.output.contains("failed"),
        "{}",
        done.output
    );
    assert!(done.output.trim_end().ends_with('0'), "{}", done.output);
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn a_command_over_the_memory_limit_is_reported_as_such() {
    let _serial = serial();
    let dir = workspace();
    let r = runner(|c| c.memory = "64m".into());
    let done = run(
        &r,
        dir.path(),
        "a=xxxxxxxxxx; while :; do a=\"$a$a\"; done",
        Duration::from_secs(120),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(done.finish, Finish::OutOfMemory, "{}", done.output);
    assert!(leftovers().is_empty());
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn a_timeout_and_a_cancel_kill_the_container_and_leave_none() {
    let _serial = serial();
    let dir = workspace();
    let r = runner(|_| {});
    let started = Instant::now();
    let done = run(
        &r,
        dir.path(),
        "sleep 60",
        Duration::from_secs(3),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(done.finish, Finish::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    assert!(leftovers().is_empty());

    let cancel = CancelToken::new();
    let c = cancel.clone();
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1500));
        c.cancel();
    });
    let started = Instant::now();
    let done = run(
        &r,
        dir.path(),
        "sleep 60",
        Duration::from_secs(120),
        &cancel,
    )
    .unwrap();
    canceller.join().unwrap();
    assert_eq!(done.finish, Finish::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    assert!(leftovers().is_empty());
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn the_sweep_removes_leftovers_of_dead_owners_and_spares_live_and_other_containers() {
    let _serial = serial();
    let r = runner(|_| {});
    let image = image();
    let label = |owner: &str| format!("--label={COMMAND_LABEL}=x --label={OWNER_LABEL}={owner}");
    let mine = std::process::id().to_string();
    for (name, labels) in [
        ("bh-test-dead", label("4000000")),
        ("bh-test-live", label(&mine)),
        ("bh-test-other", String::new()),
    ] {
        let mut args = vec!["create".to_string(), "--name".to_string(), name.to_string()];
        args.extend(labels.split_whitespace().map(str::to_string));
        args.extend([image.clone(), "true".to_string()]);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        docker(&refs);
    }
    let removed = r.sweep_leftovers().unwrap();
    assert!(removed >= 1);
    assert_eq!(docker(&["ps", "-aq", "--filter", "name=bh-test-dead"]), "");
    assert_ne!(docker(&["ps", "-aq", "--filter", "name=bh-test-live"]), "");
    assert_ne!(docker(&["ps", "-aq", "--filter", "name=bh-test-other"]), "");
    docker(&["rm", "-f", "bh-test-live", "bh-test-other"]);
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn a_missing_image_fails_with_the_pull_command_and_is_not_pulled() {
    let _serial = serial();
    let dir = workspace();
    let missing = "banana-harness-no-such-image:0";
    let r = ContainerRunner::new(ContainerConfig::new(missing));
    let error = run(
        &r,
        dir.path(),
        "true",
        Duration::from_secs(60),
        &CancelToken::new(),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&format!("docker pull {missing}")),
        "{error}"
    );
    assert!(
        !Command::new("docker")
            .args(["image", "inspect", missing])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
#[ignore = "needs Docker: set BANANA_TEST_DOCKER_IMAGE"]
fn ordinary_tools_work_under_the_read_only_root() {
    let _serial = serial();
    let dir = workspace();
    let r = runner(|_| {});
    let mut checked = Vec::new();
    for tool in ["git", "make", "python3", "npm"] {
        let probe = run(
            &r,
            dir.path(),
            &format!("command -v {tool}"),
            Duration::from_secs(60),
            &CancelToken::new(),
        )
        .unwrap();
        if probe.finish != Finish::Exited(0) {
            continue;
        }
        let done = run(
            &r,
            dir.path(),
            &format!("{tool} --version"),
            Duration::from_secs(60),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(done.finish, Finish::Exited(0), "{tool}: {}", done.output);
        checked.push(tool);
    }
    // Work that writes under HOME or /tmp (caches, scratch files) must not need a writable root.
    let done = run(
        &r,
        dir.path(),
        "mkdir -p \"$HOME/.cache/x\" && echo cache-ok",
        Duration::from_secs(60),
        &CancelToken::new(),
    )
    .unwrap();
    assert!(done.output.contains("cache-ok"), "{}", done.output);
    eprintln!("tools present in the image and run under the read-only root: {checked:?}");
}
