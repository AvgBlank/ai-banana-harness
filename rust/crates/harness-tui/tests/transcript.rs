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
fn a_tool_call_renders_as_a_collapsed_one_line_card() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();

    main.apply_event(event(EventKind::ToolCall {
        thread_id: "thread".to_string(),
        tool_call_id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: r#"{"path":"hello.txt"}"#.to_string(),
    }));
    main.apply_event(event(EventKind::ToolResponse {
        thread_id: "thread".to_string(),
        tool_call_id: "call_1".to_string(),
        content: "     1\thello world".to_string(),
        blocked: false,
    }));

    let view = rendered(&app);
    assert!(view.contains("read_file"), "{view}");
    assert!(view.contains("hello.txt"), "{view}");
    assert!(view.contains("hello world"), "{view}");
}

#[test]
fn a_blocked_tool_call_gets_a_red_badge() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();

    main.apply_event(event(EventKind::ToolCall {
        thread_id: "thread".to_string(),
        tool_call_id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: r#"{"path":"../outside.txt"}"#.to_string(),
    }));
    main.apply_event(event(EventKind::ToolResponse {
        thread_id: "thread".to_string(),
        tool_call_id: "call_1".to_string(),
        content: "Blocked by guardrail: workspace confinement".to_string(),
        blocked: true,
    }));

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let view = terminal.backend().to_string();
    assert!(view.contains("blocked"), "{view}");

    // The badge itself (not just the surrounding text) is styled in the theme's error red.
    let buffer = terminal.backend().buffer();
    let badge_is_red = (0..buffer.area.width).any(|x| {
        (0..buffer.area.height).any(|y| {
            let cell = &buffer[(x, y)];
            cell.symbol() == "b" && cell.fg == harness_tui::theme::ERROR
        })
    });
    assert!(badge_is_red, "{view}");
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

fn tool_card(main: &mut harness_tui::app::MainState, name: &str, arguments: &str, content: &str) {
    main.apply_event(event(EventKind::ToolCall {
        thread_id: "thread".to_string(),
        tool_call_id: format!("call_{name}"),
        name: name.to_string(),
        arguments: arguments.to_string(),
    }));
    main.apply_event(event(EventKind::ToolResponse {
        thread_id: "thread".to_string(),
        tool_call_id: format!("call_{name}"),
        content: content.to_string(),
        blocked: false,
    }));
}

#[test]
fn an_expanded_edit_card_renders_the_diff() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    let arguments = serde_json::json!({
        "path": "src/lib.rs",
        "old_string": "fn keep() {}\nfn old_name() {}\n",
        "new_string": "fn keep() {}\nfn new_name() {}\n",
    })
    .to_string();
    tool_card(
        main,
        "edit",
        &arguments,
        "Edited src/lib.rs: replaced 1 occurrence(s)",
    );

    let collapsed = rendered(&app);
    assert!(collapsed.contains("edit"), "{collapsed}");
    assert!(!collapsed.contains("- fn old_name"), "{collapsed}");

    app.main_mut().unwrap().toggle_last_card();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let view = terminal.backend().to_string();
    assert!(view.contains("  fn keep() {}"), "{view}");
    assert!(view.contains("- fn old_name() {}"), "{view}");
    assert!(view.contains("+ fn new_name() {}"), "{view}");

    // Removed and added rows carry the diff tints, and the code is syntax-coloured (not all
    // one foreground colour).
    let buffer = terminal.backend().buffer();
    let row_of = |needle: &str| {
        (0..buffer.area.height)
            .find(|&y| {
                let row: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                row.contains(needle)
            })
            .unwrap()
    };
    let cells = |y: u16| (0..buffer.area.width).map(move |x| &buffer[(x, y)]);
    let removed = row_of("- fn old_name");
    let added = row_of("+ fn new_name");
    assert!(cells(removed).any(|c| c.bg == harness_tui::theme::DIFF_REMOVED_BG));
    assert!(cells(added).any(|c| c.bg == harness_tui::theme::DIFF_ADDED_BG));
    let colours: std::collections::HashSet<_> = cells(added)
        .filter(|c| c.bg == harness_tui::theme::DIFF_ADDED_BG && c.symbol() != " ")
        .map(|c| c.fg)
        .collect();
    assert!(colours.len() > 2, "{colours:?}");

    // Toggling again collapses it.
    app.main_mut().unwrap().toggle_last_card();
    assert!(!rendered(&app).contains("+ fn new_name"));
}

#[test]
fn an_expanded_bash_card_shows_the_last_40_lines_and_the_exit_code() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    let output: String = (1..=45).map(|i| format!("line {i}\n")).collect();
    tool_card(
        main,
        "bash",
        r#"{"command":"cargo test"}"#,
        &format!("exit code: 101\n{output}"),
    );
    main.toggle_last_card();

    // Tall enough to show the whole expanded card.
    let mut terminal = Terminal::new(TestBackend::new(80, 60)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let view = terminal.backend().to_string();
    assert!(view.contains("$ cargo test"), "{view}");
    assert!(view.contains("… 5 earlier lines"), "{view}");
    // Body lines are indented under the card; the collapsed header's preview is not.
    assert!(!view.contains("│    line 5 "), "{view}");
    assert!(view.contains("│    line 6 "), "{view}");
    assert!(view.contains("line 45"), "{view}");
    assert!(view.contains("exit code: 101"), "{view}");
}
