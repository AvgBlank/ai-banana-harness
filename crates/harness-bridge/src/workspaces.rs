//! The open workspaces: one `Harness` per canonical path, each with its own `Store` handle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use harness_core::{
    Config, ConfigPaths, Harness, HarnessOptions, Provider, Store, last_provider,
    workspace_config_path,
};
use tokio::sync::OnceCell;

use crate::frame::BridgeError;
use crate::turns::Turns;

/// How long a closing workspace or a shutdown waits for a cancelled turn to end.
const TURN_STOP_LIMIT: std::time::Duration = std::time::Duration::from_secs(3);

pub(crate) struct OpenWorkspace {
    /// The canonical path, the workspace's identity on the wire.
    pub(crate) path: String,
    pub(crate) harness: Arc<Harness>,
    pub(crate) turns: Arc<Turns>,
    /// The provider new sessions start with.
    pub(crate) provider: Provider,
    pub(crate) config_path: PathBuf,
    pub(crate) config_present: bool,
    /// The workspace's own config layer is in effect (it exists and was trusted at open).
    pub(crate) config_loaded: bool,
}

/// What an open needs besides the request: the paths and key the bridge was started with.
pub(crate) struct OpenEnvironment<'a> {
    pub(crate) db_path: &'a std::path::Path,
    pub(crate) user_config: Option<&'a std::path::Path>,
    pub(crate) api_key: &'a str,
}

pub(crate) struct OpenRequest {
    pub(crate) path: String,
    pub(crate) trust_workspace_config: bool,
    pub(crate) provider: Option<Provider>,
}

#[derive(Default)]
pub(crate) struct Workspaces {
    // A cell per path, so concurrent opens of one path run a single open and share its result.
    cells: Mutex<HashMap<String, Arc<OnceCell<Arc<OpenWorkspace>>>>>,
}

impl Workspaces {
    /// Opens the workspace, or returns the one already open; the flag says which.
    pub(crate) async fn open(
        &self,
        request: OpenRequest,
        env: &OpenEnvironment<'_>,
    ) -> Result<(Arc<OpenWorkspace>, bool), BridgeError> {
        if env.api_key.trim().is_empty() {
            return Err(BridgeError::ApiKeyMissing);
        }
        let canonical = tokio::fs::canonicalize(&request.path)
            .await
            .ok()
            .filter(|path| path.is_dir())
            .ok_or_else(|| BridgeError::WorkspaceNotFound(request.path.clone()))?;
        let key = canonical.to_string_lossy().into_owned();

        let cell = {
            let mut cells = self.cells.lock().map_err(poisoned)?;
            Arc::clone(cells.entry(key.clone()).or_default())
        };
        let mut opened_here = false;
        let result = cell
            .get_or_try_init(|| async {
                opened_here = true;
                let (db_path, user_config, api_key) = (
                    env.db_path.to_path_buf(),
                    env.user_config.map(std::path::Path::to_path_buf),
                    env.api_key.to_string(),
                );
                let (trust, provider) = (request.trust_workspace_config, request.provider);
                let built = tokio::task::spawn_blocking(move || {
                    build(canonical, db_path, user_config, api_key, trust, provider)
                })
                .await
                .map_err(|e| BridgeError::Internal(e.into()))?;
                built.map(Arc::new).map_err(BridgeError::Internal)
            })
            .await;
        match result {
            Ok(workspace) => Ok((Arc::clone(workspace), !opened_here)),
            Err(e) => {
                // A failed open leaves nothing behind, so the next attempt starts fresh.
                let mut cells = self.cells.lock().map_err(poisoned)?;
                if cells.get(&key).is_some_and(|c| Arc::ptr_eq(c, &cell)) {
                    cells.remove(&key);
                }
                Err(e)
            }
        }
    }

    pub(crate) fn get(&self, path: &str) -> Result<Arc<OpenWorkspace>, BridgeError> {
        self.cells
            .lock()
            .map_err(poisoned)?
            .get(path)
            .and_then(|cell| cell.get().cloned())
            .ok_or_else(|| BridgeError::WorkspaceNotOpen(path.to_string()))
    }

    /// Cancels the workspace's turn, waits for it to end, and drops its harness; its sessions
    /// stay in the database.
    pub(crate) async fn close(&self, path: &str) -> Result<(), BridgeError> {
        let workspace = self.get(path)?;
        workspace.turns.cancel_any().await;
        workspace.turns.wait_idle(TURN_STOP_LIMIT).await;
        self.cells.lock().map_err(poisoned)?.remove(path);
        Ok(())
    }

    /// Cancels every running turn and waits up to [`TURN_STOP_LIMIT`] for them all to end.
    pub(crate) async fn stop_turns(&self) {
        let Ok(open) = self.list() else {
            return;
        };
        for workspace in &open {
            workspace.turns.cancel_any().await;
        }
        let deadline = tokio::time::Instant::now() + TURN_STOP_LIMIT;
        for workspace in &open {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            workspace.turns.wait_idle(left).await;
        }
    }

    /// The open workspaces in path order.
    pub(crate) fn list(&self) -> Result<Vec<Arc<OpenWorkspace>>, BridgeError> {
        let mut open: Vec<_> = self
            .cells
            .lock()
            .map_err(poisoned)?
            .values()
            .filter_map(|cell| cell.get().cloned())
            .collect();
        open.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(open)
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> BridgeError {
    BridgeError::Internal(anyhow::anyhow!("workspace table lock poisoned"))
}

fn build(
    canonical: PathBuf,
    db_path: PathBuf,
    user_config: Option<PathBuf>,
    api_key: String,
    trust_workspace_config: bool,
    requested: Option<Provider>,
) -> anyhow::Result<OpenWorkspace> {
    let store = Store::open(&db_path)?;
    let config_path = workspace_config_path(&canonical);
    let config_present = config_path.is_file();
    // The workspace layer can start commands, loosen guardrails and redirect the key; it loads
    // only on an explicit trust decision.
    let config_loaded = trust_workspace_config && config_present;
    let config = Config::load(ConfigPaths {
        user: user_config.as_deref(),
        workspace: config_loaded.then_some(config_path.as_path()),
    })
    .context("loading the configuration")?;
    let provider = requested
        .or_else(|| last_provider(&store))
        .unwrap_or(Provider::DeepSeek);
    let harness = Harness::open(HarnessOptions {
        config,
        store,
        provider,
        api_key,
        workspace: canonical.clone(),
        headless: false,
        container: None,
    })
    .context("opening the harness")?;
    Ok(OpenWorkspace {
        path: canonical.to_string_lossy().into_owned(),
        harness: Arc::new(harness),
        turns: Arc::default(),
        provider,
        config_path,
        config_present,
        config_loaded,
    })
}
