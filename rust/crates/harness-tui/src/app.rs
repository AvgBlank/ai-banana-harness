//! The TUI's state, fed by key input and by `harness_core::Event`s. Kept independent of the
//! terminal so tests can drive it directly and render it with ratatui's `TestBackend`.

use std::path::PathBuf;

use harness_core::{Event, EventKind, Provider, Session, TurnState};

/// Animation frames for the running-turn spinner.
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct App {
    pub screen: Screen,
}

pub enum Screen {
    Startup(StartupState),
    Main(MainState),
}

/// The provider + workspace popup shown before a session opens.
pub struct StartupState {
    pub selected: Provider,
    pub needs_workspace: bool,
    pub workspace_input: String,
    /// Blocks the popup entirely: `AI_API_KEY` is unset.
    pub key_missing: bool,
    /// Set when a submitted workspace path was rejected (e.g. it doesn't exist).
    pub workspace_error: Option<String>,
}

pub struct MainState {
    pub provider: Provider,
    pub workspace: PathBuf,
    pub transcript: Vec<TranscriptItem>,
    pub input: String,
    pub turn_running: bool,
    pub spinner_frame: usize,
    /// Armed by one Ctrl+C while idle; a second one quits. Any other key disarms it.
    pub quit_armed: bool,
    pub last_error: Option<String>,
    /// A modal in front of the transcript, e.g. the `/sessions` picker.
    pub overlay: Option<Overlay>,
}

/// A picker shown over the main screen, closed by resuming a choice or by Esc.
pub enum Overlay {
    Sessions {
        sessions: Vec<Session>,
        selected: usize,
    },
}

pub enum TranscriptItem {
    User(String),
    Assistant(AssistantMessage),
    Tool(ToolCallItem),
    /// A status note for a slash command, e.g. a new or resumed session.
    System(String),
    /// A subagent thread, holding its own messages and tool cards.
    Subagent(SubagentCard),
}

pub struct SubagentCard {
    thread_id: String,
    pub parent: String,
    pub title: String,
    pub status: SubagentStatus,
    /// The child thread's own transcript: assistant messages and tool cards.
    pub items: Vec<TranscriptItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubagentStatus {
    Running,
    Done,
    Error(String),
}

pub struct AssistantMessage {
    thread_id: String,
    message_id: String,
    pub content: String,
    pub error: Option<String>,
}

/// One tool call's card: the request, and its outcome once the result arrives.
pub struct ToolCallItem {
    tool_call_id: String,
    pub name: String,
    pub arguments: String,
    pub outcome: Option<ToolOutcome>,
}

pub struct ToolOutcome {
    pub content: String,
    pub blocked: bool,
}

impl App {
    pub fn new_startup(selected: Provider, needs_workspace: bool, key_missing: bool) -> Self {
        Self {
            screen: Screen::Startup(StartupState {
                selected,
                needs_workspace,
                workspace_input: String::new(),
                key_missing,
                workspace_error: None,
            }),
        }
    }

    /// Starts directly on the main screen; used once startup is confirmed, and by tests that
    /// don't care about the popup.
    pub fn new_main(provider: Provider, workspace: PathBuf) -> Self {
        Self {
            screen: Screen::Main(MainState::new(provider, workspace)),
        }
    }

    pub fn enter_main(&mut self, provider: Provider, workspace: PathBuf) {
        self.screen = Screen::Main(MainState::new(provider, workspace));
    }

    pub fn startup_mut(&mut self) -> Option<&mut StartupState> {
        match &mut self.screen {
            Screen::Startup(s) => Some(s),
            Screen::Main(_) => None,
        }
    }

    pub fn main_mut(&mut self) -> Option<&mut MainState> {
        match &mut self.screen {
            Screen::Main(m) => Some(m),
            Screen::Startup(_) => None,
        }
    }
}

impl StartupState {
    pub fn toggle_provider(&mut self) {
        self.selected = match self.selected {
            Provider::DeepSeek => Provider::Qwen,
            Provider::Qwen => Provider::DeepSeek,
        };
    }

    /// `Some` once every required field is filled in; the caller still has to open the
    /// workspace and harness before calling [`App::enter_main`].
    pub fn ready(&self) -> Option<(Provider, String)> {
        if self.key_missing {
            return None;
        }
        if self.needs_workspace {
            let workspace = self.workspace_input.trim();
            if workspace.is_empty() {
                return None;
            }
            return Some((self.selected, workspace.to_string()));
        }
        Some((self.selected, String::new()))
    }
}

impl MainState {
    fn new(provider: Provider, workspace: PathBuf) -> Self {
        Self {
            provider,
            workspace,
            transcript: Vec::new(),
            input: String::new(),
            turn_running: false,
            spinner_frame: 0,
            quit_armed: false,
            last_error: None,
            overlay: None,
        }
    }

    /// `/new`: clears the transcript for a freshly created session.
    pub fn start_new_session(&mut self) {
        self.transcript.clear();
        self.input.clear();
        self.turn_running = false;
        self.last_error = None;
        self.push_system_message("Started a new session.".to_string());
    }

    /// `/sessions`: opens the picker.
    pub fn open_sessions(&mut self, sessions: Vec<Session>) {
        self.overlay = Some(Overlay::Sessions {
            sessions,
            selected: 0,
        });
    }

    pub fn close_overlay(&mut self) {
        self.overlay = None;
    }

    /// Resuming a session shows a fresh transcript; the resumed conversation lives in the
    /// session's thread context, which the next turn picks up automatically.
    pub fn resume_session(&mut self, session: Session) {
        self.overlay = None;
        self.provider = session.provider.parse().unwrap_or(self.provider);
        let label = session.title.as_deref().unwrap_or(&session.id).to_string();
        self.transcript.clear();
        self.input.clear();
        self.turn_running = false;
        self.last_error = None;
        self.push_system_message(format!("Resumed session: {label}"));
    }

    pub fn set_provider(&mut self, provider: Provider) {
        self.provider = provider;
        self.push_system_message(format!("Switched provider to {provider}"));
    }

    fn push_system_message(&mut self, text: String) {
        self.transcript.push(TranscriptItem::System(text));
    }

    pub fn tick_spinner(&mut self) {
        if self.turn_running {
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
        }
    }

    /// Records the text the user just sent and marks a turn as running.
    pub fn push_user_message(&mut self, text: String) {
        self.transcript.push(TranscriptItem::User(text));
        self.turn_running = true;
        self.last_error = None;
    }

    /// The turn's subagent threads in creation order: the subagent tree.
    pub fn subagents(&self) -> impl Iterator<Item = &SubagentCard> {
        self.transcript.iter().filter_map(|item| match item {
            TranscriptItem::Subagent(card) => Some(card),
            _ => None,
        })
    }

    /// Applies one event from a turn's stream. Deltas append to the streaming message in
    /// place; earlier transcript items are never touched. A subagent's events go into its card.
    pub fn apply_event(&mut self, event: Event) {
        match event.kind {
            EventKind::TurnCreated { .. } => {}
            EventKind::TurnDone { state, error } => {
                self.turn_running = false;
                self.spinner_frame = 0;
                if state == TurnState::Error {
                    self.last_error = error;
                }
            }
            EventKind::ThreadCreated {
                thread_id,
                parent,
                title,
            } => {
                self.transcript.push(TranscriptItem::Subagent(SubagentCard {
                    thread_id,
                    parent,
                    title,
                    status: SubagentStatus::Running,
                    items: Vec::new(),
                }));
            }
            EventKind::ThreadDone { thread_id, error } => {
                if let Some(card) = self.subagent_mut(&thread_id) {
                    card.status = error.map_or(SubagentStatus::Done, SubagentStatus::Error);
                }
            }
            kind => {
                let thread_id = kind.thread_id().unwrap_or_default().to_string();
                match self.subagent_mut(&thread_id) {
                    Some(card) => apply_thread_event(&mut card.items, kind),
                    None => apply_thread_event(&mut self.transcript, kind),
                }
            }
        }
    }

    fn subagent_mut(&mut self, thread_id: &str) -> Option<&mut SubagentCard> {
        self.transcript.iter_mut().find_map(|item| match item {
            TranscriptItem::Subagent(card) if card.thread_id == thread_id => Some(card),
            _ => None,
        })
    }
}

/// A thread's own message and tool events, applied to that thread's transcript.
fn apply_thread_event(items: &mut Vec<TranscriptItem>, kind: EventKind) {
    match kind {
        EventKind::ModelMessage {
            thread_id,
            message_id,
            message: None,
        } => {
            items.push(TranscriptItem::Assistant(AssistantMessage {
                thread_id,
                message_id,
                content: String::new(),
                error: None,
            }));
        }
        EventKind::ModelMessageDelta {
            thread_id,
            message_id,
            content,
        } => {
            if let Some(message) = assistant_message_mut(items, &thread_id, &message_id) {
                message.content.push_str(&content);
            }
        }
        EventKind::ModelMessage {
            thread_id,
            message_id,
            message: Some(output),
        } => {
            if let Some(message) = assistant_message_mut(items, &thread_id, &message_id) {
                message.content = output.content;
            }
        }
        EventKind::ToolCall {
            tool_call_id,
            name,
            arguments,
            ..
        } => {
            items.push(TranscriptItem::Tool(ToolCallItem {
                tool_call_id,
                name,
                arguments,
                outcome: None,
            }));
        }
        EventKind::ToolResponse {
            tool_call_id,
            content,
            blocked,
            ..
        } => {
            if let Some(call) = tool_call_mut(items, &tool_call_id) {
                call.outcome = Some(ToolOutcome { content, blocked });
            }
        }
        EventKind::TurnCreated { .. }
        | EventKind::TurnDone { .. }
        | EventKind::ThreadCreated { .. }
        | EventKind::ThreadDone { .. } => {}
    }
}

fn assistant_message_mut<'a>(
    items: &'a mut [TranscriptItem],
    thread_id: &str,
    message_id: &str,
) -> Option<&'a mut AssistantMessage> {
    items.iter_mut().rev().find_map(|item| match item {
        TranscriptItem::Assistant(message)
            if message.thread_id == thread_id && message.message_id == message_id =>
        {
            Some(message)
        }
        _ => None,
    })
}

fn tool_call_mut<'a>(
    items: &'a mut [TranscriptItem],
    tool_call_id: &str,
) -> Option<&'a mut ToolCallItem> {
    items.iter_mut().rev().find_map(|item| match item {
        TranscriptItem::Tool(call) if call.tool_call_id == tool_call_id => Some(call),
        _ => None,
    })
}
