#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::TestHarness;
use harness_core::{Event, EventKind, Provider, TurnState, UndoOutcome};
use harness_testkit::{commit_all, fixture};
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

    let outcome = t.harness.undo(&session_id).unwrap().outcome;
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

    let report = t.harness.undo(&session_id).unwrap();
    assert_eq!(report.outcome, UndoOutcome::NothingToUndo);
    assert_eq!(report.event, None);
    // Undo is a strict no-op outside a git workspace: nothing is removed either.
    assert!(t.workspace.join("created.txt").exists());
}

fn turn_id_of(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::TurnCreated { turn_id, .. } => Some(turn_id.clone()),
            _ => None,
        })
        .expect("a turn.created event")
}

/// A committed `notes.txt` ("apple"), then one turn that edits it to "banana". Returns the
/// session id, that turn's id and its live events.
async fn edited_session(t: &TestHarness) -> (String, String, Vec<Event>) {
    std::fs::write(t.workspace.join("notes.txt"), "apple\n").unwrap();
    commit_all(&t.workspace, "seed");
    t.provider.push_sse(tool_call(
        "edit",
        json!({"path": "notes.txt", "old_string": "apple", "new_string": "banana"}),
    ));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (session_id, events) = t.run_turn("edit the file").await;
    assert_turn_done(&events);
    let turn_id = turn_id_of(&events);
    (session_id, turn_id, events)
}

fn notes(t: &TestHarness) -> String {
    std::fs::read_to_string(t.workspace.join("notes.txt")).unwrap()
}

fn undone_markers(t: &TestHarness, session_id: &str) -> usize {
    t.harness
        .transcript(session_id)
        .unwrap()
        .iter()
        .flat_map(|turn| &turn.events)
        .filter(|e| matches!(e.kind, EventKind::TurnUndone { .. }))
        .count()
}

#[tokio::test]
async fn undo_returns_the_turn_undone_event_and_persists_it_in_the_turns_log() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, turn_id, live) = edited_session(&t).await;

    let report = t.harness.undo(&session_id).unwrap();
    assert_eq!(report.outcome, UndoOutcome::Restored);
    let event = report
        .event
        .clone()
        .expect("a restored undo returns its event");
    assert_eq!(
        event.kind,
        EventKind::TurnUndone {
            turn_id: turn_id.clone()
        }
    );
    // Ids come from the one monotonic generator: strictly after everything the turn wrote.
    assert!(event.id > live.last().unwrap().id);

    let rows = t.reopen_store().events(&turn_id).unwrap();
    let last = rows.last().unwrap();
    assert_eq!(last.kind, "turn.undone");
    assert_eq!(last.id, event.id);
    assert_eq!(last.thread_id, None);
    assert_eq!(
        serde_json::from_str::<Event>(&last.payload_json).unwrap(),
        event
    );

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["outcome"], json!("restored"));
    assert_eq!(json["event"]["type"], json!("turn.undone"));
    assert_eq!(json["event"]["turn_id"], json!(turn_id));
}

#[tokio::test]
async fn undoing_twice_restores_once_and_never_clobbers_later_work() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, _, _) = edited_session(&t).await;
    assert_eq!(notes(&t), "banana\n");

    assert_eq!(
        t.harness.undo(&session_id).unwrap().outcome,
        UndoOutcome::Restored
    );
    assert_eq!(notes(&t), "apple\n");

    // Work done after the undo, which restoring the same stale checkpoint again would wipe out.
    std::fs::write(t.workspace.join("notes.txt"), "my own edit\n").unwrap();
    let second = t.harness.undo(&session_id).unwrap();
    assert_eq!(second.outcome, UndoOutcome::NothingToUndo);
    assert_eq!(second.event, None);
    assert_eq!(notes(&t), "my own edit\n");

    assert_eq!(undone_markers(&t, &session_id), 1);
}

#[tokio::test]
async fn a_later_turn_can_be_undone_after_an_earlier_one_was() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, first_turn, _) = edited_session(&t).await;
    t.harness.undo(&session_id).unwrap();
    assert_eq!(notes(&t), "apple\n");

    // The marker belongs to the first turn only; the next turn has its own checkpoint.
    t.provider.push_sse(tool_call(
        "edit",
        json!({"path": "notes.txt", "old_string": "apple", "new_string": "cherry"}),
    ));
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let events = common::collect(
        t.harness
            .start_turn(&session_id, "edit it again".to_string()),
    )
    .await;
    let second_turn = turn_id_of(&events);
    assert_eq!(notes(&t), "cherry\n");

    let report = t.harness.undo(&session_id).unwrap();
    assert_eq!(report.outcome, UndoOutcome::Restored);
    assert_eq!(notes(&t), "apple\n");
    assert_eq!(
        report.event.unwrap().kind,
        EventKind::TurnUndone {
            turn_id: second_turn.clone()
        }
    );
    assert_ne!(first_turn, second_turn);
    assert_eq!(undone_markers(&t, &session_id), 2);
}

#[tokio::test]
async fn a_turn_that_changed_nothing_has_nothing_to_undo() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    let (session_id, _) = t.run_turn("just answer").await;

    let report = t.harness.undo(&session_id).unwrap();
    assert_eq!(report.outcome, UndoOutcome::NothingToUndo);
    assert_eq!(report.event, None);
    assert_eq!(undone_markers(&t, &session_id), 0);

    // A session with no turns has nothing to undo either.
    let empty = t.harness.create_session().unwrap();
    assert_eq!(
        t.harness.undo(&empty).unwrap().outcome,
        UndoOutcome::NothingToUndo
    );
}

#[tokio::test]
async fn the_marker_survives_a_restart_shows_in_the_transcript_and_blocks_a_second_undo() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    let (session_id, turn_id, _) = edited_session(&t).await;
    let report = t.harness.undo(&session_id).unwrap();
    let event = report.event.unwrap();

    let transcript = t.harness.transcript(&session_id).unwrap();
    let names: Vec<_> = transcript[0].events.iter().map(|e| e.kind.name()).collect();
    assert_eq!(names.iter().filter(|n| **n == "turn.undone").count(), 1);
    // After the turn's own `turn.done`, as the last thing in its log.
    assert_eq!(&names[names.len() - 2..], ["turn.done", "turn.undone"]);
    assert_eq!(transcript[0].events.last(), Some(&event));
    assert_eq!(transcript[0].turn_id, turn_id);
    assert_eq!(transcript[0].state, Some(TurnState::Done));

    // A new harness on the same database, as after a restart: same transcript, still undone.
    std::fs::write(t.workspace.join("notes.txt"), "after restart\n").unwrap();
    let reopened = t.reopen(Provider::DeepSeek).await;
    assert_eq!(reopened.transcript(&session_id).unwrap(), transcript);
    assert_eq!(
        reopened.undo(&session_id).unwrap().outcome,
        UndoOutcome::NothingToUndo
    );
    assert_eq!(notes(&t), "after restart\n");
}
