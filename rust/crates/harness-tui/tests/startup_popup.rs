#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_core::Provider;
use harness_tui::app::App;
use harness_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn rendered(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn provider_popup_shows_both_options_with_the_preselected_choice() {
    let app = App::new_startup(Provider::Qwen, false, false);
    let view = rendered(&app);

    assert!(view.contains("Choose provider"), "{view}");
    assert!(view.contains("DeepSeek"), "{view}");
    assert!(view.contains("Qwen"), "{view}");
    // Qwen was the last choice, so it carries the selection marker and DeepSeek doesn't.
    assert!(view.contains("> Qwen"), "{view}");
    assert!(!view.contains("> DeepSeek"), "{view}");
}

#[test]
fn preselection_follows_the_stored_provider() {
    let app = App::new_startup(Provider::DeepSeek, false, false);
    let view = rendered(&app);

    assert!(view.contains("> DeepSeek"), "{view}");
    assert!(!view.contains("> Qwen"), "{view}");
}

#[test]
fn a_missing_key_shows_a_blocking_error_instead_of_the_popup() {
    let app = App::new_startup(Provider::DeepSeek, false, true);
    let view = rendered(&app);

    assert!(view.contains("AI_API_KEY not set"), "{view}");
    assert!(!view.contains("Choose provider"), "{view}");
}

#[test]
fn an_unset_workspace_adds_a_path_field_to_the_popup() {
    let app = App::new_startup(Provider::DeepSeek, true, false);
    let view = rendered(&app);

    assert!(view.contains("Workspace"), "{view}");
}

#[test]
fn a_workspace_error_shows_even_when_the_path_came_from_the_environment() {
    // WORKSPACE was already set (needs_workspace: false), so there's no path field to show
    // the error next to — but a bad path must still be reported, not silently swallowed.
    let mut app = App::new_startup(Provider::DeepSeek, false, false);
    app.startup_mut().unwrap().workspace_error = Some("workspace /nope not found".to_string());
    let view = rendered(&app);

    assert!(view.contains("workspace /nope not found"), "{view}");
}
