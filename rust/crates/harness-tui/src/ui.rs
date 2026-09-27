//! Renders an [`App`] into a [`Frame`]. Pure: it never mutates `app`.

use harness_core::{APP_NAME, McpServerState, Provider};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::{App, MainState, Overlay, Screen, StartupState, ToolCallItem, TranscriptItem};
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
    }
    // Shown even when WORKSPACE came from the environment: that path can still fail to
    // canonicalize (missing, wrong cwd, etc.), and Enter must not silently no-op then.
    if let Some(error) = &state.workspace_error {
        lines.push(Line::from(error.as_str()).style(theme::error()));
    }
    if state.needs_workspace {
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

    if let Some(overlay) = &state.overlay {
        draw_overlay(frame, overlay);
    }
}

fn draw_overlay(frame: &mut Frame, overlay: &Overlay) {
    match overlay {
        Overlay::Sessions { sessions, selected } => draw_sessions(frame, sessions, *selected),
    }
}

fn draw_sessions(frame: &mut Frame, sessions: &[harness_core::Session], selected: usize) {
    let area = centered_rect(frame.area(), 70, 60);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Sessions ")
        .border_style(theme::border_focused())
        .style(theme::base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = if sessions.is_empty() {
        vec![Line::from("No earlier sessions.").style(theme::muted())]
    } else {
        sessions
            .iter()
            .enumerate()
            .map(|(i, session)| {
                let title = session.title.as_deref().unwrap_or("(untitled)");
                let text = format!(
                    "{title}  —  {}  —  {}",
                    session.workspace, session.updated_at
                );
                if i == selected {
                    Line::from(format!("> {text}")).style(theme::accent_bold())
                } else {
                    Line::from(format!("  {text}"))
                }
            })
            .collect()
    };
    lines.push(Line::from(""));
    lines.push(Line::from("↑/↓ choose · Enter resume · Esc cancel").style(theme::muted()));

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
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
            TranscriptItem::Tool(call) => {
                lines.push(tool_card_line(call));
            }
            TranscriptItem::System(text) => {
                lines.push(Line::from(format!("· {text}")).style(theme::muted()));
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
        "Enter send · Ctrl+C cancel/quit · /new /sessions /provider /quit".to_string()
    };
    let style = if state.turn_running {
        theme::accent()
    } else {
        theme::muted()
    };
    let mcp = mcp_status_line(state);
    let [left, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(u16::try_from(mcp.width()).unwrap_or(u16::MAX)),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(text).style(style).alignment(Alignment::Left),
        left,
    );
    frame.render_widget(Paragraph::new(mcp).alignment(Alignment::Right), right);
}

/// `mcp: fs ● other ○` — connected, not started yet, or `✗` failed. Empty with no servers.
fn mcp_status_line(state: &MainState) -> Line<'static> {
    if state.mcp.is_empty() {
        return Line::default();
    }
    let mut spans = vec![Span::styled(" mcp:", theme::muted())];
    for server in &state.mcp {
        let (mark, style) = match server.state {
            McpServerState::Idle => ("○", theme::muted()),
            McpServerState::Connected { .. } => ("●", theme::accent()),
            McpServerState::Failed(_) => ("✗", theme::error()),
        };
        spans.push(Span::styled(format!(" {} {mark}", server.name), style));
    }
    Line::from(spans)
}

/// A collapsed one-line tool card: the call, and its outcome once known. A blocked call gets a
/// red badge instead of an outcome preview.
fn tool_card_line(call: &ToolCallItem) -> Line<'static> {
    let arguments = one_line(&compact_json(&call.arguments), 40);
    let mut spans = vec![
        Span::raw("🔧 "),
        Span::styled(call.name.clone(), theme::accent()),
        Span::raw(format!("({arguments}) ")),
    ];
    match &call.outcome {
        None => spans.push(Span::styled("…", theme::muted())),
        Some(outcome) if outcome.blocked => {
            spans.push(Span::styled("blocked", theme::error()));
        }
        Some(outcome) => {
            spans.push(Span::raw("→ "));
            spans.push(Span::raw(one_line(&outcome.content, 60)));
        }
    }
    Line::from(spans)
}

/// Re-serialises `json` on one line when it parses, otherwise returns it unchanged; either way
/// the result still goes through [`one_line`] to collapse whitespace and truncate.
fn compact_json(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| json.to_string())
}

/// Collapses newlines to spaces and truncates to `max` characters (adding an ellipsis), so a
/// multi-line tool result still renders as one transcript line.
fn one_line(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let truncated: String = flat.chars().take(max).collect();
        format!("{truncated}…")
    }
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
