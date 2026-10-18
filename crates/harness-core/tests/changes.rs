#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use common::{TestHarness, collect};
use harness_core::{
    ChangeStatus, Config, EventKind, FileChange, Harness, HarnessOptions, Provider, Store,
    TurnState, UndoOutcome,
};
use harness_testkit::{TEST_API_KEY, commit_all, git, sse_text, sse_tool_calls};
use serde_json::{Value, json};

fn write(workspace: &Path, name: &str, content: &str) {
    let path = workspace.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn by_path<'a>(files: &'a [FileChange], path: &str) -> &'a FileChange {
    files.iter().find(|f| f.path == path).unwrap_or_else(|| {
        panic!(
            "{path} not in {:?}",
            files.iter().map(|f| &f.path).collect::<Vec<_>>()
        )
    })
}

fn summary(files: &[FileChange]) -> Vec<(&str, ChangeStatus, u64, u64)> {
    files
        .iter()
        .map(|f| (f.path.as_str(), f.status, f.added, f.removed))
        .collect()
}

fn patch(file: &FileChange) -> &str {
    file.patch.as_deref().expect("a patch")
}

// ---------------------------------------------------------------- the workspace against HEAD

#[tokio::test]
async fn workspace_changes_lists_modified_deleted_and_untracked_files_with_their_patches() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "kept.txt", "a\nb\nc\n");
    write(&t.workspace, "gone.txt", "x\ny\n");
    write(&t.workspace, "sql.txt", "-- sql comment\nkeep\n");
    commit_all(&t.workspace, "seed");

    write(&t.workspace, "kept.txt", "a\nB\nc\nd\n");
    std::fs::remove_file(t.workspace.join("gone.txt")).unwrap();
    // Hunk lines that look like file headers: a removed `-- ...` line and an added `++ ...` one.
    write(&t.workspace, "sql.txt", "++ plus\nkeep\n");
    write(&t.workspace, "new.txt", "x\ny\n");

    let changes = t.harness.workspace_changes().unwrap();
    assert!(changes.git);
    assert_eq!(
        summary(&changes.files),
        [
            ("gone.txt", ChangeStatus::Deleted, 0, 2),
            ("kept.txt", ChangeStatus::Modified, 2, 1),
            ("sql.txt", ChangeStatus::Modified, 1, 1),
            ("new.txt", ChangeStatus::Added, 2, 0),
        ]
    );
    assert!(changes.files.iter().all(|f| !f.truncated));

    let kept = patch(by_path(&changes.files, "kept.txt"));
    assert!(
        kept.starts_with("diff --git a/kept.txt b/kept.txt\n"),
        "{kept}"
    );
    for expected in ["@@ -1,3 +1,4 @@", "\n-b\n", "\n+B\n", "\n+d\n"] {
        assert!(kept.contains(expected), "`{expected}` missing from {kept}");
    }
    let gone = patch(by_path(&changes.files, "gone.txt"));
    assert!(
        gone.contains("deleted file mode") && gone.contains("\n-x\n-y\n"),
        "{gone}"
    );
    let new = patch(by_path(&changes.files, "new.txt"));
    for expected in [
        "new file mode",
        "--- /dev/null",
        "+++ b/new.txt",
        "\n+x\n+y\n",
    ] {
        assert!(new.contains(expected), "`{expected}` missing from {new}");
    }
    // The counts are git's, not a scan of the patch, so the lookalike lines cannot skew them.
    let sql = patch(by_path(&changes.files, "sql.txt"));
    assert!(
        sql.contains("\n--- sql comment\n") && sql.contains("\n+++ plus\n"),
        "{sql}"
    );
}

#[tokio::test]
async fn binary_files_have_zero_counts_and_no_patch() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::write(t.workspace.join("tracked.bin"), [0u8, 1, 2, 3]).unwrap();
    commit_all(&t.workspace, "seed");
    std::fs::write(t.workspace.join("tracked.bin"), [0u8, 9, 9, 9]).unwrap();
    std::fs::write(t.workspace.join("fresh.bin"), [0u8, 255, 254]).unwrap();

    let changes = t.harness.workspace_changes().unwrap();
    assert_eq!(
        summary(&changes.files),
        [
            ("tracked.bin", ChangeStatus::Modified, 0, 0),
            ("fresh.bin", ChangeStatus::Added, 0, 0),
        ]
    );
    for file in &changes.files {
        assert_eq!(file.patch, None, "{}", file.path);
        assert!(!file.truncated);
    }
}

#[tokio::test]
async fn a_renamed_file_is_listed_under_its_new_path_with_the_old_one_in_its_patch() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(
        &t.workspace,
        "old name.txt",
        "rename me\nline2\nline3\nline4\n",
    );
    commit_all(&t.workspace, "seed");
    git(&t.workspace, &["mv", "old name.txt", "new name.txt"]);
    write(
        &t.workspace,
        "new name.txt",
        "rename me\nline2 CHANGED\nline3\nline4\n",
    );

    let files = t.harness.workspace_changes().unwrap().files;
    assert_eq!(
        summary(&files),
        [("new name.txt", ChangeStatus::Renamed, 1, 1)]
    );
    let rename = patch(&files[0]);
    assert!(rename.contains("rename from old name.txt"), "{rename}");
    assert!(rename.contains("rename to new name.txt"), "{rename}");
    assert!(rename.contains("\n+line2 CHANGED\n"), "{rename}");

    // A pure rename has nothing to count, but is still a change with a patch.
    write(
        &t.workspace,
        "new name.txt",
        "rename me\nline2\nline3\nline4\n",
    );
    let files = t.harness.workspace_changes().unwrap().files;
    assert_eq!(
        summary(&files),
        [("new name.txt", ChangeStatus::Renamed, 0, 0)]
    );
    assert!(patch(&files[0]).contains("similarity index 100%"));
}

#[tokio::test]
async fn a_workspace_outside_git_has_git_false_and_no_files() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::remove_dir_all(t.workspace.join(".git")).unwrap();
    write(&t.workspace, "loose.txt", "not tracked by anything\n");

    let changes = t.harness.workspace_changes().unwrap();
    assert!(!changes.git);
    assert!(changes.files.is_empty());
}

#[tokio::test]
async fn a_repository_with_no_commits_lists_its_files_as_added() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "staged.txt", "one\n");
    git(&t.workspace, &["add", "staged.txt"]);
    write(&t.workspace, "loose.txt", "two\nthree\n");

    let changes = t.harness.workspace_changes().unwrap();
    assert!(changes.git);
    assert_eq!(
        summary(&changes.files),
        [
            ("staged.txt", ChangeStatus::Added, 1, 0),
            ("loose.txt", ChangeStatus::Added, 2, 0),
        ]
    );
}

#[tokio::test]
async fn a_patch_over_256_kib_is_cut_at_a_line_and_flagged_while_counts_stay_exact() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "grown.txt", "one line\n");
    commit_all(&t.workspace, "seed");
    let big: String = (0..20_000)
        .map(|i| format!("{i:06} {}\n", "x".repeat(30)))
        .collect();
    write(&t.workspace, "grown.txt", &big);
    write(&t.workspace, "fresh.txt", &big);
    write(&t.workspace, "small.txt", "tiny\n");

    let files = t.harness.workspace_changes().unwrap().files;
    for (path, removed) in [("grown.txt", 1), ("fresh.txt", 0)] {
        let file = by_path(&files, path);
        assert!(file.truncated, "{path}");
        assert_eq!((file.added, file.removed), (20_000, removed), "{path}");
        let text = patch(file);
        assert!(text.len() <= 256 * 1024, "{path}: {} bytes", text.len());
        assert!(
            text.len() > 200 * 1024,
            "{path} was cut far below the limit"
        );
        assert!(text.ends_with('\n'), "{path} is cut mid-line");
    }
    let small = by_path(&files, "small.txt");
    assert!(!small.truncated);
    assert!(patch(small).ends_with("+tiny\n"));
}

fn harness_at(workspace: &Path, db: &Path) -> Harness {
    Harness::open(HarnessOptions {
        config: Config::embedded_default().unwrap(),
        store: Store::open(db).unwrap(),
        provider: Provider::DeepSeek,
        api_key: TEST_API_KEY.to_string(),
        workspace: workspace.to_path_buf(),
        headless: false,
    })
    .unwrap()
}

#[test]
fn a_workspace_inside_a_larger_repository_only_reports_its_own_files() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let sub = repo.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    git(&repo, &["init", "-q"]);
    write(&repo, "top.txt", "top\n");
    write(&sub, "inner.txt", "inner\n");
    commit_all(&repo, "seed");
    write(&repo, "top.txt", "top changed\n");
    write(&sub, "inner.txt", "inner changed\n");
    write(&repo, "other.txt", "outside the workspace\n");
    write(&sub, "new.txt", "inside\n");

    let harness = harness_at(&sub, &dir.path().join("harness.db"));
    let files = harness.workspace_changes().unwrap().files;
    assert_eq!(
        summary(&files),
        [
            ("inner.txt", ChangeStatus::Modified, 1, 1),
            ("new.txt", ChangeStatus::Added, 1, 0),
        ]
    );
}

#[tokio::test]
async fn an_embedded_repository_is_skipped_and_unusual_file_names_come_back_exact() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "dir/ünï cøde.txt", "old\n");
    commit_all(&t.workspace, "seed");
    write(&t.workspace, "dir/ünï cøde.txt", "new\n");
    write(&t.workspace, "my file é.txt", "untracked\n");
    // git lists a directory that holds its own repository as one `nested/` entry.
    let nested = t.workspace.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    git(&nested, &["init", "-q"]);
    write(&nested, "inside.txt", "ignored by this repository\n");

    let files = t.harness.workspace_changes().unwrap().files;
    assert_eq!(
        summary(&files),
        [
            ("dir/ünï cøde.txt", ChangeStatus::Modified, 1, 1),
            ("my file é.txt", ChangeStatus::Added, 1, 0),
        ]
    );
}

// ---------------------------------------------------------------- a turn against its checkpoint

fn call(id: &str, name: &str, arguments: Value) -> String {
    sse_tool_calls(&[(id, name, &arguments.to_string())])
}

async fn run_turn_in(t: &TestHarness, session_id: &str, input: &str, replies: Vec<String>) {
    for reply in replies {
        t.provider.push_sse(reply);
    }
    let events = collect(t.harness.start_turn(session_id, input.to_string())).await;
    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(EventKind::TurnDone {
                state: TurnState::Done,
                error: None
            })
        ),
        "{events:?}"
    );
}

#[tokio::test]
async fn turn_changes_exclude_edits_made_before_the_turn_and_list_files_created_in_it() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "notes.txt", "apple\n");
    write(&t.workspace, "other.txt", "one\n");
    commit_all(&t.workspace, "seed");
    // Before the turn: an uncommitted edit, and a file that was already untracked.
    write(&t.workspace, "other.txt", "one pre-turn\n");
    write(&t.workspace, "scratch.txt", "mine\n");

    let session_id = t.harness.create_session().unwrap();
    run_turn_in(
        &t,
        &session_id,
        "change things",
        vec![
            call("c1", "edit", json!({"path": "notes.txt", "old_string": "apple", "new_string": "banana"})),
            call("c2", "edit", json!({"path": "other.txt", "old_string": "one pre-turn", "new_string": "one pre-turn and turn"})),
            call("c3", "write_file", json!({"path": "created.txt", "content": "new\n"})),
            sse_text("done"),
        ],
    )
    .await;

    let turn = t.harness.turn_changes(&session_id).unwrap();
    assert!(turn.checkpoint);
    assert_eq!(
        summary(&turn.files),
        [
            ("notes.txt", ChangeStatus::Modified, 1, 1),
            ("other.txt", ChangeStatus::Modified, 1, 1),
            ("created.txt", ChangeStatus::Added, 1, 0),
        ]
    );
    // The baseline is the checkpoint, so the turn's patch is against the pre-turn edit...
    let other = patch(by_path(&turn.files, "other.txt"));
    assert!(
        other.contains("\n-one pre-turn\n+one pre-turn and turn\n"),
        "{other}"
    );
    assert!(!other.contains("\n-one\n"), "{other}");
    assert!(patch(by_path(&turn.files, "created.txt")).contains("\n+new\n"));

    // ...while the workspace's changes, against HEAD, also show what was there before it.
    let workspace = t.harness.workspace_changes().unwrap();
    let other = patch(by_path(&workspace.files, "other.txt"));
    assert!(
        other.contains("\n-one\n+one pre-turn and turn\n"),
        "{other}"
    );
    assert!(workspace.files.iter().any(|f| f.path == "scratch.txt"));
}

#[tokio::test]
async fn turn_changes_include_files_changed_through_bash() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "notes.txt", "apple\n");
    commit_all(&t.workspace, "seed");

    let session_id = t.harness.create_session().unwrap();
    run_turn_in(
        &t,
        &session_id,
        "use the shell",
        vec![
            call(
                "c1",
                "bash",
                json!({"command": "printf 'made\\n' > made.txt && printf 'more\\n' >> notes.txt"}),
            ),
            sse_text("done"),
        ],
    )
    .await;

    let turn = t.harness.turn_changes(&session_id).unwrap();
    assert!(turn.checkpoint);
    assert_eq!(
        summary(&turn.files),
        [
            ("notes.txt", ChangeStatus::Modified, 1, 0),
            ("made.txt", ChangeStatus::Added, 1, 0),
        ]
    );
}

#[tokio::test]
async fn only_the_sessions_last_turn_counts_and_a_read_only_turn_has_no_checkpoint() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "a.txt", "a\n");
    write(&t.workspace, "b.txt", "b\n");
    commit_all(&t.workspace, "seed");
    let session_id = t.harness.create_session().unwrap();

    // No turns yet, and an unknown session: no checkpoint, no files.
    for id in [session_id.as_str(), "no-such-session"] {
        let turn = t.harness.turn_changes(id).unwrap();
        assert!(!turn.checkpoint && turn.files.is_empty(), "{id}");
    }

    run_turn_in(
        &t,
        &session_id,
        "edit a",
        vec![
            call(
                "c1",
                "edit",
                json!({"path": "a.txt", "old_string": "a", "new_string": "A"}),
            ),
            sse_text("done"),
        ],
    )
    .await;
    assert_eq!(
        summary(&t.harness.turn_changes(&session_id).unwrap().files),
        [("a.txt", ChangeStatus::Modified, 1, 1)]
    );

    // A read-only turn is now the last one: it has no checkpoint, though the first turn had.
    run_turn_in(&t, &session_id, "just answer", vec![sse_text("fine")]).await;
    let turn = t.harness.turn_changes(&session_id).unwrap();
    assert!(!turn.checkpoint);
    assert!(turn.files.is_empty());

    // The next editing turn lists only its own file, though a.txt is still modified.
    run_turn_in(
        &t,
        &session_id,
        "edit b",
        vec![
            call(
                "c2",
                "edit",
                json!({"path": "b.txt", "old_string": "b", "new_string": "B"}),
            ),
            sse_text("done"),
        ],
    )
    .await;
    let turn = t.harness.turn_changes(&session_id).unwrap();
    assert_eq!(
        summary(&turn.files),
        [("b.txt", ChangeStatus::Modified, 1, 1)]
    );
    assert_eq!(t.harness.workspace_changes().unwrap().files.len(), 2);
}

#[tokio::test]
async fn a_turn_outside_git_has_no_checkpoint_and_after_an_undo_nothing_is_left() {
    // Outside git there is no checkpoint, so nothing to compare against.
    let t = TestHarness::open(Provider::DeepSeek).await;
    std::fs::remove_dir_all(t.workspace.join(".git")).unwrap();
    let session_id = t.harness.create_session().unwrap();
    run_turn_in(
        &t,
        &session_id,
        "create a file",
        vec![
            call(
                "c1",
                "write_file",
                json!({"path": "created.txt", "content": "new\n"}),
            ),
            sse_text("done"),
        ],
    )
    .await;
    let turn = t.harness.turn_changes(&session_id).unwrap();
    assert!(!turn.checkpoint && turn.files.is_empty());

    // In git, undoing the turn puts the files back at the checkpoint: its checkpoint still
    // exists, and the turn no longer differs from it.
    let t = TestHarness::open(Provider::DeepSeek).await;
    write(&t.workspace, "notes.txt", "apple\n");
    commit_all(&t.workspace, "seed");
    let session_id = t.harness.create_session().unwrap();
    run_turn_in(
        &t,
        &session_id,
        "edit and create",
        vec![
            call(
                "c1",
                "edit",
                json!({"path": "notes.txt", "old_string": "apple", "new_string": "banana"}),
            ),
            call(
                "c2",
                "write_file",
                json!({"path": "created.txt", "content": "new\n"}),
            ),
            sse_text("done"),
        ],
    )
    .await;
    assert_eq!(t.harness.turn_changes(&session_id).unwrap().files.len(), 2);
    assert_eq!(
        t.harness.undo(&session_id).unwrap().outcome,
        UndoOutcome::Restored
    );
    let turn = t.harness.turn_changes(&session_id).unwrap();
    assert!(turn.checkpoint);
    assert!(turn.files.is_empty(), "{:?}", turn.files);
}
