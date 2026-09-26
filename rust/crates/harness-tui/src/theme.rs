//! The one dark palette every screen draws from, with a banana-yellow accent.
//!
//! Colours are truecolor `Rgb`; terminals without 24-bit support degrade these to their
//! nearest palette entry on their own, so no separate 256-colour table is kept here.

use ratatui::style::{Color, Modifier, Style};

pub const BACKGROUND: Color = Color::Rgb(0x12, 0x12, 0x14);
pub const TEXT: Color = Color::Rgb(0xe8, 0xe6, 0xe1);
pub const MUTED: Color = Color::Rgb(0x8a, 0x8a, 0x92);
pub const BORDER: Color = Color::Rgb(0x3a, 0x3a, 0x40);
pub const ACCENT: Color = Color::Rgb(0xff, 0xe1, 0x35);
pub const ERROR: Color = Color::Rgb(0xe0, 0x5a, 0x5a);

pub fn base() -> Style {
    Style::new().fg(TEXT).bg(BACKGROUND)
}

pub fn border() -> Style {
    Style::new().fg(BORDER)
}

pub fn border_focused() -> Style {
    Style::new().fg(ACCENT)
}

pub fn accent() -> Style {
    Style::new().fg(ACCENT)
}

pub fn accent_bold() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::new().fg(MUTED)
}

pub fn error() -> Style {
    Style::new().fg(ERROR).add_modifier(Modifier::BOLD)
}
