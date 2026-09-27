//! Renders an [`App`] into a [`Frame`]. Pure: it never mutates `app`.

use std::path::Path;
use std::sync::LazyLock;

use harness_core::{APP_NAME, McpServerState, Provider};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

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
                if call.expanded {
                    lines.extend(expanded_card_lines(call));
                }
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
        "Enter send · Ctrl+O expand · Ctrl+C cancel/quit · /new /sessions /provider /quit"
            .to_string()
    };
    let style = if state.turn_running {
        theme::accent()
    } else {
        theme::muted()
    };
    let mcp = mcp_status_line(state);
    // The context bar only takes space once a threshold is known (it needs a completed LLM
    // call or a config-provided context length; see `Harness::compaction_threshold`).
    let context_width = if state.context_limit.is_some() { 20 } else { 0 };
    let [left, middle, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(context_width),
        Constraint::Length(u16::try_from(mcp.width()).unwrap_or(u16::MAX)),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(text).style(style).alignment(Alignment::Left),
        left,
    );
    if let Some(limit) = state.context_limit {
        frame.render_widget(
            Paragraph::new(context_bar(state.context_tokens, limit))
                .style(theme::muted())
                .alignment(Alignment::Right),
            middle,
        );
    }
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

/// `ctx ■■■□□□□□□□ 34%`, where 100% is the compaction threshold.
fn context_bar(tokens: u64, limit: u64) -> String {
    let percent = (tokens * 100).checked_div(limit).unwrap_or(0).min(100);
    let filled = (percent / 10) as usize;
    format!(
        "ctx {}{} {percent}%",
        "■".repeat(filled),
        "□".repeat(10 - filled)
    )
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

/// Lines of bash output an expanded card shows.
const BASH_TAIL_LINES: usize = 40;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static SYNTAX_THEME: LazyLock<Option<Theme>> =
    LazyLock::new(|| ThemeSet::load_defaults().themes.remove("base16-ocean.dark"));

/// The body under an expanded card: a diff for edit/write, the output's tail for bash.
fn expanded_card_lines(call: &ToolCallItem) -> Vec<Line<'static>> {
    let arguments: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    let text = |key| arguments.get(key).and_then(Value::as_str).unwrap_or("");
    match call.name.as_str() {
        "edit" => diff_lines(text("path"), text("old_string"), text("new_string")),
        // The prior content isn't known here, so a write shows as all additions.
        "write_file" => diff_lines(text("path"), "", text("content")),
        "bash" => bash_lines(text("command"), call),
        _ => Vec::new(),
    }
}

/// A line diff, each line syntax-highlighted for `path`'s language on a red/green row tint.
// ponytail: re-highlighted on every redraw; cache per card if big diffs make frames slow.
fn diff_lines(path: &str, old: &str, new: &str) -> Vec<Line<'static>> {
    let syntax = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| SYNTAXES.find_syntax_by_extension(ext))
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());
    let mut highlighter = SYNTAX_THEME
        .as_ref()
        .map(|theme| HighlightLines::new(syntax, theme));
    TextDiff::from_lines(old, new)
        .iter_all_changes()
        .map(|change| {
            let (sign, row) = match change.tag() {
                ChangeTag::Delete => ("- ", Style::new().bg(theme::DIFF_REMOVED_BG)),
                ChangeTag::Insert => ("+ ", Style::new().bg(theme::DIFF_ADDED_BG)),
                ChangeTag::Equal => ("  ", Style::new()),
            };
            let sign_style = match change.tag() {
                ChangeTag::Delete => theme::error(),
                ChangeTag::Insert => theme::success(),
                ChangeTag::Equal => theme::muted(),
            };
            let value = change.value();
            let mut spans = vec![Span::raw("    "), Span::styled(sign, sign_style.patch(row))];
            let highlighted = highlighter
                .as_mut()
                .and_then(|h| h.highlight_line(value, &SYNTAXES).ok());
            match highlighted {
                Some(ranges) => spans.extend(ranges.into_iter().map(|(style, piece)| {
                    let fg = style.foreground;
                    Span::styled(
                        piece.trim_end_matches('\n').to_string(),
                        row.fg(Color::Rgb(fg.r, fg.g, fg.b)),
                    )
                })),
                None => spans.push(Span::styled(value.trim_end_matches('\n').to_string(), row)),
            }
            Line::from(spans)
        })
        .collect()
}

/// The command, the last lines of its output, and its exit status (the result's first line).
fn bash_lines(command: &str, call: &ToolCallItem) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!("    $ {command}")).style(theme::muted())];
    let Some(outcome) = &call.outcome else {
        return lines;
    };
    let (status, output) = outcome
        .content
        .split_once('\n')
        .unwrap_or((outcome.content.as_str(), ""));
    let output: Vec<&str> = output.lines().collect();
    let skip = output.len().saturating_sub(BASH_TAIL_LINES);
    if skip > 0 {
        lines.push(Line::from(format!("    … {skip} earlier lines")).style(theme::muted()));
    }
    lines.extend(
        output[skip..]
            .iter()
            .map(|l| Line::from(format!("    {l}"))),
    );
    let status_style = if status == "exit code: 0" {
        theme::success()
    } else {
        theme::error()
    };
    lines.push(Line::from(format!("    {status}")).style(status_style));
    lines
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
