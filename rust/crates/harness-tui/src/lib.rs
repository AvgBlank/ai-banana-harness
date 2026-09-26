//! The ratatui terminal app: state (`app`), rendering (`ui`) and the terminal-owning event loop
//! (`run`) that both use.

pub mod app;
mod theme;
pub mod ui;

use std::io;
use std::path::PathBuf;
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
    Config, Event as HarnessEvent, EventKind, Harness, HarnessOptions, Provider, Store, TurnHandle,
    api_key_from_env, default_db_path,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::time::interval;

use crate::app::{App, Screen};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const LAST_PROVIDER_KEY: &str = "last_provider";

/// Runs the TUI to completion. The terminal is restored on every exit path, including an error.
pub async fn run() -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;

    let mut store = Some(Store::open(&default_db_path()?).context("opening the database")?);
    let last_provider = store
        .as_ref()
        .and_then(|s| s.get_setting(LAST_PROVIDER_KEY).ok().flatten())
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
                    main.apply_event(event);
                }
                if done {
                    turn = None;
                }
                dirty = true;
            }
            ct_event = term_events.next() => {
                let Some(ct_event) = ct_event else { break };
                let ct_event = ct_event.context("reading a terminal event")?;
                if handle_terminal_event(
                    &mut app,
                    &ct_event,
                    &mut store,
                    &env_workspace,
                    &mut harness,
                    &mut session_id,
                    &mut turn,
                )? {
                    break;
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

/// Returns `true` when the loop should exit.
fn handle_terminal_event(
    app: &mut App,
    event: &CtEvent,
    store: &mut Option<Store>,
    env_workspace: &Option<PathBuf>,
    harness: &mut Option<Harness>,
    session_id: &mut String,
    turn: &mut Option<TurnHandle>,
) -> Result<bool> {
    match event {
        CtEvent::Paste(text) => {
            paste_into(app, text);
            Ok(false)
        }
        CtEvent::Key(key) if key.kind == KeyEventKind::Press => match interpret_key(app, *key) {
            Action::None => Ok(false),
            Action::Quit => Ok(true),
            Action::Cancel => {
                if let Some(t) = turn.as_ref() {
                    t.cancel();
                }
                Ok(false)
            }
            Action::Send(text) => {
                if let Some(h) = harness.as_ref() {
                    if let Some(main) = app.main_mut() {
                        main.push_user_message(text.clone());
                    }
                    *turn = Some(h.start_turn(session_id, text));
                }
                Ok(false)
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
                            return Ok(false);
                        };
                        let (opened, sid) = open_harness(taken, provider, path.clone())?;
                        *harness = Some(opened);
                        *session_id = sid;
                        app.enter_main(provider, path);
                    }
                    Err(e) => {
                        if let Some(s) = app.startup_mut() {
                            s.workspace_error =
                                Some(format!("workspace {} not found: {e}", resolved.display()));
                        }
                    }
                }
                Ok(false)
            }
        },
        _ => Ok(false),
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
            match key.code {
                KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                    state.input.push('\n');
                }
                KeyCode::Enter => {
                    let text = state.input.trim().to_string();
                    if text.is_empty() || state.turn_running {
                        return Action::None;
                    }
                    state.input.clear();
                    if text == "/quit" {
                        return Action::Quit;
                    }
                    return Action::Send(text);
                }
                KeyCode::Backspace => {
                    state.input.pop();
                }
                KeyCode::Char(c) if !ctrl => state.input.push(c),
                _ => {}
            }
            Action::None
        }
    }
}

/// Persists the chosen provider, opens the harness against `workspace`, and creates its session.
/// A failure here is treated as fatal: the config is embedded and the key was already confirmed
/// present, so this should not fail in practice.
fn open_harness(store: Store, provider: Provider, workspace: PathBuf) -> Result<(Harness, String)> {
    store.set_setting(LAST_PROVIDER_KEY, provider.as_str())?;
    let config = Config::embedded_default()?;
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
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            self.terminal.backend_mut(),
            crossterm::event::DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}
