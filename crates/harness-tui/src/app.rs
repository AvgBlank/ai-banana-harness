//! The TUI's state, fed by key input and by `harness_core::Event`s. Kept independent of the
//! terminal so tests can drive it directly and render it with ratatui's `TestBackend`.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use harness_core::{
    Config, Event, EventKind, McpServerStatus, Provider, Session, TurnState, UndoOutcome, Usage,
};

use crate::input::InputBuffer;

/// The sidebar hides itself below this terminal width, whatever its toggle says.
pub const SIDEBAR_MIN_WIDTH: u16 = 100;

/// Lines moved per PgUp/PgDn.
pub const SCROLL_STEP: usize = 10;

/// Lines moved per mouse-wheel notch.
pub const WHEEL_SCROLL_STEP: usize = 3;

/// Slash commands and their descriptions: the one list `/help`, the suggestions and dispatch use.
pub const COMMANDS: &[(&str, &str)] = &[
    ("/new", "start a fresh session"),
    ("/sessions", "list and resume earlier sessions"),
    ("/provider", "switch between DeepSeek and Qwen"),
    (
        "/undo",
        "restore the workspace to the last turn's checkpoint",
    ),
    ("/help", "show commands and key bindings"),
    ("/quit", "exit"),
];

/// Animation frames for the running-turn spinner.
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct App {
    pub screen: Screen,
}

pub enum Screen {
    Startup(StartupState),
    Main(Box<MainState>),
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
    pub input: InputBuffer,
    /// The input's inner width as of the last draw, for moving the cursor by visual row.
    pub input_width: Cell<u16>,
    suggestion: SuggestionState,
    pub turn_running: bool,
    pub spinner_frame: usize,
    /// Armed by one Ctrl+C while idle; a second one quits. Any other key disarms it.
    pub quit_armed: bool,
    pub last_error: Option<String>,
    /// A modal in front of the transcript, e.g. the `/sessions` picker.
    pub overlay: Option<Overlay>,
    /// Refreshed from `Harness::mcp_status` as turn events arrive.
    pub mcp: Vec<McpServerStatus>,
    /// The context's size as of the last LLM call or compaction.
    pub context_tokens: u64,
    /// The size at which the context gets compacted; the usage bar is hidden until it's known.
    pub context_limit: Option<u64>,
    /// The config the harness runs with; the sidebar reads the model and context window from
    /// it. `None` only in tests that don't set it.
    pub config: Option<Config>,
    /// The Ctrl+B toggle; see [`MainState::sidebar_visible`] for the width rule.
    pub sidebar_open: bool,
    /// The transcript's first visible line, or `None` to follow the newest output.
    pub scroll_top: Option<usize>,
    /// The largest useful `scroll_top`, recorded by each draw so PgUp stops at the real top.
    pub scroll_max: Cell<usize>,
    /// Tokens used by the session so far, summed over every completion.
    pub session_tokens: Usage,
    turn_tokens: u64,
    turn_started: Option<Instant>,
    /// The last finished turn's duration, shown once the turn is no longer running.
    pub last_turn_duration: Option<Duration>,
    /// The workspace's uncommitted changes, refreshed by the run loop as tools run.
    pub modified_files: Vec<FileChange>,
}

/// Suggestion-popup state, valid only for the input revision it was recorded at.
#[derive(Clone, Copy, Default)]
struct SuggestionState {
    revision: u64,
    selected: usize,
    dismissed: bool,
}

/// One changed file in the workspace, with its added and removed line counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: u64,
    pub removed: u64,
}

/// A modal shown over the main screen, closed by Esc. `Approval` and `Question` are required
/// actions: Esc on either cancels the whole turn (`Action::Cancel`) rather than a bare close,
/// because the turn is waiting on this action's reply and must not be left hanging.
pub enum Overlay {
    Sessions {
        sessions: Vec<Session>,
        selected: usize,
    },
    /// `/help`: commands and key bindings.
    Help,
    /// A tool call whose permission resolved to `ask`: `a` allow once, `s` allow for session,
    /// `d` deny.
    Approval {
        action_id: String,
        name: String,
        arguments: String,
        rule: String,
    },
    /// The root-only question tool's question: a digit key picks a suggested option, or free
    /// text plus Enter answers directly.
    Question {
        action_id: String,
        question: String,
        options: Vec<String>,
        free_text: String,
    },
    /// A remote MCP server needs a browser OAuth login.
    McpAuth { server: String, url: String },
}

pub enum TranscriptItem {
    User(String),
    Assistant(AssistantMessage),
    Tool(ToolCallItem),
    /// A status note for a slash command, e.g. a new or resumed session.
    System(String),
    /// A subagent thread, holding its own messages and tool cards.
    Subagent(SubagentCard),
    /// The end-of-run card added when a turn finishes.
    Summary(SummaryCard),
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

pub struct SummaryCard {
    pub state: TurnState,
    pub files_changed: usize,
    pub tokens: u64,
    pub duration: Duration,
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
    /// Edit/write cards expand to a diff, bash cards to the output's tail.
    pub expanded: bool,
}

impl ToolCallItem {
    pub fn expandable(&self) -> bool {
        matches!(self.name.as_str(), "edit" | "write_file" | "bash")
    }
}

pub struct ToolOutcome {
    pub content: String,
    pub blocked: bool,
    pub diff: Option<String>,
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
            screen: Screen::Main(Box::new(MainState::new(provider, workspace))),
        }
    }

    pub fn enter_main(&mut self, provider: Provider, workspace: PathBuf) {
        self.screen = Screen::Main(Box::new(MainState::new(provider, workspace)));
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
            input: InputBuffer::default(),
            input_width: Cell::new(0),
            suggestion: SuggestionState::default(),
            turn_running: false,
            spinner_frame: 0,
            quit_armed: false,
            last_error: None,
            overlay: None,
            mcp: Vec::new(),
            context_tokens: 0,
            context_limit: None,
            config: None,
            sidebar_open: true,
            scroll_top: None,
            scroll_max: Cell::new(0),
            session_tokens: Usage::default(),
            turn_tokens: 0,
            turn_started: None,
            last_turn_duration: None,
            modified_files: Vec::new(),
        }
    }

    pub fn sidebar_visible(&self, width: u16) -> bool {
        self.sidebar_open && width >= SIDEBAR_MIN_WIDTH
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_open = !self.sidebar_open;
    }

    pub fn open_help(&mut self) {
        self.overlay = Some(Overlay::Help);
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let top = self.scroll_top.unwrap_or(self.scroll_max.get());
        self.scroll_top = Some(top.saturating_sub(lines));
    }

    /// Scrolling back past the bottom resumes following new output.
    pub fn scroll_down(&mut self, lines: usize) {
        if let Some(top) = self.scroll_top {
            let next = top + lines;
            self.scroll_top = (next < self.scroll_max.get()).then_some(next);
        }
    }

    /// Commands matching a `/…` input: prefix matches first, then other substring matches.
    /// Empty unless the whole input is one `/word`, no overlay is open and Esc didn't dismiss it.
    pub fn command_suggestions(&self) -> Vec<(&'static str, &'static str)> {
        let text = self.input.text();
        if self.overlay.is_some()
            || self.current_suggestion().dismissed
            || !text.starts_with('/')
            || text.contains(char::is_whitespace)
        {
            return Vec::new();
        }
        let query = text.to_lowercase();
        let (mut matches, rest): (Vec<_>, Vec<_>) = COMMANDS
            .iter()
            .copied()
            .filter(|(name, _)| name.contains(&query[1..]))
            .partition(|(name, _)| name.starts_with(&query));
        matches.extend(rest);
        matches
    }

    /// The highlighted suggestion's index, reset whenever the input changes.
    pub fn selected_suggestion(&self) -> usize {
        let count = self.command_suggestions().len();
        self.current_suggestion()
            .selected
            .min(count.saturating_sub(1))
    }

    /// Moves the highlight by `delta`, wrapping around the list.
    pub fn move_suggestion(&mut self, delta: isize) {
        let count = self.command_suggestions().len();
        if count == 0 {
            return;
        }
        let selected = self.selected_suggestion().cast_signed() + delta;
        self.suggestion = SuggestionState {
            revision: self.input.revision(),
            selected: selected.rem_euclid(count.cast_signed()).cast_unsigned(),
            dismissed: false,
        };
    }

    /// Esc: hides the suggestions until the input changes.
    pub fn dismiss_suggestions(&mut self) {
        self.suggestion = SuggestionState {
            revision: self.input.revision(),
            selected: 0,
            dismissed: true,
        };
    }

    fn current_suggestion(&self) -> SuggestionState {
        if self.suggestion.revision == self.input.revision() {
            self.suggestion
        } else {
            SuggestionState::default()
        }
    }

    /// The running turn's elapsed time, else the last turn's duration.
    pub fn elapsed(&self) -> Option<Duration> {
        match self.turn_started {
            Some(started) if self.turn_running => Some(started.elapsed()),
            _ => self.last_turn_duration,
        }
    }

    /// Clears everything tied to the previous session's conversation.
    fn reset_conversation(&mut self) {
        self.transcript.clear();
        self.input.clear();
        self.turn_running = false;
        self.last_error = None;
        self.context_tokens = 0;
        self.scroll_top = None;
        self.session_tokens = Usage::default();
        self.turn_tokens = 0;
        self.turn_started = None;
        self.last_turn_duration = None;
    }

    /// `/new`: clears the transcript for a freshly created session.
    pub fn start_new_session(&mut self) {
        self.reset_conversation();
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
        self.reset_conversation();
        self.push_system_message(format!("Resumed session: {label}"));
    }

    pub fn set_provider(&mut self, provider: Provider) {
        self.provider = provider;
        self.push_system_message(format!("Switched provider to {provider}"));
    }

    /// `/undo`: confirms whether the last turn's checkpoint was restored.
    pub fn confirm_undo(&mut self, outcome: UndoOutcome) {
        self.push_system_message(match outcome {
            UndoOutcome::Restored => {
                "Restored the workspace to the last turn's checkpoint.".to_string()
            }
            UndoOutcome::NothingToUndo => {
                "Nothing to undo: the last turn made no checkpointed changes.".to_string()
            }
        });
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
        self.scroll_top = None;
        self.turn_tokens = 0;
        self.turn_started = Some(Instant::now());
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
                let duration = self
                    .turn_started
                    .take()
                    .map_or(Duration::ZERO, |started| started.elapsed());
                self.last_turn_duration = Some(duration);
                self.transcript.push(TranscriptItem::Summary(SummaryCard {
                    state,
                    files_changed: self.modified_files.len(),
                    tokens: self.turn_tokens,
                    duration,
                }));
                // Cancelling while a required action is pending resolves it without a reply
                // (every waiter also selects on the cancellation token); nothing would ever
                // close this overlay otherwise.
                if matches!(
                    self.overlay,
                    Some(Overlay::Approval { .. } | Overlay::Question { .. })
                ) {
                    self.overlay = None;
                }
            }
            EventKind::ToolApprovalRequired {
                action_id,
                name,
                arguments,
                rule,
                ..
            } => {
                self.overlay = Some(Overlay::Approval {
                    action_id,
                    name,
                    arguments,
                    rule,
                });
            }
            EventKind::ToolResponseRequired {
                action_id,
                question,
                options,
                ..
            } => {
                self.overlay = Some(Overlay::Question {
                    action_id,
                    question,
                    options,
                    free_text: String::new(),
                });
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
            EventKind::McpAuthRequired { server, url, .. } => {
                self.overlay = Some(Overlay::McpAuth { server, url });
            }
            kind => {
                let thread_id = kind.thread_id().unwrap_or_default().to_string();
                if let EventKind::ModelMessage {
                    message: Some(output),
                    ..
                } = &kind
                    && let Some(usage) = &output.usage
                {
                    self.record_usage(usage);
                }
                if self.subagent_mut(&thread_id).is_none() {
                    match &kind {
                        EventKind::ModelMessage {
                            message: Some(output),
                            ..
                        } => {
                            if let Some(usage) = &output.usage {
                                self.context_tokens = usage.prompt_tokens + usage.completion_tokens;
                            }
                        }
                        EventKind::ContextCompacted { usage, .. } => {
                            self.context_tokens = usage.prompt_tokens + usage.completion_tokens;
                        }
                        _ => {}
                    }
                }
                match self.subagent_mut(&thread_id) {
                    Some(card) => apply_thread_event(&mut card.items, kind),
                    None => apply_thread_event(&mut self.transcript, kind),
                }
            }
        }
    }

    /// Enter (on empty input) or Ctrl+O: expands or collapses the most recent expandable card.
    pub fn toggle_last_card(&mut self) {
        let card = last_expandable_card(&mut self.transcript);
        if let Some(call) = card {
            call.expanded = !call.expanded;
        }
    }

    fn record_usage(&mut self, usage: &Usage) {
        self.session_tokens.prompt_tokens += usage.prompt_tokens;
        self.session_tokens.completion_tokens += usage.completion_tokens;
        self.session_tokens.total_tokens += usage.total_tokens;
        self.turn_tokens += usage.total_tokens;
    }

    fn subagent_mut(&mut self, thread_id: &str) -> Option<&mut SubagentCard> {
        self.transcript.iter_mut().find_map(|item| match item {
            TranscriptItem::Subagent(card) if card.thread_id == thread_id => Some(card),
            _ => None,
        })
    }
}

fn last_expandable_card(items: &mut [TranscriptItem]) -> Option<&mut ToolCallItem> {
    items.iter_mut().rev().find_map(|item| match item {
        TranscriptItem::Tool(call) if call.expandable() => Some(call),
        TranscriptItem::Subagent(card) => last_expandable_card(&mut card.items),
        _ => None,
    })
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
            if let Some(message) = assistant_message_mut(
                items,
                MessageKey {
                    thread_id: &thread_id,
                    message_id: &message_id,
                },
            ) {
                message.content.push_str(&content);
            }
        }
        EventKind::ModelMessage {
            thread_id,
            message_id,
            message: Some(output),
        } => {
            if let Some(message) = assistant_message_mut(
                items,
                MessageKey {
                    thread_id: &thread_id,
                    message_id: &message_id,
                },
            ) {
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
                expanded: false,
            }));
        }
        EventKind::ToolResponse {
            tool_call_id,
            content,
            blocked,
            diff,
            ..
        } => {
            if let Some(call) = tool_call_mut(items, &tool_call_id) {
                call.outcome = Some(ToolOutcome {
                    content,
                    blocked,
                    diff,
                });
            }
        }
        EventKind::ContextCompacted { .. } => {
            items.push(TranscriptItem::System("⟳ context compacted".to_string()));
        }
        EventKind::TurnCreated { .. }
        | EventKind::TurnDone { .. }
        | EventKind::ThreadCreated { .. }
        | EventKind::ThreadDone { .. }
        | EventKind::ToolApprovalRequired { .. }
        | EventKind::ToolResponseRequired { .. }
        | EventKind::McpAuthRequired { .. } => {}
    }
}

struct MessageKey<'a> {
    thread_id: &'a str,
    message_id: &'a str,
}

fn assistant_message_mut<'a>(
    items: &'a mut [TranscriptItem],
    key: MessageKey<'_>,
) -> Option<&'a mut AssistantMessage> {
    items.iter_mut().rev().find_map(|item| match item {
        TranscriptItem::Assistant(message)
            if message.thread_id == key.thread_id && message.message_id == key.message_id =>
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
