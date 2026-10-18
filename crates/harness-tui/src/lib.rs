//! The ratatui terminal app: state (`app`), rendering (`ui`) and the terminal-owning event loop
//! (`run`) that both use.

pub mod app;
pub mod input;
pub mod theme;
pub mod ui;

use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event as CtEvent, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    KeyboardEnhancementFlags, MouseEvent, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};
use futures_util::StreamExt;
use harness_core::{
    Answer, Config, Event as HarnessEvent, EventKind, FileChange, Harness, HarnessOptions,
    LAST_PROVIDER_SETTING, Provider, Store, TurnHandle, api_key_from_env, default_db_path,
    last_provider,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::time::interval;

use crate::app::{App, MainState, SCROLL_STEP, Screen, WHEEL_SCROLL_STEP};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Runs the TUI to completion. The terminal is restored on every exit path, including an error.
pub async fn run() -> Result<()> {
    theme::init();
    let mut terminal = TerminalGuard::enter()?;

    let mut store = Some(Store::open(&default_db_path()?).context("opening the database")?);
    let last_provider = store
        .as_ref()
        .and_then(last_provider)
        .unwrap_or(Provider::DeepSeek);
    let env_workspace = std::env::var_os("WORKSPACE").map(PathBuf::from);
    let key_missing = api_key_from_env().is_err();

    let mut app = App::new_startup(last_provider, env_workspace.is_none(), key_missing);
    terminal.draw(|frame| ui::draw(frame, &app))?;

    let mut term_events = EventStream::new();
    let mut harness: Option<Harness> = None;
    let mut session_id = String::new();
    let mut turn: Option<TurnHandle> = None;
    let mut ticker = interval(FRAME_INTERVAL);
    let mut dirty = false;

    loop {
        tokio::select! {
            biased;
            _ = ticker.tick() => {
                if dirty {
                    if let Some(main) = app.main_mut() {
                        main.tick_spinner();
                    }
                    terminal.draw(|frame| ui::draw(frame, &app))?;
                    dirty = false;
                }
            }
            event = recv_turn_event(&mut turn) => {
                let Some(event) = event else {
                    turn = None;
                    continue;
                };
                let done = matches!(event.kind, EventKind::TurnDone { .. });
                if let Some(main) = app.main_mut() {
                    // Before `turn.done` is applied, so its summary card counts the final state.
                    if (done || matches!(event.kind, EventKind::ToolResponse { .. }))
                        && let Some(h) = harness.as_ref()
                    {
                        main.modified_files = changed_files(h);
                    }
                    main.apply_event(event);
                    // A turn is what connects MCP servers, so its events are when state changes.
                    if let Some(h) = harness.as_ref() {
                        main.mcp = h.mcp_status();
                    }
                }
                if done {
                    turn = None;
                }
                dirty = true;
            }
            ct_event = term_events.next() => {
                let Some(ct_event) = ct_event else { break };
                let ct_event = ct_event.context("reading a terminal event")?;
                // Mouse capture also reports every motion; only redraw for what changed state.
                if let CtEvent::Mouse(mouse) = ct_event {
                    dirty |= interpret_mouse(&mut app, mouse);
                    continue;
                }
                match handle_terminal_event(
                    &mut app,
                    &ct_event,
                    &mut store,
                    &env_workspace,
                    &mut harness,
                    &mut session_id,
                    &mut turn,
                )? {
                    Flow::Continue => {}
                    Flow::Quit => break,
                    Flow::OpenEditor => {
                        // The stream's background reader would steal the editor's keystrokes,
                        // so it's dropped for the duration and recreated afterwards.
                        drop(term_events);
                        if let Some(main) = app.main_mut() {
                            match edit_in_editor(&mut terminal, main.input.text()) {
                                Ok(text) => main.input.set(text),
                                Err(e) => main.last_error = Some(format!("{e:#}")),
                            }
                        }
                        term_events = EventStream::new();
                    }
                }
                dirty = true;
            }
        }
    }

    Ok(())
}

/// Awaits the next event of the running turn, or never resolves when there isn't one.
async fn recv_turn_event(turn: &mut Option<TurnHandle>) -> Option<HarnessEvent> {
    match turn {
        Some(handle) => handle.events.recv().await,
        None => std::future::pending().await,
    }
}

/// What the run loop does after a terminal event: the parts that need the terminal itself.
enum Flow {
    Continue,
    Quit,
    OpenEditor,
}

fn handle_terminal_event(
    app: &mut App,
    event: &CtEvent,
    store: &mut Option<Store>,
    env_workspace: &Option<PathBuf>,
    harness: &mut Option<Harness>,
    session_id: &mut String,
    turn: &mut Option<TurnHandle>,
) -> Result<Flow> {
    match event {
        CtEvent::Paste(text) => {
            paste_into(app, text);
            Ok(Flow::Continue)
        }
        CtEvent::Key(key) if key.kind == KeyEventKind::Press => match interpret_key(app, *key) {
            Action::None => Ok(Flow::Continue),
            Action::Quit => Ok(Flow::Quit),
            Action::OpenEditor => Ok(Flow::OpenEditor),
            Action::Cancel => {
                if let Some(t) = turn.as_ref() {
                    t.cancel();
                }
                Ok(Flow::Continue)
            }
            Action::Send(text) => {
                if let Some(h) = harness.as_ref() {
                    if let Some(main) = app.main_mut() {
                        main.push_user_message(text.clone());
                    }
                    *turn = Some(h.start_turn(session_id, text));
                }
                Ok(Flow::Continue)
            }
            Action::NewSession => {
                if let Some(h) = harness.as_ref() {
                    *session_id = h.create_session()?;
                    if let Some(main) = app.main_mut() {
                        main.start_new_session();
                    }
                }
                Ok(Flow::Continue)
            }
            Action::ListSessions => {
                if let Some(h) = harness.as_ref() {
                    let sessions = h.sessions()?;
                    if let Some(main) = app.main_mut() {
                        main.open_sessions(sessions);
                    }
                }
                Ok(Flow::Continue)
            }
            Action::ResumeSession(id) => {
                if let Some(h) = harness.as_ref()
                    && let Some(session) = h.session(&id)?
                {
                    *session_id = id;
                    if let Some(main) = app.main_mut() {
                        main.resume_session(session);
                        main.context_limit = Some(h.compaction_threshold(main.provider));
                    }
                }
                Ok(Flow::Continue)
            }
            Action::Undo => {
                if let Some(h) = harness.as_ref() {
                    let outcome = h.undo(session_id)?.outcome;
                    if let Some(main) = app.main_mut() {
                        main.modified_files = changed_files(h);
                        main.confirm_undo(outcome);
                    }
                }
                Ok(Flow::Continue)
            }
            Action::CloseOverlay => {
                if let Some(main) = app.main_mut() {
                    main.close_overlay();
                }
                Ok(Flow::Continue)
            }
            Action::Reply { action_id, answer } => {
                if let Some(t) = turn.as_ref() {
                    t.reply(&action_id, answer);
                }
                if let Some(main) = app.main_mut() {
                    main.close_overlay();
                }
                Ok(Flow::Continue)
            }
            Action::ToggleProvider => {
                if let Some(h) = harness.as_ref()
                    && let Some(main) = app.main_mut()
                {
                    let next = match main.provider {
                        Provider::DeepSeek => Provider::Qwen,
                        Provider::Qwen => Provider::DeepSeek,
                    };
                    h.switch_provider(session_id, next)?;
                    main.set_provider(next);
                    main.context_limit = Some(h.compaction_threshold(next));
                }
                Ok(Flow::Continue)
            }
            Action::ConfirmStartup {
                provider,
                workspace,
            } => {
                let resolved = match env_workspace {
                    Some(path) => path.clone(),
                    None => PathBuf::from(workspace),
                };
                match resolved.canonicalize() {
                    Ok(path) => {
                        let Some(taken) = store.take() else {
                            return Ok(Flow::Continue);
                        };
                        let config = Config::embedded_default()?;
                        let (opened, sid) = open_harness(OpenHarness {
                            store: taken,
                            config: config.clone(),
                            provider,
                            workspace: path.clone(),
                        })?;
                        app.enter_main(provider, path);
                        if let Some(main) = app.main_mut() {
                            main.config = Some(config);
                            main.modified_files = changed_files(&opened);
                            main.mcp = opened.mcp_status();
                            main.context_limit = Some(opened.compaction_threshold(provider));
                        }
                        *harness = Some(opened);
                        *session_id = sid;
                    }
                    Err(e) => {
                        if let Some(s) = app.startup_mut() {
                            s.workspace_error =
                                Some(format!("workspace {} not found: {e}", resolved.display()));
                        }
                    }
                }
                Ok(Flow::Continue)
            }
        },
        _ => Ok(Flow::Continue),
    }
}

fn paste_into(app: &mut App, text: &str) {
    match &mut app.screen {
        Screen::Main(state) => state.input.insert_str(text),
        Screen::Startup(state) if state.needs_workspace && !state.key_missing => {
            state.workspace_input.push_str(text.trim());
        }
        Screen::Startup(_) => {}
    }
}

/// What a key asks the run loop to do beyond changing the app's state.
pub enum Action {
    None,
    Quit,
    Cancel,
    Send(String),
    NewSession,
    ListSessions,
    ResumeSession(String),
    Undo,
    CloseOverlay,
    ToggleProvider,
    OpenEditor,
    ConfirmStartup {
        provider: Provider,
        workspace: String,
    },
    /// Answers a required action (an approval or a question).
    Reply {
        action_id: String,
        answer: Answer,
    },
}

/// The pure key → state seam: applies `key` to `app` and says what the run loop must do.
pub fn interpret_key(app: &mut App, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let ctrl_c = ctrl && key.code == KeyCode::Char('c');

    match &mut app.screen {
        Screen::Startup(state) => {
            if ctrl_c || key.code == KeyCode::Char('q') {
                return Action::Quit;
            }
            if state.key_missing {
                return Action::None;
            }
            match key.code {
                KeyCode::Up | KeyCode::Down => state.toggle_provider(),
                KeyCode::Enter => {
                    if let Some((provider, workspace)) = state.ready() {
                        return Action::ConfirmStartup {
                            provider,
                            workspace,
                        };
                    }
                }
                KeyCode::Backspace if state.needs_workspace => {
                    state.workspace_input.pop();
                }
                KeyCode::Char(c) if state.needs_workspace && !ctrl => {
                    state.workspace_input.push(c);
                }
                _ => {}
            }
            Action::None
        }
        Screen::Main(state) => {
            if ctrl_c {
                return if state.turn_running {
                    Action::Cancel
                } else if state.quit_armed {
                    Action::Quit
                } else {
                    state.quit_armed = true;
                    Action::None
                };
            }
            state.quit_armed = false;

            if let Some(overlay) = &mut state.overlay {
                return interpret_overlay_key(overlay, key.code);
            }
            if let Some(action) = interpret_suggestion_key(state, key) {
                return action;
            }
            interpret_input_key(state, key)
        }
    }
}

/// Keys the slash-command popup takes while it's showing; `None` leaves the key to the input.
fn interpret_suggestion_key(state: &mut MainState, key: KeyEvent) -> Option<Action> {
    let suggestions = state.command_suggestions();
    let selected = suggestions.get(state.selected_suggestion())?.0;
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let plain = !key
        .modifiers
        .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Up | KeyCode::BackTab => state.move_suggestion(-1),
        KeyCode::Char('p') if ctrl => state.move_suggestion(-1),
        KeyCode::Down => state.move_suggestion(1),
        KeyCode::Char('n') if ctrl => state.move_suggestion(1),
        KeyCode::Tab => state.input.set(selected.to_string()),
        KeyCode::Esc => state.dismiss_suggestions(),
        KeyCode::Enter if plain => {
            if state.turn_running {
                return Some(Action::None);
            }
            state.input.clear();
            return Some(run_command(state, selected).unwrap_or(Action::None));
        }
        _ => return None,
    }
    Some(Action::None)
}

/// Dispatches a slash command by name; `None` when `name` isn't one of [`app::COMMANDS`].
fn run_command(state: &mut MainState, name: &str) -> Option<Action> {
    Some(match name {
        "/quit" => Action::Quit,
        "/new" => Action::NewSession,
        "/sessions" => Action::ListSessions,
        "/provider" => Action::ToggleProvider,
        "/undo" => Action::Undo,
        "/help" => {
            state.open_help();
            Action::None
        }
        _ => return None,
    })
}

/// Editing keys for the message input. Covers the encodings terminals actually send for
/// Option/Alt word keys: ESC b/f (Terminal.app, Ghostty), ESC[1;3D/C (iTerm2, xterm), ESC DEL
/// and ^W for Option+Backspace, ^U/^A for Cmd+Backspace/Left, plus kitty-protocol modifiers.
fn interpret_input_key(state: &mut MainState, key: KeyEvent) -> Action {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);
    let cmd = mods.contains(KeyModifiers::SUPER);
    let input = &mut state.input;
    match key.code {
        KeyCode::Char('b') if ctrl => state.toggle_sidebar(),
        KeyCode::Char('e') if ctrl => return Action::OpenEditor,
        KeyCode::Char('o') if ctrl => state.toggle_last_card(),
        KeyCode::Char('a') if ctrl => input.move_line_start(),
        KeyCode::Char('u') if ctrl => input.delete_to_line_start(),
        KeyCode::Char('k') if ctrl => input.delete_to_line_end(),
        KeyCode::Char('w') if ctrl => input.delete_word_left(),
        KeyCode::Char('j') if ctrl => input.insert_char('\n'),
        KeyCode::Char('b') if alt && !ctrl => input.move_word_left(),
        KeyCode::Char('f') if alt && !ctrl => input.move_word_right(),
        KeyCode::Char('d') if alt && !ctrl => input.delete_word_right(),
        KeyCode::Char(c) if !ctrl && !alt && !cmd => input.insert_char(c),
        KeyCode::PageUp => state.scroll_up(SCROLL_STEP),
        KeyCode::PageDown => state.scroll_down(SCROLL_STEP),
        KeyCode::Enter if alt || shift => input.insert_char('\n'),
        KeyCode::Enter => {
            let text = input.text().trim().to_string();
            if text.is_empty() {
                state.toggle_last_card();
                return Action::None;
            }
            if state.turn_running {
                return Action::None;
            }
            input.clear();
            return run_command(state, &text).unwrap_or(Action::Send(text));
        }
        KeyCode::Backspace if alt || ctrl => input.delete_word_left(),
        KeyCode::Backspace if cmd => input.delete_to_line_start(),
        KeyCode::Backspace => input.backspace(),
        KeyCode::Delete if alt => input.delete_word_right(),
        KeyCode::Delete => input.delete(),
        KeyCode::Left if alt || ctrl => input.move_word_left(),
        KeyCode::Left if cmd => input.move_line_start(),
        KeyCode::Left => input.move_left(),
        KeyCode::Right if alt || ctrl => input.move_word_right(),
        KeyCode::Right if cmd => input.move_line_end(),
        KeyCode::Right => input.move_right(),
        KeyCode::Home => input.move_line_start(),
        KeyCode::End => input.move_line_end(),
        KeyCode::Up => input.move_up(state.input_width.get()),
        KeyCode::Down => input.move_down(state.input_width.get()),
        _ => {}
    }
    Action::None
}

/// Mouse input: the wheel scrolls the transcript, or moves the `/sessions` selection. Returns
/// whether anything changed, so motion reports don't trigger redraws.
pub fn interpret_mouse(app: &mut App, mouse: MouseEvent) -> bool {
    let Some(state) = app.main_mut() else {
        return false;
    };
    let up = match mouse.kind {
        MouseEventKind::ScrollUp => true,
        MouseEventKind::ScrollDown => false,
        _ => return false,
    };
    match &mut state.overlay {
        None if up => state.scroll_up(WHEEL_SCROLL_STEP),
        None => state.scroll_down(WHEEL_SCROLL_STEP),
        Some(app::Overlay::Sessions { sessions, selected }) => {
            *selected = if up {
                selected.saturating_sub(1)
            } else {
                (*selected + 1).min(sessions.len().saturating_sub(1))
            };
        }
        Some(_) => return false,
    }
    true
}

/// Key handling while an overlay is open: it captures every key until it closes.
fn interpret_overlay_key(overlay: &mut app::Overlay, code: KeyCode) -> Action {
    match overlay {
        app::Overlay::Sessions { sessions, selected } => match code {
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                if *selected + 1 < sessions.len() {
                    *selected += 1;
                }
                Action::None
            }
            KeyCode::Enter => match sessions.get(*selected) {
                Some(session) => Action::ResumeSession(session.id.clone()),
                None => Action::CloseOverlay,
            },
            KeyCode::Esc => Action::CloseOverlay,
            _ => Action::None,
        },
        app::Overlay::Help => match code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => Action::CloseOverlay,
            _ => Action::None,
        },
        // Esc cancels the whole turn rather than closing the overlay: the turn is waiting on
        // this action, and a bare close would leave it parked forever (see `Overlay`'s doc).
        app::Overlay::Approval { action_id, .. } => match code {
            KeyCode::Char('a') => Action::Reply {
                action_id: action_id.clone(),
                answer: Answer::Allow,
            },
            KeyCode::Char('s') => Action::Reply {
                action_id: action_id.clone(),
                answer: Answer::AllowForSession,
            },
            KeyCode::Char('d') => Action::Reply {
                action_id: action_id.clone(),
                answer: Answer::Deny,
            },
            KeyCode::Esc => Action::Cancel,
            _ => Action::None,
        },
        app::Overlay::Question {
            action_id,
            options,
            free_text,
            ..
        } => match code {
            KeyCode::Char(c @ '1'..='9') => {
                let index = c.to_digit(10).unwrap_or(0) as usize;
                match index.checked_sub(1).and_then(|i| options.get(i)) {
                    Some(option) => Action::Reply {
                        action_id: action_id.clone(),
                        answer: Answer::Text(option.clone()),
                    },
                    None => {
                        free_text.push(c);
                        Action::None
                    }
                }
            }
            KeyCode::Backspace => {
                free_text.pop();
                Action::None
            }
            KeyCode::Char(c) => {
                free_text.push(c);
                Action::None
            }
            KeyCode::Enter if !free_text.trim().is_empty() => Action::Reply {
                action_id: action_id.clone(),
                answer: Answer::Text(free_text.trim().to_string()),
            },
            KeyCode::Esc => Action::Cancel,
            _ => Action::None,
        },
        app::Overlay::McpAuth { .. } => match code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => Action::CloseOverlay,
            _ => Action::None,
        },
    }
}

/// Suspends the TUI, opens `text` in `$VISUAL`/`$EDITOR` (falling back to `vi`), and returns
/// the edited text once the editor exits. The TUI is restored even when the editor fails.
fn edit_in_editor(terminal: &mut TerminalGuard, text: &str) -> Result<String> {
    let path = std::env::temp_dir().join(format!("banana-harness-input-{}.md", std::process::id()));
    std::fs::write(&path, text).context("writing the editor's temp file")?;
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    // `EDITOR` may carry flags, e.g. `code --wait`.
    let mut words = editor.split_whitespace();
    let program = words.next().unwrap_or("vi");

    terminal.suspend();
    let status = Command::new(program).args(words).arg(&path).status();
    terminal.resume()?;

    let result = match status {
        Ok(status) if status.success() => std::fs::read_to_string(&path)
            .map(|edited| edited.trim_end_matches('\n').to_string())
            .context("reading the edited input"),
        Ok(status) => Err(anyhow::anyhow!("{editor} exited with {status}")),
        Err(e) => Err(e).with_context(|| format!("running {editor}")),
    };
    let _ = std::fs::remove_file(&path);
    result
}

/// The sidebar's changed-file list; no git, or an error from it, shows as an empty list.
// ponytail: runs git on the UI thread, and builds each file's patch though the sidebar only
// shows counts; move to spawn_blocking, or a counts-only call, if it ever lags.
fn changed_files(harness: &Harness) -> Vec<FileChange> {
    harness
        .workspace_changes()
        .map(|changes| changes.files)
        .unwrap_or_default()
}

struct OpenHarness {
    store: Store,
    config: Config,
    provider: Provider,
    workspace: PathBuf,
}

/// Persists the chosen provider, opens the harness against `workspace`, and creates its session.
/// A failure here is treated as fatal: the config is embedded and the key was already confirmed
/// present, so this should not fail in practice.
fn open_harness(
    OpenHarness {
        store,
        config,
        provider,
        workspace,
    }: OpenHarness,
) -> Result<(Harness, String)> {
    store.set_setting(LAST_PROVIDER_SETTING, provider.as_str())?;
    let api_key = api_key_from_env()?;
    let harness = Harness::open(HarnessOptions {
        config,
        store,
        provider,
        api_key,
        workspace,
        headless: false,
        container: None,
    })?;
    let session_id = harness.create_session()?;
    Ok((harness, session_id))
}

/// Owns the terminal: enters raw mode and the alternate screen, and restores both on drop so the
/// evaluator's shell is left clean whether the run loop exits normally or with an error.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    /// Whether the kitty keyboard protocol was pushed, so it's popped symmetrically.
    keyboard_enhanced: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enabling raw mode")?;
        // Queried before the event stream exists, which would otherwise swallow the reply.
        let keyboard_enhanced = matches!(supports_keyboard_enhancement(), Ok(true));
        let mut guard = Self {
            terminal: Terminal::new(CrosstermBackend::new(io::stdout()))
                .context("opening terminal")?,
            keyboard_enhanced,
        };
        if let Err(e) = guard.take_over() {
            guard.suspend();
            return Err(e).context("entering the alternate screen");
        }
        Ok(guard)
    }

    fn take_over(&mut self) -> io::Result<()> {
        let out = self.terminal.backend_mut();
        execute!(
            out,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture,
            SetCursorStyle::BlinkingBar
        )?;
        if self.keyboard_enhanced {
            // Makes Alt/Shift/Cmd on Backspace, Enter and arrows arrive unambiguously.
            execute!(
                out,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        Ok(())
    }

    fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame)) -> Result<()> {
        self.terminal.draw(render).context("drawing a frame")?;
        Ok(())
    }

    /// Hands the terminal back to the shell, e.g. while `$EDITOR` runs. Best effort, as in `Drop`.
    fn suspend(&mut self) {
        let out = self.terminal.backend_mut();
        if self.keyboard_enhanced {
            let _ = execute!(out, PopKeyboardEnhancementFlags);
        }
        let _ = execute!(
            out,
            DisableMouseCapture,
            DisableBracketedPaste,
            SetCursorStyle::DefaultUserShape,
            LeaveAlternateScreen,
            Show
        );
        let _ = disable_raw_mode();
    }

    /// Undoes [`Self::suspend`] and forces a full redraw of whatever the editor left behind.
    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().context("enabling raw mode")?;
        self.take_over().context("entering the alternate screen")?;
        self.terminal.clear().context("clearing the terminal")?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.suspend();
    }
}
