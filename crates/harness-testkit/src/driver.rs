//! Runs a scenario's steps against a sidecar over any transport, so the in-process bridge tests
//! and the real-process test perform the same verbs and check the same expectations.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

use crate::{FakeProvider, Scenario, Step};

/// How long the driver waits for any one frame.
const FRAME_TIMEOUT: Duration = Duration::from_secs(20);

/// One end of a connection to a sidecar: whole JSON frames in, whole JSON frames out.
pub trait Transport {
    fn send(&mut self, frame: Value) -> impl Future<Output = Result<()>>;
    /// The next frame, or an error when the sidecar closed the stream.
    fn next(&mut self) -> impl Future<Output = Result<Value>>;
    /// Kills the sidecar without warning and starts a new one on the same data, waiting for its
    /// `ready` frame. Only a transport backed by a real process can do this.
    fn kill_and_restart(&mut self) -> impl Future<Output = Result<()>> {
        async { bail!("this transport cannot kill its sidecar") }
    }
}

/// What a finished run produced.
#[derive(Debug)]
pub struct Outcome {
    /// Every `event` frame received, in order.
    pub events: Vec<Value>,
}

impl Outcome {
    /// The events' `type`s in order.
    pub fn event_types(&self) -> Vec<String> {
        self.events
            .iter()
            .map(|f| f["event"]["type"].as_str().unwrap_or("?").to_string())
            .collect()
    }
}

struct Driver<'a, T> {
    transport: &'a mut T,
    events: Vec<Value>,
    /// Index of the first event no `wait_for` has consumed yet.
    cursor: usize,
    next_id: u32,
}

impl<T: Transport> Driver<'_, T> {
    async fn frame(&mut self) -> Result<Value> {
        let frame = tokio::time::timeout(FRAME_TIMEOUT, self.transport.next())
            .await
            .context("no frame from the sidecar in time")??;
        if frame["kind"] == "event" {
            self.events.push(frame.clone());
        }
        Ok(frame)
    }

    /// A request and its response; events that arrive first are collected.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = format!("d{}", self.next_id);
        self.transport
            .send(json!({"kind": "request", "id": id, "method": method, "params": params}))
            .await?;
        loop {
            let frame = self.frame().await?;
            if frame["kind"] == "response" && frame["id"] == id.as_str() {
                if frame["ok"] == true {
                    return Ok(frame["result"].clone());
                }
                bail!("{method} failed: {}", frame["error"]);
            }
        }
    }

    async fn wait_for(&mut self, event_type: &str) -> Result<Value> {
        loop {
            let found = self.events[self.cursor..]
                .iter()
                .position(|f| f["event"]["type"] == event_type);
            if let Some(offset) = found {
                let event = self.events[self.cursor + offset]["event"].clone();
                self.cursor += offset + 1;
                return Ok(event);
            }
            self.frame().await?;
        }
    }
}

/// Opens the scenario's workspace (restricted), creates a session, performs the steps, then
/// checks the scenario's expectations. `stub` is the provider the sidecar was configured with.
pub async fn run<T: Transport>(
    transport: &mut T,
    scenario: &Scenario,
    stub: &FakeProvider,
    workspace: &Path,
) -> Result<Outcome> {
    let mut driver = Driver {
        transport,
        events: Vec::new(),
        cursor: 0,
        next_id: 0,
    };
    let opened = driver
        .call("workspace.open", json!({"path": workspace}))
        .await?;
    let workspace_id = opened["workspace"]
        .as_str()
        .ok_or_else(|| anyhow!("workspace.open returned no workspace"))?
        .to_string();
    let session = driver
        .call("session.create", json!({"workspace": workspace_id}))
        .await?;
    let session_id = session["session"]["id"]
        .as_str()
        .ok_or_else(|| anyhow!("session.create returned no id"))?
        .to_string();
    let base = json!({"workspace": workspace_id, "session_id": session_id});
    let with = |extra: Value| {
        let mut params = base.clone();
        params
            .as_object_mut()
            .into_iter()
            .zip(extra.as_object())
            .for_each(|(p, e)| p.extend(e.clone()));
        params
    };

    for step in &scenario.steps {
        match step {
            Step::Send(input) => {
                driver
                    .call("turn.start", with(json!({"input": input})))
                    .await?;
            }
            Step::Approve(approval) => {
                let event = driver.wait_for("tool.approval_required").await?;
                let answer = serde_json::to_value(approval)?;
                driver
                    .call(
                        "action.reply",
                        with(json!({"action_id": event["action_id"], "answer": answer})),
                    )
                    .await?;
            }
            Step::Answer(text) => {
                let event = driver.wait_for("tool.response_required").await?;
                driver
                    .call(
                        "action.reply",
                        with(json!({"action_id": event["action_id"], "answer": {"text": text}})),
                    )
                    .await?;
            }
            Step::Cancel(_) => {
                let cancelled = driver.call("turn.cancel", base.clone()).await?;
                if cancelled["cancelled"] != true {
                    bail!("turn.cancel found no turn to cancel");
                }
            }
            Step::Release(gate) => stub.release(gate),
            Step::WaitFor(event_type) => {
                driver.wait_for(event_type).await?;
            }
            Step::Undo(_) => {
                let undone = driver.call("session.undo", base.clone()).await?;
                if undone["outcome"] != "restored" {
                    bail!("session.undo did not restore: {undone}");
                }
            }
            Step::KillSidecar(_) => {
                driver.transport.kill_and_restart().await?;
                // A new process picks the session up from the database; the turn it was in the
                // middle of never finished, which the transcript shows as no state.
                driver
                    .call("workspace.open", json!({"path": workspace_id}))
                    .await?;
                let resumed = driver.call("session.resume", base.clone()).await?;
                let last = resumed["turns"].as_array().and_then(|t| t.last());
                match last {
                    Some(turn) if turn["state"].is_null() => {}
                    other => bail!("the interrupted turn should have no state, got {other:?}"),
                }
            }
        }
    }

    let outcome = Outcome {
        events: driver.events,
    };
    check(
        scenario,
        stub,
        workspace,
        &outcome,
        &workspace_id,
        &session_id,
    )?;
    Ok(outcome)
}

fn check(
    scenario: &Scenario,
    stub: &FakeProvider,
    workspace: &Path,
    outcome: &Outcome,
    workspace_id: &str,
    session_id: &str,
) -> Result<()> {
    let expect = &scenario.expect;

    // Every event is routed to this workspace and session; the turn id is known from
    // `turn.created` on and never changes within the run's single turn.
    let mut turn_id: Option<&str> = None;
    for frame in &outcome.events {
        if frame["workspace"] != workspace_id || frame["session_id"] != session_id {
            bail!("an event carries the wrong routing: {frame}");
        }
        match (turn_id, frame["turn_id"].as_str()) {
            (None, Some(id)) => turn_id = Some(id),
            (Some(known), got) if got != Some(known) => {
                bail!("turn_id changed from {known} to {got:?}")
            }
            _ => {}
        }
    }
    let types = outcome.event_types();
    if types.first().map(String::as_str) != Some("turn.created") {
        bail!("the first event is {:?}, not turn.created", types.first());
    }
    if !expect.events.is_empty() && types != expect.events {
        bail!("events were {types:?}, expected {:?}", expect.events);
    }

    let requests = stub.requests();
    if let Some(count) = expect.requests
        && requests.len() != count
    {
        bail!(
            "the model got {} requests, expected {count}",
            requests.len()
        );
    }
    for contains in &expect.request_contains {
        let request = requests
            .get(contains.index)
            .ok_or_else(|| anyhow!("no request {}", contains.index))?;
        if !request.body.to_string().contains(&contains.text) {
            bail!(
                "request {} does not contain {:?}",
                contains.index,
                contains.text
            );
        }
    }
    for (path, content) in &expect.files {
        let actual = std::fs::read_to_string(workspace.join(path))
            .with_context(|| format!("reading {path}"))?;
        if &actual != content {
            bail!("{path} holds {actual:?}, expected {content:?}");
        }
    }
    let leftover = stub.leftover();
    if leftover.queued != 0 || !leftover.unmatched.is_empty() {
        bail!("provider leftovers {leftover:?}");
    }
    Ok(())
}
