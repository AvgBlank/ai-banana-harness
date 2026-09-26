#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_core::{Event, EventKind, ModelOutput, Provider, TurnState};
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
fn a_turns_events_render_the_user_message_and_the_assembled_reply() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();

    main.push_user_message("what is 2 + 2?".to_string());

    main.apply_event(event(EventKind::TurnCreated {
        session_id: "session".to_string(),
        turn_id: "turn".to_string(),
    }));
    main.apply_event(event(EventKind::ModelMessage {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        message: None,
    }));
    main.apply_event(event(EventKind::ModelMessageDelta {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        content: "The answer ".to_string(),
    }));
    main.apply_event(event(EventKind::ModelMessageDelta {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        content: "is **4**.".to_string(),
    }));
    main.apply_event(event(EventKind::ModelMessage {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        message: Some(ModelOutput {
            content: "The answer is **4**.".to_string(),
            finish_reason: "stop".to_string(),
            usage: None,
        }),
    }));
    main.apply_event(event(EventKind::TurnDone {
        state: TurnState::Done,
        error: None,
    }));

    assert!(!main.turn_running);

    let view = rendered(&app);
    assert!(view.contains("what is 2 + 2?"), "{view}");
    assert!(view.contains("The answer"), "{view}");
    assert!(view.contains("4"), "{view}");
}

#[test]
fn the_status_line_shows_a_spinner_while_a_turn_runs() {
    let mut app = App::new_main(Provider::Qwen, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.push_user_message("hi".to_string());
    assert!(main.turn_running);

    let view = rendered(&app);
    assert!(view.contains("Ctrl+C to cancel"), "{view}");
}
