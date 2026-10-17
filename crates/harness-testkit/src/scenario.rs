//! A scenario: the scripted provider replies, the steps a driver performs, and what must hold
//! afterwards. The one definition every test seam reads; the JSON files are the data.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{FakeProvider, ReplySpec};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    /// A directory under the testkit's `workspaces/`.
    pub workspace: String,
    pub replies: Vec<ReplySpec>,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub expect: Expect,
}

/// What a driver does, in order. A driver implements the verbs it can perform; `kill_sidecar`
/// is for process-level and Electron drivers only.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Send(String),
    Approve(Approval),
    Answer(String),
    Cancel(bool),
    Release(String),
    WaitFor(String),
    Undo(bool),
    KillSidecar(bool),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    Allow,
    AllowForSession,
    Deny,
}

/// Every driver checks the model-facing and file-system parts; `events` is for the Rust drivers
/// and `ui` for the Electron one.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    pub events: Vec<String>,
    /// How many requests the model was sent.
    pub requests: Option<usize>,
    #[serde(default)]
    pub request_contains: Vec<RequestContains>,
    /// Workspace-relative path to exact content after the run.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub ui: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RequestContains {
    pub index: usize,
    pub text: String,
}

impl Scenario {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading scenario {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing scenario {}", path.display()))
    }

    /// The scenario files shipped with the testkit, in name order.
    pub fn bundled_paths() -> Result<Vec<PathBuf>> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios");
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
            .with_context(|| format!("listing {}", dir.display()))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<_>>()?;
        paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
        paths.sort();
        Ok(paths)
    }

    /// Queues this scenario's replies on `stub`.
    pub fn script(&self, stub: &FakeProvider) -> Result<()> {
        for spec in &self.replies {
            stub.push_spec(spec)
                .map_err(|e| anyhow::anyhow!("scenario {}: {e}", self.name))?;
        }
        Ok(())
    }

    /// The scenario's workspace template directory.
    pub fn workspace_template(&self) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("workspaces")
            .join(&self.workspace)
    }
}
