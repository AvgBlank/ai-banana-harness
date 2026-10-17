//! `claim_stdout` is tested by re-executing this test binary as a child that prints a stray line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::process::Command;

#[test]
#[ignore = "run only as the child of `a_stray_print_does_not_reach_the_protocol_stream`"]
fn child_prints_a_stray_line() {
    let mut protocol = harness_bridge::claim_stdout().unwrap();
    println!("stray-print-from-a-dependency");
    writeln!(protocol, r#"{{"kind":"fatal"}}"#).unwrap();
}

#[test]
fn a_stray_print_does_not_reach_the_protocol_stream() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "child_prints_a_stray_line",
            "--nocapture",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains(r#"{"kind":"fatal"}"#),
        "the frame was lost: {stdout}"
    );
    assert!(
        !stdout.contains("stray-print"),
        "a stray print reached stdout: {stdout}"
    );
    assert!(
        stderr.contains("stray-print-from-a-dependency"),
        "the stray print vanished: {stderr}"
    );
}
