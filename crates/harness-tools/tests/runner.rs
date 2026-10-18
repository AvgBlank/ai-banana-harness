//! `HostRunner` through the crate's public surface: how commands end, how cancellation and the
//! timeout stop them, and that nothing of a command is left running afterwards.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use harness_tools::{
    CancelToken, CommandOutput, CommandRequest, CommandRunner, Finish, HostRunner,
};

fn run(
    command: &str,
    workspace: &Path,
    timeout: Duration,
    cancel: &CancelToken,
) -> (CommandOutput, Duration) {
    run_capped(command, workspace, timeout, 30_000, &[], cancel)
}

fn run_capped(
    command: &str,
    workspace: &Path,
    timeout: Duration,
    output_max_bytes: usize,
    hidden_env: &[&str],
    cancel: &CancelToken,
) -> (CommandOutput, Duration) {
    let started = Instant::now();
    let done = HostRunner
        .run(&CommandRequest {
            command,
            workspace,
            hidden_env,
            timeout,
            output_max_bytes,
            cancel,
        })
        .unwrap();
    (done, started.elapsed())
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

/// The pid a command wrote to `name` in the workspace, if it got that far.
fn pid_in(workspace: &Path, name: &str) -> Option<u32> {
    std::fs::read_to_string(workspace.join(name))
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

/// Waits for a process that was killed to be gone (it may be reaped a moment later).
fn assert_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while alive(pid) {
        assert!(Instant::now() < deadline, "process {pid} is still running");
        thread::sleep(Duration::from_millis(20));
    }
}

/// A command that records its own pid and the pid of a background job, then waits for the job.
const LONG: &str = "echo $$ > shell.pid; sleep 30 & echo $! > job.pid; wait";

fn assert_nothing_left(workspace: &Path) {
    for name in ["shell.pid", "job.pid"] {
        if let Some(pid) = pid_in(workspace, name) {
            assert_gone(pid);
        }
    }
}

/// Cancels once the command has created `marker` in the workspace (the login shell's start-up
/// time varies from machine to machine, so a fixed delay would test nothing), plus `extra`.
fn when_started(
    workspace: &Path,
    marker: &str,
    extra: Duration,
    cancel: &CancelToken,
) -> thread::JoinHandle<()> {
    let (marker, cancel) = (workspace.join(marker), cancel.clone());
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(extra);
        cancel.cancel();
    })
}

fn after(delay: Duration, cancel: &CancelToken) -> thread::JoinHandle<()> {
    let cancel = cancel.clone();
    thread::spawn(move || {
        thread::sleep(delay);
        cancel.cancel();
    })
}

#[test]
fn a_command_that_exits_reports_its_code_and_both_streams_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run(
        "echo out; echo err >&2; exit 3",
        dir.path(),
        Duration::from_secs(10),
        &CancelToken::new(),
    );
    assert_eq!(done.finish, Finish::Exited(3));
    assert_eq!(done.output, "out\nerr\n");
}

#[test]
fn a_command_ended_by_a_signal_reports_the_signal() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run(
        "kill -9 $$",
        dir.path(),
        Duration::from_secs(10),
        &CancelToken::new(),
    );
    assert_eq!(done.finish, Finish::Signaled(9));
}

#[test]
fn the_command_runs_in_the_workspace_and_without_the_hidden_environment() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run_capped(
        "pwd -P; echo \"[${HOME-unset}]\"",
        dir.path(),
        Duration::from_secs(10),
        30_000,
        &["HOME"],
        &CancelToken::new(),
    );
    let expected = dir.path().canonicalize().unwrap();
    // A login shell may print its own start-up noise first; the command's output is the tail.
    assert!(
        done.output
            .ends_with(&format!("{}\n[unset]\n", expected.display())),
        "{}",
        done.output
    );
}

#[test]
fn long_output_keeps_both_ends_and_counts_what_it_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run_capped(
        "head -c 50000 /dev/zero | tr '\\0' x",
        dir.path(),
        Duration::from_secs(10),
        1_000,
        &[],
        &CancelToken::new(),
    );
    assert_eq!(done.finish, Finish::Exited(0));
    assert!(done.output.contains("bytes truncated"), "{}", done.output);
    assert!(done.output.len() < 1_200);
}

#[test]
fn the_timeout_kills_the_command_and_everything_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let (done, took) = run(
        LONG,
        dir.path(),
        Duration::from_secs(1),
        &CancelToken::new(),
    );
    assert_eq!(done.finish, Finish::TimedOut);
    assert!(took < Duration::from_secs(4), "took {took:?}");
    assert_nothing_left(dir.path());
}

#[test]
fn a_background_job_left_behind_by_a_normal_exit_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run(
        "sleep 30 & echo $! > job.pid",
        dir.path(),
        Duration::from_secs(10),
        &CancelToken::new(),
    );
    assert_eq!(done.finish, Finish::Exited(0));
    assert_gone(pid_in(dir.path(), "job.pid").unwrap());
}

#[test]
fn cancelling_stops_a_long_command_promptly_and_leaves_nothing_running() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    let canceller = when_started(dir.path(), "job.pid", Duration::from_millis(50), &cancel);
    let started = Instant::now();
    let (done, _) = run(LONG, dir.path(), Duration::from_secs(60), &cancel);
    canceller.join().unwrap();
    assert_eq!(done.finish, Finish::Cancelled);
    // Natural completion would take 30 s more than the start-up.
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "took {:?}",
        started.elapsed()
    );
    assert!(
        pid_in(dir.path(), "job.pid").is_some(),
        "the command had not started"
    );
    assert_nothing_left(dir.path());
}

#[test]
fn output_written_before_the_cancel_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    let canceller = when_started(dir.path(), "ready", Duration::from_millis(50), &cancel);
    let (done, _) = run(
        "echo before; touch ready; sleep 30",
        dir.path(),
        Duration::from_secs(60),
        &cancel,
    );
    canceller.join().unwrap();
    assert_eq!(done.finish, Finish::Cancelled);
    assert!(done.output.ends_with("before\n"), "{}", done.output);
}

#[test]
fn a_command_cancelled_before_it_starts_is_never_spawned() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    let (done, took) = run(
        "touch started",
        dir.path(),
        Duration::from_secs(10),
        &cancel,
    );
    assert_eq!(done.finish, Finish::Cancelled);
    assert_eq!(done.output, "");
    assert!(!dir.path().join("started").exists());
    assert!(took < Duration::from_secs(1));
}

/// Cancelling at every moment of the startup: the command takes 30 s if left alone, so each run
/// must end cancelled, and whatever it had started by then must be gone.
#[test]
fn cancelling_during_startup_never_leaves_a_process_behind() {
    for delay_ms in 0..30u64 {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        let canceller = after(Duration::from_millis(delay_ms), &cancel);
        let (done, took) = run(LONG, dir.path(), Duration::from_secs(60), &cancel);
        canceller.join().unwrap();
        assert_eq!(done.finish, Finish::Cancelled, "delay {delay_ms} ms");
        assert!(
            took < Duration::from_secs(3),
            "delay {delay_ms} ms took {took:?}"
        );
        assert_nothing_left(dir.path());
    }
}

/// Cancelling while a short command finishes: it ends either way, never hangs, and a job it
/// left in the background is killed in both cases.
#[test]
fn cancelling_as_a_command_finishes_ends_one_way_or_the_other_without_leftovers() {
    const SHORT: &str = "sleep 30 & echo $! > job.pid; echo done";
    // How long the command takes alone on this machine (a login shell's start-up varies).
    let dir = tempfile::tempdir().unwrap();
    let (_, natural) = run(
        SHORT,
        dir.path(),
        Duration::from_secs(60),
        &CancelToken::new(),
    );
    let mut finishes = Vec::new();
    // From before it starts to after it ended, in 16 steps.
    for step in 0..=16u32 {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        let canceller = after(natural * 2 * step / 16, &cancel);
        let (done, took) = run(SHORT, dir.path(), Duration::from_secs(60), &cancel);
        canceller.join().unwrap();
        assert!(
            matches!(done.finish, Finish::Exited(0) | Finish::Cancelled),
            "step {step}: {:?}",
            done.finish
        );
        assert!(took < Duration::from_secs(5), "step {step} took {took:?}");
        if let Some(pid) = pid_in(dir.path(), "job.pid") {
            assert_gone(pid);
        }
        finishes.push(done.finish);
    }
    // An early cancel wins. (That a late one finds the command finished depends on this
    // machine's timing, so it is allowed above but not required.)
    assert!(finishes.contains(&Finish::Cancelled), "{finishes:?}");
}

#[test]
fn a_cancel_and_a_timeout_together_end_the_command_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    let canceller = after(Duration::from_millis(300), &cancel);
    let (done, took) = run(LONG, dir.path(), Duration::from_millis(300), &cancel);
    canceller.join().unwrap();
    assert!(
        matches!(done.finish, Finish::Cancelled | Finish::TimedOut),
        "{:?}",
        done.finish
    );
    assert!(took < Duration::from_secs(3), "took {took:?}");
    assert_nothing_left(dir.path());
}

#[test]
fn one_token_stops_every_runner_that_holds_it() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    let canceller = after(Duration::from_millis(300), &cancel);
    thread::scope(|scope| {
        let handles: Vec<_> = (0..3)
            .map(|_| scope.spawn(|| run("sleep 30", dir.path(), Duration::from_secs(60), &cancel)))
            .collect();
        for handle in handles {
            let (done, took) = handle.join().unwrap();
            assert_eq!(done.finish, Finish::Cancelled);
            assert!(took < Duration::from_secs(3));
        }
    });
    canceller.join().unwrap();
}
