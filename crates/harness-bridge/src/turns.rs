//! A workspace's one running turn: its events become frames, its required actions are tracked.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use harness_core::{Answer, Event, EventKind, Harness, TurnHandle};
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify, mpsc};
use tokio::task::JoinSet;

use crate::frame::{self, BridgeError, Line};

struct ActiveTurn {
    session_id: String,
    /// Known from `turn.created`.
    turn_id: Option<String>,
    /// Required actions waiting for an answer.
    pending: Vec<String>,
    handle: Arc<TurnHandle>,
}

#[derive(Default)]
enum Busy {
    #[default]
    Idle,
    /// An undo is restoring files for this session; no turn may start meanwhile.
    Undoing(String),
    Turn(ActiveTurn),
}

/// The tasks forwarding turn events; shutdown waits for them so every `turn.done` is written.
pub(crate) type Forwarders = StdMutex<JoinSet<()>>;

#[derive(Default)]
pub(crate) struct Turns {
    // Held while the closing `turn.done` frame is queued, so a client that has seen it finds
    // the workspace idle and its next `turn.start` cannot overtake it.
    busy: Mutex<Busy>,
    idle: Notify,
}

impl Busy {
    fn session_id(&self) -> Option<&str> {
        match self {
            Busy::Idle => None,
            Busy::Undoing(id) => Some(id),
            Busy::Turn(turn) => Some(&turn.session_id),
        }
    }
}

impl Turns {
    /// Starts a turn and a task that forwards its events as frames.
    pub(crate) async fn start(
        self: &Arc<Self>,
        harness: &Harness,
        workspace: &str,
        session_id: &str,
        input: String,
        tx: mpsc::Sender<Line>,
        forwarders: &Forwarders,
    ) -> Result<(), BridgeError> {
        let mut busy = self.busy.lock().await;
        if let Some(running) = busy.session_id() {
            return Err(BridgeError::TurnInProgress {
                session_id: running.to_string(),
            });
        }
        let mut handle = harness.start_turn(session_id, input);
        // The forwarder owns the stream; the handle stays shared for cancel and reply.
        let events = std::mem::replace(&mut handle.events, mpsc::channel(1).1);
        let handle = Arc::new(handle);
        *busy = Busy::Turn(ActiveTurn {
            session_id: session_id.to_string(),
            turn_id: None,
            pending: Vec::new(),
            handle: Arc::clone(&handle),
        });
        drop(busy);
        let forward = Forward {
            turns: Arc::clone(self),
            workspace: workspace.to_string(),
            session_id: session_id.to_string(),
            handle,
            tx,
        };
        if let Ok(mut set) = forwarders.lock() {
            while set.try_join_next().is_some() {}
            set.spawn(forward.run(events));
        }
        Ok(())
    }

    /// Cancels the running turn when it belongs to `session_id`.
    pub(crate) async fn cancel(&self, session_id: &str) -> bool {
        let mut busy = self.busy.lock().await;
        match &mut *busy {
            Busy::Turn(turn) if turn.session_id == session_id => {
                // Core resolves every waiting action as denied, so none stays pending.
                turn.pending.clear();
                turn.handle.cancel();
                true
            }
            _ => false,
        }
    }

    /// Cancels whatever turn is running, for a closing workspace or shutdown.
    pub(crate) async fn cancel_any(&self) {
        if let Busy::Turn(turn) = &mut *self.busy.lock().await {
            turn.pending.clear();
            turn.handle.cancel();
        }
    }

    pub(crate) async fn reply(
        &self,
        session_id: &str,
        action_id: &str,
        answer: Answer,
    ) -> Result<(), BridgeError> {
        let mut busy = self.busy.lock().await;
        let Busy::Turn(turn) = &mut *busy else {
            return Err(BridgeError::NoActiveTurn);
        };
        if turn.session_id != session_id {
            return Err(BridgeError::NoActiveTurn);
        }
        let Some(position) = turn.pending.iter().position(|id| id == action_id) else {
            return Err(BridgeError::UnknownAction(action_id.to_string()));
        };
        turn.pending.remove(position);
        turn.handle.reply(action_id, answer);
        Ok(())
    }

    /// `{session_id, turn_id, pending_action_ids}` for a running turn, else `null`.
    pub(crate) async fn summary(&self) -> Value {
        match &*self.busy.lock().await {
            Busy::Turn(turn) => json!({
                "session_id": turn.session_id,
                "turn_id": turn.turn_id,
                "pending_action_ids": turn.pending,
            }),
            _ => Value::Null,
        }
    }

    /// Reserves the workspace for an undo, refused while a turn (or another undo) runs.
    pub(crate) async fn begin_undo(&self, session_id: &str) -> Result<(), BridgeError> {
        let mut busy = self.busy.lock().await;
        if let Some(running) = busy.session_id() {
            return Err(BridgeError::TurnInProgress {
                session_id: running.to_string(),
            });
        }
        *busy = Busy::Undoing(session_id.to_string());
        Ok(())
    }

    pub(crate) async fn end_undo(&self) {
        let mut busy = self.busy.lock().await;
        if matches!(*busy, Busy::Undoing(_)) {
            *busy = Busy::Idle;
        }
        drop(busy);
        self.idle.notify_waiters();
    }

    /// Waits until no turn runs, up to `limit`; returns whether the workspace went idle.
    pub(crate) async fn wait_idle(&self, limit: Duration) -> bool {
        let wait = async {
            loop {
                let notified = self.idle.notified();
                tokio::pin!(notified);
                // Registered before the check, so an idle between the two is not missed.
                notified.as_mut().enable();
                if matches!(*self.busy.lock().await, Busy::Idle) {
                    return;
                }
                notified.await;
            }
        };
        tokio::time::timeout(limit, wait).await.is_ok()
    }
}

struct Forward {
    turns: Arc<Turns>,
    workspace: String,
    session_id: String,
    handle: Arc<TurnHandle>,
    tx: mpsc::Sender<Line>,
}

impl Forward {
    async fn run(self, mut events: mpsc::Receiver<Event>) {
        let mut turn_id: Option<String> = None;
        let mut client_gone = false;
        while let Some(event) = events.recv().await {
            self.note(&event, &mut turn_id).await;
            let done = matches!(event.kind, EventKind::TurnDone { .. });
            if done {
                // Idle only after the closing frame is queued, under the same lock.
                let mut busy = self.turns.busy.lock().await;
                self.send(&event, turn_id.as_deref(), &mut client_gone)
                    .await;
                *busy = Busy::Idle;
                drop(busy);
                self.turns.idle.notify_waiters();
                return;
            }
            self.send(&event, turn_id.as_deref(), &mut client_gone)
                .await;
        }
        // The stream ended without `turn.done`; do not leave the workspace marked busy.
        *self.turns.busy.lock().await = Busy::Idle;
        self.turns.idle.notify_waiters();
    }

    /// Records what the event changes before its frame is queued, so a client that has seen
    /// an approval request can always answer it.
    async fn note(&self, event: &Event, turn_id: &mut Option<String>) {
        let mut busy = self.turns.busy.lock().await;
        let Busy::Turn(active) = &mut *busy else {
            return;
        };
        match &event.kind {
            EventKind::TurnCreated { turn_id: id, .. } => {
                *turn_id = Some(id.clone());
                active.turn_id = Some(id.clone());
            }
            EventKind::ToolApprovalRequired { action_id, .. }
            | EventKind::ToolResponseRequired { action_id, .. } => {
                active.pending.push(action_id.clone());
            }
            EventKind::TurnDone { .. } => active.pending.clear(),
            _ => {}
        }
    }

    async fn send(&self, event: &Event, turn_id: Option<&str>, client_gone: &mut bool) {
        if *client_gone {
            return;
        }
        let line = frame::event(&self.workspace, &self.session_id, turn_id, event);
        if self.tx.send(line).await.is_err() {
            // Nobody reads the frames any more: stop the turn instead of running it unseen.
            *client_gone = true;
            self.handle.cancel();
        }
    }
}
