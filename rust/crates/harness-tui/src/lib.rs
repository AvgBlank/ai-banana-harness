//! The ratatui terminal app: state (`app`), rendering (`ui`) and the terminal-owning event loop
//! (`run`) that both use.

pub mod app;
pub mod theme;
pub mod ui;

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{
    Event as CtEvent, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures_util::StreamExt;
use harness_core::{
    Config, Event as HarnessEvent, EventKind, Harness, HarnessOptions, LAST_PROVIDER_SETTING,
    Provider, Store, TurnHandle, api_key_from_env, default_db_path,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::time::interval;

use crate::app::{App, FileChange, Screen};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Runs the TUI to completion. The terminal is restored on every exit path, including an error.
pub async fn run() -> Result<()> {
    theme::init();
    let mut terminal = TerminalGuard::enter()?;

    let mut store = Some(Store::open(&default_db_path()?).context("opening the database")?);
    let last_provider = store
        .as_ref()
        .and_then(|s| s.get_setting(LAST_PROVIDER_SETTING).ok().flatten())
        .and_then(|value| value.parse().ok())
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
                    if done || matches!(event.kind, EventKind::ToolResponse { .. }) {
                        main.modified_files = workspace_changes(&main.workspace);
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
                            match edit_in_editor(&mut terminal, &main.input) {
                                Ok(text) => main.input = text,
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
            Action::CloseOverlay => {
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
                            main.modified_files = workspace_changes(&main.workspace);
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
        Screen::Main(state) => state.input.push_str(text),
        Screen::Startup(state) if state.needs_workspace && !state.key_missing => {
            state.workspace_input.push_str(text.trim());
        }
        Screen::Startup(_) => {}
    }
}

enum Action {
    None,
    Quit,
    Cancel,
    Send(String),
    NewSession,
    ListSessions,
    ResumeSession(String),
    CloseOverlay,
    ToggleProvider,
    OpenEditor,
    ConfirmStartup {
        provider: Provider,
        workspace: String,
    },
}

fn interpret_key(app: &mut App, key: KeyEvent) -> Action {
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

            match key.code {
                KeyCode::Char('b') if ctrl => state.toggle_sidebar(),
                KeyCode::Char('e') if ctrl => return Action::OpenEditor,
                KeyCode::PageUp => state.scroll_up(),
                KeyCode::PageDown => state.scroll_down(),
                KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                    state.input.push('\n');
                }
                KeyCode::Enter => {
                    let text = state.input.trim().to_string();
                    if text.is_empty() {
                        state.toggle_last_card();
                        return Action::None;
                    }
                    if state.turn_running {
                        return Action::None;
                    }
                    state.input.clear();
                    match text.as_str() {
                        "/quit" => return Action::Quit,
                        "/new" => return Action::NewSession,
                        "/sessions" => return Action::ListSessions,
                        "/provider" => return Action::ToggleProvider,
                        "/help" => state.open_help(),
                        _ => return Action::Send(text),
                    }
                }
                KeyCode::Backspace => {
                    state.input.pop();
                }
                KeyCode::Char('o') if ctrl => state.toggle_last_card(),
                KeyCode::Char(c) if !ctrl => state.input.push(c),
                _ => {}
            }
            Action::None
        }
    }
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

/// The workspace's uncommitted changes against `HEAD`, plus untracked files, for display only.
/// Empty when the workspace isn't a git repository or git isn't installed.
// ponytail: runs git synchronously on the UI thread; move to spawn_blocking if it ever lags.
pub fn workspace_changes(workspace: &Path) -> Vec<FileChange> {
    let mut changes: Vec<FileChange> = git(workspace, &["diff", "--numstat", "HEAD"])
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let added = fields.next()?;
            let removed = fields.next()?;
            Some(FileChange {
                path: fields.next()?.to_string(),
                // Binary files report `-`.
                added: added.parse().unwrap_or(0),
                removed: removed.parse().unwrap_or(0),
            })
        })
        .collect();
    for path in git(workspace, &["ls-files", "--others", "--exclude-standard"]).lines() {
        let added = std::fs::read_to_string(workspace.join(path))
            .map_or(0, |text| text.lines().count() as u64);
        changes.push(FileChange {
            path: path.to_string(),
            added,
            removed: 0,
        });
    }
    changes
}

fn git(workspace: &Path, args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
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
    })?;
    let session_id = harness.create_session()?;
    Ok((harness, session_id))
}

/// Owns the terminal: enters raw mode and the alternate screen, and restores both on drop so the
/// evaluator's shell is left clean whether the run loop exits normally or with an error.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enabling raw mode")?;
        let mut stdout = io::stdout();
        if let Err(e) = execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste
        ) {
            let _ = disable_raw_mode();
            return Err(e).context("entering the alternate screen");
        }
        let terminal = Terminal::new(CrosstermBackend::new(stdout)).context("opening terminal")?;
        Ok(Self { terminal })
    }

    fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame)) -> Result<()> {
        self.terminal.draw(render).context("drawing a frame")?;
        Ok(())
    }

    /// Hands the terminal back to the shell, e.g. while `$EDITOR` runs. Best effort, as in `Drop`.
    fn suspend(&mut self) {
        let _ = execute!(
            self.terminal.backend_mut(),
            crossterm::event::DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }

    /// Undoes [`Self::suspend`] and forces a full redraw of whatever the editor left behind.
    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().context("enabling raw mode")?;
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste
        )
        .context("entering the alternate screen")?;
        self.terminal.clear().context("clearing the terminal")?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.suspend();
    }
}
