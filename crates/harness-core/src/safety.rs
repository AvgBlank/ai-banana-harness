//! The turn's safety net: a shared LLM-call/wall-clock budget (checked by every thread of the
//! turn, root and subagents alike) and per-thread doom-loop detection for identical tool calls.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::compaction::{INTERNAL_END, INTERNAL_START};
use crate::config::AgentConfig;

/// Three identical consecutive tool calls warn the model; a fifth ends the turn.
const WARN_AT: u32 = 3;
const END_AT: u32 = 5;

/// Shared by every thread of one turn: an LLM call counts against the same budget wherever it
/// runs, and the wall clock started when the turn did.
pub(crate) struct TurnLimits {
    max_llm_calls: u32,
    max_turn_minutes: u32,
    started: Instant,
    calls: AtomicU32,
}

impl TurnLimits {
    pub(crate) fn new(config: &AgentConfig) -> Self {
        Self {
            max_llm_calls: config.max_llm_calls,
            max_turn_minutes: config.max_turn_minutes,
            started: Instant::now(),
            calls: AtomicU32::new(0),
        }
    }

    /// Called once per LLM call, before it's made. `Err` ends the turn with a clear error.
    pub(crate) fn check(&self) -> Result<()> {
        let elapsed = self.started.elapsed();
        let limit = Duration::from_secs(u64::from(self.max_turn_minutes) * 60);
        if elapsed > limit {
            bail!(
                "turn limit reached: {} minutes of wall clock",
                self.max_turn_minutes
            );
        }
        let calls = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if calls > self.max_llm_calls {
            bail!("turn limit reached: {} LLM calls", self.max_llm_calls);
        }
        Ok(())
    }
}

/// One thread's run-length count of identical consecutive tool calls (by name and arguments).
#[derive(Default)]
pub(crate) struct DoomLoop {
    last: Option<(String, String)>,
    repeat: u32,
}

pub(crate) enum DoomLoopAction {
    /// Nothing to do yet.
    None,
    /// The third identical call in a row: inject this warning as an internal user message.
    Warn(String),
    /// The fifth: end the turn with this error.
    End(String),
}

impl DoomLoop {
    /// Folds in one tool call the model just requested, in order.
    pub(crate) fn note(&mut self, name: &str, arguments: &str) -> DoomLoopAction {
        let key = (name.to_string(), arguments.to_string());
        if self.last.as_ref() == Some(&key) {
            self.repeat += 1;
        } else {
            self.last = Some(key);
            self.repeat = 1;
        }
        match self.repeat {
            END_AT => DoomLoopAction::End(format!(
                "doom loop: \"{name}\" called with identical arguments {END_AT} times in a row"
            )),
            WARN_AT => DoomLoopAction::Warn(format!(
                "{INTERNAL_START}You have called \"{name}\" with identical arguments {WARN_AT} \
                 times in a row. Reconsider your approach instead of repeating it.{INTERNAL_END}"
            )),
            _ => DoomLoopAction::None,
        }
    }
}
