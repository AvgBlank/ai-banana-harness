#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use harness_core::{
    ChangeStatus, Config, Event, EventKind, FileChange, ModelOutput, Provider, TurnState, Usage,
};
use harness_tui::app::{App, COMMANDS, MainState, SCROLL_STEP};
use harness_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// A modified file with the given counts; the sidebar shows counts only, so no patch is needed.
fn change(path: &str, added: u64, removed: u64) -> FileChange {
    FileChange {
        path: path.to_string(),
        status: ChangeStatus::Modified,
        added,
        removed,
        patch: None,
        truncated: false,
    }
}

fn rendered(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().to_string()
}

fn event(kind: EventKind) -> Event {
    Event {
        id: "01arz3ndektsv4rrffq69g5fav".to_string(),
        kind,
    }
}

fn main_app() -> App {
    let mut app = App::new_main(Provider::DeepSeek, PathBuf::from("/repo/demo"));
    app.main_mut().unwrap().config = Some(Config::embedded_default().unwrap());
    app
}

/// A one-round turn that used `usage` tokens.
fn run_turn(main: &mut MainState, usage: Usage, state: TurnState) {
    main.push_user_message("fix the bug".to_string());
    main.apply_event(event(EventKind::ModelMessage {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        message: None,
    }));
    main.apply_event(event(EventKind::ModelMessage {
        thread_id: "thread".to_string(),
        message_id: "message".to_string(),
        message: Some(ModelOutput {
            content: "Fixed.".to_string(),
            finish_reason: "stop".to_string(),
            usage: Some(usage),
        }),
    }));
    main.apply_event(event(EventKind::TurnDone { state, error: None }));
}

fn usage(prompt: u64, completion: u64) -> Usage {
    Usage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: prompt + completion,
    }
}

#[test]
fn the_sidebar_renders_at_120_columns() {
    let mut app = main_app();
    let main = app.main_mut().unwrap();
    main.modified_files = vec![change("src/lib.rs", 12, 3)];
    run_turn(main, usage(104_000, 858), TurnState::Done);

    let view = rendered(&app, 120, 40);
    for expected in [
        "Workspace",
        "/repo/demo",
        "deepseek · deepseek-flash",
        "Context",
        "10%",
        "Tokens",
        "in 104.0k · out 858",
        "Elapsed",
        "Modified files (1)",
        "src/lib.rs +12 −3",
        "MCP",
        "Subagents",
    ] {
        assert!(view.contains(expected), "missing {expected:?}:\n{view}");
    }
}

#[test]
fn the_sidebar_hides_itself_at_90_columns() {
    let app = main_app();
    let view = rendered(&app, 90, 40);
    assert!(!view.contains("Modified files"), "{view}");
    assert!(!view.contains("Subagents"), "{view}");
}

#[test]
fn ctrl_b_state_hides_the_sidebar_even_when_wide() {
    let mut app = main_app();
    app.main_mut().unwrap().toggle_sidebar();
    let view = rendered(&app, 120, 40);
    assert!(!view.contains("Subagents"), "{view}");
}

#[test]
fn the_summary_card_renders_after_turn_done() {
    let mut app = main_app();
    let main = app.main_mut().unwrap();
    main.modified_files = vec![change("a.rs", 1, 0), change("b.rs", 2, 2)];
    run_turn(main, usage(1_000, 234), TurnState::Done);

    let view = rendered(&app, 90, 30);
    assert!(view.contains("✓ done"), "{view}");
    assert!(view.contains("2 files changed"), "{view}");
    assert!(view.contains("1.2k tokens"), "{view}");
    assert!(view.contains("0s"), "{view}");
}

#[test]
fn a_cancelled_turn_gets_a_cancelled_summary() {
    let mut app = main_app();
    run_turn(app.main_mut().unwrap(), usage(10, 5), TurnState::Cancelled);
    let view = rendered(&app, 90, 30);
    assert!(
        view.contains("cancelled · 0 files changed · 15 tokens"),
        "{view}"
    );
}

#[test]
fn the_mcp_auth_modal_renders_the_login_url() {
    let mut app = main_app();
    app.main_mut()
        .unwrap()
        .apply_event(event(EventKind::McpAuthRequired {
            thread_id: "thread".to_string(),
            server: "docs".to_string(),
            url: "https://auth.example.com/authorize?client_id=abc".to_string(),
        }));

    let view = rendered(&app, 100, 20);
    assert!(view.contains("docs"), "{view}");
    assert!(
        view.contains("https://auth.example.com/authorize?client_id=abc"),
        "{view}"
    );
}

#[test]
fn the_help_overlay_lists_commands_and_keys() {
    let mut app = main_app();
    app.main_mut().unwrap().open_help();

    // The overlay must fit a standard 80x24 terminal.
    let view = rendered(&app, 80, 24);
    let commands = COMMANDS.iter().map(|(name, _)| *name);
    for expected in [
        "Help",
        "Shift/Alt+Enter",
        "Alt+← →",
        "Alt+⌫ / Ctrl+W",
        "Tab / ↑ ↓ / Esc",
        "Ctrl+C",
        "Ctrl+E",
        "Wheel / PgUp PgDn",
        "Esc",
    ]
    .into_iter()
    .chain(commands)
    {
        assert!(view.contains(expected), "missing {expected:?}:\n{view}");
    }
}

#[test]
fn the_transcript_follows_new_output_and_pgup_scrolls_back() {
    let mut app = main_app();
    let main = app.main_mut().unwrap();
    for i in 0..60 {
        main.push_user_message(format!("message number {i}"));
    }

    let view = rendered(&app, 90, 30);
    assert!(view.contains("message number 59"), "{view}");
    assert!(!view.contains("message number 10 "), "{view}");

    let main = app.main_mut().unwrap();
    for _ in 0..20 {
        main.scroll_up(SCROLL_STEP);
    }
    let view = rendered(&app, 90, 30);
    assert!(view.contains("message number 0"), "{view}");
    assert!(!view.contains("message number 59"), "{view}");

    // Paging back down past the bottom resumes following.
    let main = app.main_mut().unwrap();
    for _ in 0..40 {
        main.scroll_down(SCROLL_STEP);
    }
    assert_eq!(main.scroll_top, None);
    let view = rendered(&app, 90, 30);
    assert!(view.contains("message number 59"), "{view}");
}

#[test]
fn the_input_grows_from_three_to_eight_rows() {
    let rows = |input: &str| {
        let mut app = main_app();
        app.main_mut().unwrap().input.set(input.to_string());
        let mut terminal = Terminal::new(TestBackend::new(90, 30)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        // The input box's top border is the row whose title starts with "Message".
        let title_row = (0..buffer.area.height)
            .find(|&y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .contains("Message")
            })
            .unwrap();
        // Everything from there down to the status line (the last row) is the input box.
        buffer.area.height - 1 - title_row
    };

    assert_eq!(rows(""), 3);
    assert_eq!(rows("one\ntwo\nthree"), 5);
    assert_eq!(rows("one\n"), 4);
    assert_eq!(rows(&"line\n".repeat(20)), 8);
}
