#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_core::{Event, EventKind, Provider};
use harness_tui::app::App;
use harness_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn rendered(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().to_string()
}

fn event(kind: EventKind) -> Event {
    Event {
        id: "01arz3ndektsv4rrffq69g5fav".to_string(),
        kind,
    }
}

#[test]
fn the_approval_modal_renders_the_tool_and_rule_with_key_hints() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.apply_event(event(EventKind::ToolApprovalRequired {
        action_id: "a1".to_string(),
        thread_id: "t1".to_string(),
        tool_call_id: "c1".to_string(),
        name: "bash".to_string(),
        arguments: r#"{"command":"rm -rf build"}"#.to_string(),
        rule: r#"bash rule "rm -rf *" requires approval"#.to_string(),
    }));

    let view = rendered(&app);
    assert!(view.contains("Approval required"), "{view}");
    assert!(view.contains("bash"), "{view}");
    assert!(view.contains("rm -rf *"), "{view}");
    assert!(view.contains("allow once"), "{view}");
    assert!(view.contains("allow for session"), "{view}");
    assert!(view.contains("deny"), "{view}");
}

#[test]
fn the_question_modal_renders_the_question_and_its_options() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.apply_event(event(EventKind::ToolResponseRequired {
        action_id: "a2".to_string(),
        thread_id: "t1".to_string(),
        question: "Which file should I edit?".to_string(),
        options: vec!["a.txt".to_string(), "b.txt".to_string()],
    }));

    let view = rendered(&app);
    assert!(view.contains("Question"), "{view}");
    assert!(view.contains("Which file should I edit?"), "{view}");
    assert!(view.contains("a.txt"), "{view}");
    assert!(view.contains("b.txt"), "{view}");
}

#[test]
fn a_turn_done_while_a_required_action_is_still_open_closes_the_overlay() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.apply_event(event(EventKind::ToolApprovalRequired {
        action_id: "a1".to_string(),
        thread_id: "t1".to_string(),
        tool_call_id: "c1".to_string(),
        name: "bash".to_string(),
        arguments: "{}".to_string(),
        rule: "bash rule \"*\" requires approval".to_string(),
    }));
    assert!(main.overlay.is_some());

    main.apply_event(event(EventKind::TurnDone {
        state: harness_core::TurnState::Cancelled,
        error: None,
    }));

    assert!(main.overlay.is_none());
}
