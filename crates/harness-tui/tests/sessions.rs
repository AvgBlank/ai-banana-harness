#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_core::{Provider, Session};
use harness_tui::app::App;
use harness_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn rendered(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().to_string()
}

fn session(id: &str, title: Option<&str>, workspace: &str) -> Session {
    Session {
        id: id.to_string(),
        title: title.map(str::to_string),
        workspace: workspace.to_string(),
        provider: "deepseek".to_string(),
        model: "deepseek-flash".to_string(),
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-02T00:00:00.000Z".to_string(),
    }
}

#[test]
fn the_sessions_list_renders_title_workspace_and_time_with_the_selection_marker() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.open_sessions(vec![
        session("s1", Some("Fix the flaky test"), "/repo/one"),
        session("s2", None, "/repo/two"),
    ]);

    let view = rendered(&app);
    assert!(view.contains("Sessions"), "{view}");
    assert!(view.contains("Fix the flaky test"), "{view}");
    assert!(view.contains("/repo/one"), "{view}");
    assert!(view.contains("2026-01-02T00:00:00.000Z"), "{view}");
    assert!(view.contains("(untitled)"), "{view}");
    // The first session is selected by default.
    assert!(view.contains("> Fix the flaky test"), "{view}");
}

#[test]
fn new_clears_the_transcript_and_notes_the_fresh_session() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.push_user_message("earlier question".to_string());

    main.start_new_session();

    assert!(main.transcript.iter().all(|item| !matches!(
        item,
        harness_tui::app::TranscriptItem::User(text) if text == "earlier question"
    )));
    assert!(!main.turn_running);
    let view = rendered(&app);
    assert!(view.contains("Started a new session"), "{view}");
    assert!(!view.contains("earlier question"), "{view}");
}

#[test]
fn resuming_a_session_replaces_the_transcript_and_adopts_its_provider() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.push_user_message("earlier question".to_string());

    let mut resumed = session("s1", Some("Older task"), "/repo/one");
    resumed.provider = "qwen".to_string();
    main.resume_session(resumed);

    assert_eq!(main.provider, Provider::Qwen);
    assert!(main.overlay.is_none());
    let view = rendered(&app);
    assert!(view.contains("Resumed session: Older task"), "{view}");
    assert!(!view.contains("earlier question"), "{view}");
}

#[test]
fn provider_toggle_updates_state_and_notes_the_switch() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();

    main.set_provider(Provider::Qwen);

    assert_eq!(main.provider, Provider::Qwen);
    let view = rendered(&app);
    assert!(view.contains("Switched provider to qwen"), "{view}");
}
