#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use common::{TestHarness, fixture};
use harness_core::{Provider, TurnState, UndoOutcome};
use serde_json::{Value, json};

/// One DeepSeek tool-call reply asking for `name(arguments)`.
fn tool_call(name: &str, arguments: Value) -> String {
    let escaped = serde_json::to_string(&arguments.to_string()).unwrap();
    fixture("deepseek_tool_call.sse")
        .replace("__NAME__", name)
        .replace("__ARGS__", escaped.trim_matches('"'))
}

fn assert_turn_done(events: &[harness_core::Event]) {
    assert!(matches!(
        events.last().map(|e| &e.kind),
        Some(harness_core::EventKind::TurnDone {
            state: TurnState::Done,
            error: None
        })
    ));
}

fn git(workspace: &Path, args: &[&str]) {
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(args)
            .status()
            .unwrap()
            .success(),
        "git {args:?} failed in {}",
        workspace.display()
    );
}

fn commit_all(workspace: &Path, message: &str) {
    git(workspace, &["add", "-A"]);
    git(
        workspace,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

#[tokio::test]
async fn undo_restores_the_edited_file_and_removes_the_created_one() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::write(t.workspace.join("notes.txt"), "apple\n").unwrap();
    commit_all(&t.workspace, "seed");

    t.provider.push_sse(tool_call(
        "edit",
        json!({"path": "notes.txt", "old_string": "apple", "new_string": "banana"}),
    ));
    t.provider.push_sse(tool_call(
        "write_file",
        json!({"path": "created.txt", "content": "new\n"}),
    ));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (session_id, events) = t.run_turn("edit the file and create another").await;
    assert_turn_done(&events);
    assert_eq!(
        std::fs::read_to_string(t.workspace.join("notes.txt")).unwrap(),
        "banana\n"
    );
    assert!(t.workspace.join("created.txt").exists());

    let outcome = t.harness.undo(&session_id).unwrap();
    assert_eq!(outcome, UndoOutcome::Restored);
    assert_eq!(
        std::fs::read_to_string(t.workspace.join("notes.txt")).unwrap(),
        "apple\n",
        "the edit must be undone"
    );
    assert!(
        !t.workspace.join("created.txt").exists(),
        "the created file must be removed"
    );
}

#[tokio::test]
async fn non_git_workspace_has_no_checkpoint_and_undo_reports_unavailable() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::remove_dir_all(t.workspace.join(".git")).unwrap();

    t.provider.push_sse(tool_call(
        "write_file",
        json!({"path": "created.txt", "content": "new\n"}),
    ));
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (session_id, events) = t.run_turn("create a file").await;
    assert_turn_done(&events);
    assert!(t.workspace.join("created.txt").exists());

    let outcome = t.harness.undo(&session_id).unwrap();
    assert_eq!(outcome, UndoOutcome::NothingToUndo);
    // Undo is a strict no-op outside a git workspace: nothing is removed either.
    assert!(t.workspace.join("created.txt").exists());
}
