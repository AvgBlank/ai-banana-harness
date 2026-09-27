#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_core::{Event, EventKind, ModelOutput, Provider, Usage};
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

fn usage(prompt_tokens: u64, completion_tokens: u64) -> Usage {
    Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
    }
}

#[test]
fn a_compaction_shows_a_notice_and_resets_the_context_usage_bar() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.context_limit = Some(50_000);
    main.push_user_message("keep going".to_string());
    main.apply_event(event(EventKind::ModelMessage {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        message: None,
    }));
    main.apply_event(event(EventKind::ModelMessage {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        message: Some(ModelOutput {
            content: "Working on it.".to_string(),
            finish_reason: "tool_calls".to_string(),
            usage: Some(usage(44_000, 1_000)),
        }),
    }));

    let before = rendered(&app);
    assert!(before.contains("ctx ■■■■■■■■■□ 90%"), "{before}");
    assert!(!before.contains("context compacted"));

    app.main_mut()
        .unwrap()
        .apply_event(event(EventKind::ContextCompacted {
            thread_id: "thread".to_string(),
            usage: usage(5_042, 0),
        }));

    let after = rendered(&app);
    assert!(after.contains("⟳ context compacted"), "{after}");
    assert!(after.contains("ctx ■□□□□□□□□□ 10%"), "{after}");
}
