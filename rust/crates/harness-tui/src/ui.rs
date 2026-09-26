//! Renders an [`App`] into a [`Frame`]. Pure: it never mutates `app`.

use harness_core::{APP_NAME, Provider};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::{App, MainState, Screen, StartupState, TranscriptItem};
use crate::theme;

pub fn draw(frame: &mut Frame, app: &App) {
    frame.render_widget(Block::new().style(theme::base()), frame.area());
    match &app.screen {
        Screen::Startup(state) => draw_startup(frame, state),
        Screen::Main(state) => draw_main(frame, state),
    }
}

fn draw_startup(frame: &mut Frame, state: &StartupState) {
    let area = centered_rect(frame.area(), 50, if state.key_missing { 30 } else { 50 });
    frame.render_widget(Clear, area);

    if state.key_missing {
        let block = Block::bordered()
            .title(" AI_API_KEY not set ")
            .border_style(theme::error())
            .style(theme::base());
        let text = Text::from(vec![
            Line::from("Export your DeepSeek or Qwen API key and restart:"),
            Line::from(""),
            Line::from("  export AI_API_KEY=..."),
            Line::from(""),
            Line::from("Press q to quit.").style(theme::muted()),
        ]);
        frame.render_widget(
            Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let block = Block::bordered()
        .title(" Choose provider ")
        .border_style(theme::border_focused())
        .style(theme::base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        provider_line(Provider::DeepSeek, state.selected),
        provider_line(Provider::Qwen, state.selected),
    ];
    lines.push(Line::from(""));
    if state.needs_workspace {
        lines.push(Line::from("Workspace:").style(theme::muted()));
        lines.push(Line::from(format!("  {}_", state.workspace_input)));
        if let Some(error) = &state.workspace_error {
            lines.push(Line::from(error.as_str()).style(theme::error()));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from("↑/↓ choose · Enter confirm · q quit").style(theme::muted()));

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn provider_line(provider: Provider, selected: Provider) -> Line<'static> {
    let label = match provider {
        Provider::DeepSeek => "DeepSeek",
        Provider::Qwen => "Qwen",
    };
    if provider == selected {
        Line::from(format!("> {label}")).style(theme::accent_bold())
    } else {
        Line::from(format!("  {label}")).style(theme::base())
    }
}

fn draw_main(frame: &mut Frame, state: &MainState) {
    let area = frame.area();
    let input_rows = state.input.lines().count().clamp(1, 6) as u16 + 2;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(input_rows),
            Constraint::Length(1),
        ])
        .split(area);

    draw_transcript(frame, state, chunks[0]);
    draw_input(frame, state, chunks[1]);
    draw_status(frame, state, chunks[2]);
}

fn draw_transcript<'a>(frame: &mut Frame, state: &'a MainState, area: Rect) {
    let mut lines: Vec<Line<'a>> = Vec::new();
    for item in &state.transcript {
        match item {
            TranscriptItem::User(text) => {
                lines.push(Line::from("You").style(theme::accent_bold()));
                for line in text.lines() {
                    lines.push(Line::from(line.to_string()));
                }
                lines.push(Line::from(""));
            }
            TranscriptItem::Assistant(message) => {
                lines.push(Line::from(APP_NAME).style(theme::accent_bold()));
                // Only this message's text is parsed as markdown; earlier items are untouched.
                lines.extend(tui_markdown::from_str(&message.content).lines);
                if let Some(error) = &message.error {
                    lines.push(Line::from(format!("✗ {error}")).style(theme::error()));
                }
                lines.push(Line::from(""));
            }
        }
    }
    if let Some(error) = &state.last_error {
        lines.push(Line::from(format!("✗ {error}")).style(theme::error()));
    }

    let block = Block::bordered()
        .title(" Transcript ")
        .border_style(theme::border())
        .style(theme::base());
    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, state: &MainState, area: Rect) {
    let block = Block::bordered()
        .title(" Message (Enter sends · Alt+Enter newline) ")
        .border_style(theme::border_focused())
        .style(theme::base());
    frame.render_widget(
        Paragraph::new(state.input.as_str())
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_status(frame: &mut Frame, state: &MainState, area: Rect) {
    let text = if state.turn_running {
        format!(
            "{} running… Ctrl+C to cancel",
            crate::app::SPINNER_FRAMES[state.spinner_frame]
        )
    } else if state.quit_armed {
        "Press Ctrl+C again to quit".to_string()
    } else {
        "Enter send · Alt+Enter newline · Ctrl+C cancel/quit · /quit exit".to_string()
    };
    let style = if state.turn_running {
        theme::accent()
    } else {
        theme::muted()
    };
    frame.render_widget(
        Paragraph::new(text).style(style).alignment(Alignment::Left),
        area,
    );
}

/// A `width%` x `height%` box centred in `area`.
fn centered_rect(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1])[1]
}
