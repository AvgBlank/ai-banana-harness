//! The one dark palette every screen draws from, with a banana-yellow accent.
//!
//! Each colour has a truecolor value and a hand-picked nearest xterm-256 entry; [`init`] picks
//! one set once at startup, and every style helper below honours that choice.

use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::style::{Color, Modifier, Style};

pub const BACKGROUND: Color = Color::Rgb(0x12, 0x12, 0x14);
pub const TEXT: Color = Color::Rgb(0xe8, 0xe6, 0xe1);
pub const MUTED: Color = Color::Rgb(0x8a, 0x8a, 0x92);
pub const BORDER: Color = Color::Rgb(0x3a, 0x3a, 0x40);
pub const ACCENT: Color = Color::Rgb(0xff, 0xe1, 0x35);
pub const ERROR: Color = Color::Rgb(0xe0, 0x5a, 0x5a);
pub const SUCCESS: Color = Color::Rgb(0x7e, 0xc0, 0x7e);

/// A palette colour: its truecolor value and the nearest xterm-256 entry.
struct Swatch {
    truecolor: Color,
    fallback: u8,
}

impl Swatch {
    fn get(&self) -> Color {
        if TRUECOLOR.load(Ordering::Relaxed) {
            self.truecolor
        } else {
            Color::Indexed(self.fallback)
        }
    }
}

const BACKGROUND_SWATCH: Swatch = Swatch {
    truecolor: BACKGROUND,
    fallback: 233,
};
const TEXT_SWATCH: Swatch = Swatch {
    truecolor: TEXT,
    fallback: 254,
};
const MUTED_SWATCH: Swatch = Swatch {
    truecolor: MUTED,
    fallback: 245,
};
const BORDER_SWATCH: Swatch = Swatch {
    truecolor: BORDER,
    fallback: 237,
};
const ACCENT_SWATCH: Swatch = Swatch {
    truecolor: ACCENT,
    fallback: 221,
};
const ERROR_SWATCH: Swatch = Swatch {
    truecolor: ERROR,
    fallback: 167,
};
const SUCCESS_SWATCH: Swatch = Swatch {
    truecolor: SUCCESS,
    fallback: 108,
};

/// Truecolor until [`init`] or [`set_truecolor`] says otherwise, so tests see the RGB palette.
static TRUECOLOR: AtomicBool = AtomicBool::new(true);

/// Env var that forces the palette: `truecolor` or `256`.
pub const COLORS_ENV: &str = "BANANA_HARNESS_COLORS";

/// Picks the palette once at startup from the process environment.
pub fn init() {
    set_truecolor(supports_truecolor(|name| std::env::var(name).ok()));
}

/// The [`COLORS_ENV`] override wins; otherwise truecolor only when `COLORTERM` says so, which
/// truecolor terminals do (`truecolor`/`24bit`) and 256-colour ones such as Apple Terminal don't.
// ponytail: env heuristic, not terminfo; the override covers terminals that guess wrong.
pub fn supports_truecolor(env: impl Fn(&str) -> Option<String>) -> bool {
    let truecolor = |value: &str| matches!(value, "truecolor" | "24bit");
    match env(COLORS_ENV) {
        Some(forced) if forced == "256" => false,
        Some(forced) if truecolor(&forced) => true,
        _ => env("COLORTERM").is_some_and(|value| truecolor(&value)),
    }
}

pub fn set_truecolor(enabled: bool) {
    TRUECOLOR.store(enabled, Ordering::Relaxed);
}

pub fn base() -> Style {
    Style::new()
        .fg(TEXT_SWATCH.get())
        .bg(BACKGROUND_SWATCH.get())
}

pub fn border() -> Style {
    Style::new().fg(BORDER_SWATCH.get())
}

pub fn border_focused() -> Style {
    accent()
}

pub fn accent() -> Style {
    Style::new().fg(ACCENT_SWATCH.get())
}

pub fn accent_bold() -> Style {
    accent().add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::new().fg(MUTED_SWATCH.get())
}

pub fn error() -> Style {
    Style::new()
        .fg(ERROR_SWATCH.get())
        .add_modifier(Modifier::BOLD)
}

pub fn success() -> Style {
    Style::new().fg(SUCCESS_SWATCH.get())
}
