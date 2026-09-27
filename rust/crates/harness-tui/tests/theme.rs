#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The palette switch is process-global, so this file (its own test process) is the only one
//! that flips it.

use std::collections::HashMap;

use harness_core::Provider;
use harness_tui::app::App;
use harness_tui::{theme, ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

#[test]
fn the_palette_follows_colorterm_unless_overridden() {
    let env = |pairs: &[(&str, &str)]| {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| map.get(name).cloned()
    };
    assert!(theme::supports_truecolor(env(&[(
        "COLORTERM",
        "truecolor"
    )])));
    assert!(theme::supports_truecolor(env(&[("COLORTERM", "24bit")])));
    assert!(!theme::supports_truecolor(env(&[])));
    assert!(!theme::supports_truecolor(env(&[
        ("COLORTERM", "truecolor"),
        (theme::COLORS_ENV, "256"),
    ])));
    assert!(theme::supports_truecolor(env(&[(
        theme::COLORS_ENV,
        "truecolor"
    )])));
}

#[test]
fn a_256_colour_terminal_gets_only_indexed_colours() {
    theme::set_truecolor(false);
    let app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    theme::set_truecolor(true);

    let buffer = terminal.backend().buffer();
    let colours: Vec<Color> = buffer.content().iter().flat_map(|c| [c.fg, c.bg]).collect();
    assert!(
        colours
            .iter()
            .all(|c| matches!(c, Color::Indexed(_) | Color::Reset)),
        "{colours:?}"
    );
    // The banana accent maps to its nearest xterm-256 yellow.
    assert!(colours.contains(&Color::Indexed(221)));
}
