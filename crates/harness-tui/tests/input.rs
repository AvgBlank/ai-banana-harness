#![allow(clippy::unwrap_used, clippy::expect_used)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use harness_core::Provider;
use harness_tui::app::{App, COMMANDS, MainState, Overlay};
use harness_tui::{Action, interpret_key, interpret_mouse, ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn app() -> App {
    App::new_main(Provider::DeepSeek, std::env::temp_dir())
}

fn main(app: &mut App) -> &mut MainState {
    app.main_mut().unwrap()
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Action {
    interpret_key(app, KeyEvent::new(code, modifiers))
}

fn key(app: &mut App, code: KeyCode) -> Action {
    press(app, code, KeyModifiers::NONE)
}

fn alt(app: &mut App, code: KeyCode) {
    press(app, code, KeyModifiers::ALT);
}

fn ctrl(app: &mut App, c: char) {
    press(app, KeyCode::Char(c), KeyModifiers::CONTROL);
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, KeyCode::Char(c));
    }
}

fn text(app: &mut App) -> String {
    main(app).input.text().to_string()
}

fn draw(app: &App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal
}

/// The input box's first text cell: one inside its left border, one below its top border.
fn input_origin(terminal: &Terminal<TestBackend>) -> (u16, u16) {
    let buffer = terminal.backend().buffer();
    let title_row = (0..buffer.area.height)
        .find(|&y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .contains("Message")
        })
        .unwrap();
    (1, title_row + 1)
}

fn assert_cursor(app: &App, width: u16, expected: (u16, u16)) {
    let mut terminal = draw(app, width, 24);
    let (x, y) = input_origin(&terminal);
    terminal
        .backend_mut()
        .assert_cursor_position((x + expected.0, y + expected.1));
}

#[test]
fn characters_insert_at_the_cursor_which_the_terminal_shows() {
    let mut app = app();
    type_text(&mut app, "helo");
    key(&mut app, KeyCode::Left);
    type_text(&mut app, "l");
    assert_eq!(text(&mut app), "hello");
    assert_cursor(&app, 80, (4, 0));

    key(&mut app, KeyCode::Home);
    assert_cursor(&app, 80, (0, 0));
    key(&mut app, KeyCode::End);
    assert_cursor(&app, 80, (5, 0));
}

#[test]
fn the_cursor_counts_display_columns_for_wide_characters_and_tabs() {
    let mut app = app();
    type_text(&mut app, "日本");
    assert_cursor(&app, 80, (4, 0));
    type_text(&mut app, "\tx");
    assert_cursor(&app, 80, (9, 0));
}

#[test]
fn a_wrapped_input_grows_and_keeps_the_cursor_on_its_row() {
    let mut app = app();
    // 80 columns minus the border leaves 78 per row.
    type_text(&mut app, &"word ".repeat(20));
    let terminal = draw(&app, 80, 24);
    let (_, y) = input_origin(&terminal);
    // "word " × 15 fills 75 columns; the 16th word wraps, so the cursor sits on row 1.
    assert_cursor(&app, 80, (25, 1));
    // Two text rows plus borders above the status line: the first text row is at most 20.
    assert!(y <= 20, "the box grew to at least two text rows");

    // A row that's exactly full moves the cursor to the start of a fresh row.
    let mut app = self::app();
    type_text(&mut app, &"x".repeat(78));
    assert_cursor(&app, 80, (0, 1));
}

#[test]
fn up_and_down_move_by_visual_row_keeping_the_column() {
    let mut app = app();
    type_text(&mut app, "first line");
    press(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    type_text(&mut app, "second");
    draw(&app, 80, 24);
    key(&mut app, KeyCode::Up);
    assert_cursor(&app, 80, (6, 0));
    key(&mut app, KeyCode::Up);
    assert_eq!(main(&mut app).input.cursor(), 0);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Down);
    assert_eq!(main(&mut app).input.cursor(), text(&mut app).len());
}

#[test]
fn option_and_ctrl_word_keys_in_every_encoding() {
    for delete_word in [
        |a: &mut App| alt(a, KeyCode::Backspace),
        |a: &mut App| ctrl(a, 'w'),
        |a: &mut App| {
            press(a, KeyCode::Backspace, KeyModifiers::CONTROL);
        },
    ] {
        let mut app = app();
        type_text(&mut app, "foo bar baz");
        delete_word(&mut app);
        assert_eq!(text(&mut app), "foo bar ");
    }

    for word_left in [
        |a: &mut App| alt(a, KeyCode::Char('b')),
        |a: &mut App| alt(a, KeyCode::Left),
        |a: &mut App| {
            press(a, KeyCode::Left, KeyModifiers::CONTROL);
        },
    ] {
        let mut app = app();
        type_text(&mut app, "foo bar baz");
        word_left(&mut app);
        assert_eq!(main(&mut app).input.cursor(), 8);
        word_left(&mut app);
        assert_eq!(main(&mut app).input.cursor(), 4);
        // Alt+b is a command, never a typed "b".
        assert_eq!(text(&mut app), "foo bar baz");
    }

    for word_right in [
        |a: &mut App| alt(a, KeyCode::Char('f')),
        |a: &mut App| alt(a, KeyCode::Right),
        |a: &mut App| {
            press(a, KeyCode::Right, KeyModifiers::CONTROL);
        },
    ] {
        let mut app = app();
        type_text(&mut app, "foo bar baz");
        key(&mut app, KeyCode::Home);
        word_right(&mut app);
        assert_eq!(main(&mut app).input.cursor(), 3);
    }
}

#[test]
fn line_and_forward_deletion_keys() {
    let mut app = app();
    type_text(&mut app, "keep this");
    ctrl(&mut app, 'a');
    alt(&mut app, KeyCode::Char('d'));
    assert_eq!(text(&mut app), " this");
    key(&mut app, KeyCode::Delete);
    assert_eq!(text(&mut app), "this");
    key(&mut app, KeyCode::Right);
    ctrl(&mut app, 'k');
    assert_eq!(text(&mut app), "t");
    type_text(&mut app, "ail");
    ctrl(&mut app, 'u');
    assert_eq!(text(&mut app), "");
}

#[test]
fn shift_alt_enter_and_ctrl_j_insert_newlines_and_enter_sends() {
    let mut app = app();
    type_text(&mut app, "a");
    press(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Enter, KeyModifiers::ALT);
    ctrl(&mut app, 'j');
    type_text(&mut app, "b");
    assert_eq!(text(&mut app), "a\n\n\nb");

    match key(&mut app, KeyCode::Enter) {
        Action::Send(sent) => assert_eq!(sent, "a\n\n\nb"),
        _ => panic!("Enter should send"),
    }
    assert_eq!(text(&mut app), "");
}

#[test]
fn backspace_removes_a_whole_grapheme_and_paste_goes_to_the_cursor() {
    let mut app = app();
    type_text(&mut app, "ok 👍🏽");
    key(&mut app, KeyCode::Backspace);
    assert_eq!(text(&mut app), "ok ");

    key(&mut app, KeyCode::Home);
    main(&mut app).input.insert_str("line one\r\nline two\r");
    assert_eq!(text(&mut app), "line one\nline two\nok ");
}

#[test]
fn the_cursor_leaves_the_input_while_an_overlay_is_open() {
    let cursor = |app: &App| {
        let mut terminal = draw(app, 80, 24);
        ratatui::backend::Backend::get_cursor_position(terminal.backend_mut()).unwrap()
    };
    let mut app = app();
    type_text(&mut app, "abc");
    let in_input = cursor(&app);
    main(&mut app).open_help();
    assert_ne!(cursor(&app), in_input);
}

fn suggestions(app: &mut App) -> Vec<&'static str> {
    main(app)
        .command_suggestions()
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn typing_a_slash_suggests_matching_commands() {
    let mut app = app();
    type_text(&mut app, "/");
    assert_eq!(suggestions(&mut app).len(), COMMANDS.len());
    let view = draw(&app, 100, 30).backend().to_string();
    assert!(view.contains("Tab complete"), "{view}");
    assert!(view.contains("list and resume earlier sessions"), "{view}");

    type_text(&mut app, "SE");
    assert_eq!(suggestions(&mut app), ["/sessions"]);

    let mut app = self::app();
    type_text(&mut app, "/do");
    assert_eq!(suggestions(&mut app), ["/undo"]);

    for no_popup in ["/new foo", "hello /new", "/xyz"] {
        let mut app = self::app();
        type_text(&mut app, no_popup);
        assert!(suggestions(&mut app).is_empty(), "{no_popup}");
    }
}

#[test]
fn tab_completes_the_highlighted_suggestion_and_arrows_wrap() {
    let mut app = app();
    type_text(&mut app, "/");
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Tab);
    assert_eq!(text(&mut app), COMMANDS[2].0);

    let mut app = self::app();
    type_text(&mut app, "/");
    key(&mut app, KeyCode::Up);
    assert_eq!(main(&mut app).selected_suggestion(), COMMANDS.len() - 1);
    // An edit resets the highlight.
    type_text(&mut app, "n");
    assert_eq!(main(&mut app).selected_suggestion(), 0);
}

#[test]
fn enter_runs_the_highlighted_command() {
    let mut app = app();
    type_text(&mut app, "/he");
    key(&mut app, KeyCode::Enter);
    assert!(matches!(main(&mut app).overlay, Some(Overlay::Help)));
    assert_eq!(text(&mut app), "");

    let mut app = self::app();
    type_text(&mut app, "/un");
    assert!(matches!(key(&mut app, KeyCode::Enter), Action::Undo));

    // Nothing runs, and the input is kept, while a turn is running.
    let mut app = self::app();
    main(&mut app).turn_running = true;
    type_text(&mut app, "/un");
    assert!(matches!(key(&mut app, KeyCode::Enter), Action::None));
    assert_eq!(text(&mut app), "/un");
}

#[test]
fn esc_dismisses_suggestions_until_the_next_edit() {
    let mut app = app();
    type_text(&mut app, "/s");
    key(&mut app, KeyCode::Esc);
    assert!(suggestions(&mut app).is_empty());
    type_text(&mut app, "e");
    assert_eq!(suggestions(&mut app), ["/sessions"]);
}

#[test]
fn every_command_dispatches_rather_than_being_sent_to_the_model() {
    for (name, _) in COMMANDS {
        let mut app = app();
        main(&mut app).dismiss_suggestions();
        main(&mut app).input.set((*name).to_string());
        let action = key(&mut app, KeyCode::Enter);
        assert!(!matches!(action, Action::Send(_)), "{name} was sent");
    }
}

#[test]
fn the_popup_and_placeholder_render_in_small_terminals() {
    let mut app = app();
    let view = draw(&app, 80, 24).backend().to_string();
    assert!(view.contains("/ for commands"), "{view}");
    type_text(&mut app, "/");
    draw(&app, 40, 10);
    draw(&app, 20, 6);
}

fn wheel(app: &mut App, kind: MouseEventKind) -> bool {
    interpret_mouse(
        app,
        MouseEvent {
            kind,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        },
    )
}

#[test]
fn the_mouse_wheel_scrolls_the_transcript() {
    let mut app = app();
    for i in 0..60 {
        main(&mut app).push_user_message(format!("message number {i}"));
    }
    draw(&app, 90, 30);
    assert!(wheel(&mut app, MouseEventKind::ScrollUp));
    let top = main(&mut app).scroll_top.unwrap();
    assert_eq!(top, main(&mut app).scroll_max.get() - 3);
    let view = draw(&app, 90, 30).backend().to_string();
    assert!(view.contains("scrolled"), "{view}");

    assert!(wheel(&mut app, MouseEventKind::ScrollDown));
    assert_eq!(main(&mut app).scroll_top, None);

    // Motion reports and wheel events over an overlay change nothing.
    assert!(!wheel(&mut app, MouseEventKind::Moved));
    main(&mut app).open_help();
    assert!(!wheel(&mut app, MouseEventKind::ScrollUp));
    assert_eq!(main(&mut app).scroll_top, None);
}
