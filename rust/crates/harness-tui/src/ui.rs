//! Renders an [`App`] into a [`Frame`]. Pure: it never mutates `app`.

use std::time::Duration;

use harness_core::{APP_NAME, Provider, TurnState};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Padding, Paragraph, Wrap};

use crate::app::{
    App, MainState, Overlay, Screen, StartupState, SummaryCard, ToolCallItem, TranscriptItem,
};
use crate::theme;

const SIDEBAR_WIDTH: u16 = 34;

/// The input box shows 1–6 text rows plus its border, so it grows from 3 to 8 rows.
const INPUT_MAX_TEXT_ROWS: usize = 6;

/// Most modified files listed in the sidebar before collapsing the rest into a count.
const SIDEBAR_MAX_FILES: usize = 8;

/// `/help`'s command list; keep in step with `interpret_key` in `lib.rs`.
const HELP_COMMANDS: &[(&str, &str)] = &[
    ("/new", "start a fresh session"),
    ("/sessions", "list and resume earlier sessions"),
    ("/provider", "switch between DeepSeek and Qwen"),
    ("/help", "show this help"),
    ("/quit", "exit"),
];

const HELP_KEYS: &[(&str, &str)] = &[
    ("Enter", "send the message"),
    ("Alt+Enter", "insert a newline"),
    ("Ctrl+C", "cancel the running turn; twice to quit"),
    ("Ctrl+B", "toggle the sidebar"),
    ("Ctrl+E", "edit the input in $EDITOR"),
    ("PgUp / PgDn", "scroll the transcript"),
    ("Esc", "close an overlay"),
];

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
    let [body, status] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);
    let main = if state.sidebar_visible(area.width) {
        let [main, sidebar] =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(SIDEBAR_WIDTH)])
                .areas(body);
        draw_sidebar(frame, state, sidebar);
        main
    } else {
        body
    };

    let input_lines = input_line_count(state, main.width.saturating_sub(2));
    let input_rows = input_lines.clamp(1, INPUT_MAX_TEXT_ROWS) as u16 + 2;
    let [transcript, input] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(input_rows)]).areas(main);

    draw_transcript(frame, state, transcript);
    draw_input(frame, state, input, input_lines);
    draw_status(frame, state, status);

    if let Some(overlay) = &state.overlay {
        draw_overlay(frame, overlay);
    }
}

fn draw_overlay(frame: &mut Frame, overlay: &Overlay) {
    match overlay {
        Overlay::Sessions { sessions, selected } => draw_sessions(frame, sessions, *selected),
        Overlay::Help => draw_help(frame),
    }
}

fn draw_help(frame: &mut Frame) {
    let entry = |(key, description): &(&str, &str)| {
        Line::from(vec![
            Span::styled(format!("  {key:<13}"), theme::accent()),
            Span::raw(description.to_string()),
        ])
    };
    let mut lines = vec![Line::from("Commands").style(theme::accent_bold())];
    lines.extend(HELP_COMMANDS.iter().map(entry));
    lines.push(Line::from(""));
    lines.push(Line::from("Keys").style(theme::accent_bold()));
    lines.extend(HELP_KEYS.iter().map(entry));
    lines.push(Line::from(""));
    lines.push(Line::from("Esc close").style(theme::muted()));

    // Sized to its content (plus border and padding), centred.
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::horizontal([Constraint::Length(64)])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Help ")
        .border_style(theme::border_focused())
        .padding(Padding::uniform(1))
        .style(theme::base());
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_sidebar(frame: &mut Frame, state: &MainState, area: Rect) {
    let block = Block::bordered()
        .title(" Session ")
        .border_style(theme::border())
        .padding(Padding::horizontal(1))
        .style(theme::base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width);

    let provider = state.config.as_ref().map(|c| c.provider(state.provider));
    let mut lines = vec![heading("Workspace")];
    lines.push(Line::from(tail(
        &state.workspace.display().to_string(),
        width,
    )));

    lines.push(Line::from(""));
    lines.push(heading("Model"));
    let model = provider.map_or("—", |p| p.model.as_str());
    lines.push(Line::from(tail(
        &format!("{} · {model}", state.provider),
        width,
    )));

    lines.push(Line::from(""));
    lines.push(heading("Context"));
    lines.extend(context_lines(
        state,
        provider.map(|p| p.context_length),
        width,
    ));

    lines.push(Line::from(""));
    lines.push(heading("Tokens"));
    lines.push(Line::from(format!(
        "in {} · out {}",
        count(state.session_tokens.prompt_tokens),
        count(state.session_tokens.completion_tokens)
    )));

    lines.push(Line::from(""));
    lines.push(heading("Elapsed"));
    lines.push(Line::from(
        state.elapsed().map_or("—".to_string(), duration),
    ));

    lines.push(Line::from(""));
    lines.push(heading(&format!(
        "Modified files ({})",
        state.modified_files.len()
    )));
    if state.modified_files.is_empty() {
        lines.push(Line::from("none").style(theme::muted()));
    }
    for change in state.modified_files.iter().take(SIDEBAR_MAX_FILES) {
        let added = format!(" +{}", change.added);
        let removed = format!(" −{}", change.removed);
        let room = width.saturating_sub(added.chars().count() + removed.chars().count());
        lines.push(Line::from(vec![
            Span::raw(tail(&change.path, room)),
            Span::styled(added, theme::success()),
            Span::styled(removed, theme::error()),
        ]));
    }
    if state.modified_files.len() > SIDEBAR_MAX_FILES {
        let more = state.modified_files.len() - SIDEBAR_MAX_FILES;
        lines.push(Line::from(format!("…and {more} more")).style(theme::muted()));
    }

    // No MCP servers or subagents exist yet; these rows fill in once their events do.
    lines.push(Line::from(""));
    lines.push(heading("MCP"));
    lines.push(Line::from("none configured").style(theme::muted()));
    lines.push(Line::from(""));
    lines.push(heading("Subagents"));
    lines.push(Line::from("none").style(theme::muted()));

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn heading(text: &str) -> Line<'static> {
    Line::from(text.to_string()).style(theme::accent_bold())
}

/// A usage bar and percentage, then used/window counts; just the used count when the window
/// size is unknown.
fn context_lines(state: &MainState, window: Option<u64>, width: usize) -> Vec<Line<'static>> {
    let Some(usage) = &state.last_usage else {
        return vec![Line::from("—").style(theme::muted())];
    };
    let used = usage.total_tokens;
    let Some(window) = window.filter(|w| *w > 0) else {
        return vec![Line::from(format!("{} tokens", count(used)))];
    };
    let fraction = (used as f64 / window as f64).clamp(0.0, 1.0);
    let bar_width = width.saturating_sub(5);
    let filled = (fraction * bar_width as f64).round() as usize;
    vec![
        Line::from(vec![
            Span::styled("█".repeat(filled), theme::accent()),
            Span::styled("░".repeat(bar_width - filled), theme::border()),
            Span::raw(format!(" {:>3.0}%", fraction * 100.0)),
        ]),
        Line::from(format!("{} / {}", count(used), count(window))).style(theme::muted()),
    ]
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
            TranscriptItem::Summary(card) => {
                lines.push(summary_line(card));
                lines.push(Line::from(""));
            }
        }
    }
    if let Some(error) = &state.last_error {
        lines.push(Line::from(format!("✗ {error}")).style(theme::error()));
    }

    let title = if state.scroll_top.is_some() {
        " Transcript (scrolled · PgDn to follow) "
    } else {
        " Transcript "
    };
    let block = Block::bordered()
        .title(title)
        .border_style(theme::border())
        .style(theme::base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Scroll positions are counted in wrapped lines, so pinning to the bottom needs the total.
    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let max_top = paragraph
        .line_count(inner.width)
        .saturating_sub(usize::from(inner.height));
    state.scroll_max.set(max_top);
    let top = state.scroll_top.map_or(max_top, |top| top.min(max_top));
    frame.render_widget(
        paragraph.scroll((u16::try_from(top).unwrap_or(u16::MAX), 0)),
        inner,
    );
}

/// The input's rendered line count at `width`, including the empty line a trailing Alt+Enter
/// starts, which wrapping alone doesn't count.
fn input_line_count(state: &MainState, width: u16) -> usize {
    let wrapped = Paragraph::new(state.input.as_str())
        .wrap(Wrap { trim: false })
        .line_count(width);
    wrapped.max(state.input.split('\n').count())
}

fn draw_input(frame: &mut Frame, state: &MainState, area: Rect, lines: usize) {
    let block = Block::bordered()
        .title(" Message (Enter sends · Alt+Enter newline · Ctrl+E editor) ")
        .border_style(theme::border_focused())
        .style(theme::base());
    // Past the height cap, keep the newest lines (where typing happens) in view.
    let top = lines.saturating_sub(INPUT_MAX_TEXT_ROWS);
    frame.render_widget(
        Paragraph::new(state.input.as_str())
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(top).unwrap_or(u16::MAX), 0)),
        area,
    );
}

fn summary_line(card: &SummaryCard) -> Line<'static> {
    let (label, style) = match card.state {
        TurnState::Done => ("✓ done", theme::success()),
        TurnState::Error => ("✗ error", theme::error()),
        TurnState::Cancelled => ("■ cancelled", theme::muted()),
    };
    let files = match card.files_changed {
        1 => "1 file changed".to_string(),
        n => format!("{n} files changed"),
    };
    Line::from(vec![
        Span::styled("── ", theme::border()),
        Span::styled(label, style.add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(
                " · {files} · {} tokens · {}",
                count(card.tokens),
                duration(card.duration)
            ),
            theme::muted(),
        ),
    ])
}

/// `950`, `12.3k`, `1.0M`.
fn count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1_000.0),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

/// `42s`, `3m 07s`, `1h 05m`.
fn duration(d: Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..60 => format!("{secs}s"),
        60..3_600 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!("{}h {:02}m", secs / 3_600, secs % 3_600 / 60),
    }
}

/// Keeps the end of `text` (the informative part of a path) within `max` characters.
fn tail(text: &str, max: usize) -> String {
    let len = text.chars().count();
    if len <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    let rest: String = text.chars().skip(len - keep).collect();
    format!("…{rest}")
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
        "Enter send · Ctrl+C cancel/quit · Ctrl+B sidebar · PgUp/PgDn scroll · /help".to_string()
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
