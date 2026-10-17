//! The request handlers. Params are parsed per method; unknown fields are ignored.

use std::path::PathBuf;
use std::sync::Arc;

use harness_core::{Provider, Redactor, Store};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::frame::BridgeError;
use crate::workspaces::{OpenEnvironment, OpenRequest, OpenWorkspace, Workspaces};

pub(crate) struct Context {
    pub(crate) store: Arc<Store>,
    pub(crate) redactor: Redactor,
    pub(crate) workspaces: Workspaces,
    pub(crate) db_path: PathBuf,
    pub(crate) user_config: Option<PathBuf>,
    pub(crate) api_key: String,
}

fn params<T: DeserializeOwned>(map: Map<String, Value>) -> Result<T, BridgeError> {
    serde_json::from_value(Value::Object(map))
        .map_err(|e| BridgeError::InvalidParams(e.to_string()))
}

fn provider_param(name: &str) -> Result<Provider, BridgeError> {
    name.parse()
        .map_err(|e: anyhow::Error| BridgeError::InvalidParams(e.to_string()))
}

fn internal(e: impl Into<anyhow::Error>) -> BridgeError {
    BridgeError::Internal(e.into())
}

/// Runs blocking core work (SQLite, git) off the async threads.
async fn blocking<T, F>(work: F) -> Result<T, BridgeError>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(internal)?
        .map_err(BridgeError::Internal)
}

#[derive(Deserialize)]
struct WorkspaceParams {
    workspace: String,
}

#[derive(Deserialize)]
struct SessionParams {
    workspace: String,
    session_id: String,
}

#[derive(Deserialize)]
struct OpenParams {
    path: String,
    #[serde(default)]
    trust_workspace_config: bool,
    provider: Option<String>,
}

#[derive(Deserialize)]
struct CreateParams {
    workspace: String,
    provider: Option<String>,
}

#[derive(Deserialize)]
struct RenameParams {
    workspace: String,
    session_id: String,
    title: String,
}

#[derive(Deserialize)]
struct SwitchParams {
    workspace: String,
    session_id: String,
    provider: String,
}

pub(crate) async fn dispatch(
    context: &Context,
    method: &str,
    map: Map<String, Value>,
) -> Result<Value, BridgeError> {
    match method {
        "shutdown" => Ok(json!({})),
        "session.list" => {
            let store = Arc::clone(&context.store);
            let sessions = blocking(move || store.sessions()).await?;
            Ok(json!({ "sessions": sessions }))
        }
        "workspace.open" => open(context, params(map)?).await,
        "workspace.close" => {
            let p: WorkspaceParams = params(map)?;
            context.workspaces.close(&p.workspace)?;
            Ok(json!({}))
        }
        "workspace.list" => {
            let workspaces: Vec<Value> = context
                .workspaces
                .list()?
                .iter()
                .map(|w| json!({"workspace": w.path, "provider": w.provider, "active_turn": null}))
                .collect();
            Ok(json!({ "workspaces": workspaces }))
        }
        "provider.list" => {
            let p: WorkspaceParams = params(map)?;
            let workspace = context.workspaces.get(&p.workspace)?;
            let providers = blocking({
                let workspace = Arc::clone(&workspace);
                move || workspace.harness.providers()
            })
            .await?;
            Ok(json!({"providers": providers, "default": workspace.provider}))
        }
        "session.create" => create(context, params(map)?).await,
        "session.resume" => {
            let p: SessionParams = params(map)?;
            let workspace = context.workspaces.get(&p.workspace)?;
            let session = session_in(&workspace, &p.session_id).await?;
            let turns = blocking({
                let workspace = Arc::clone(&workspace);
                move || workspace.harness.transcript(&p.session_id)
            })
            .await?;
            Ok(json!({"session": session, "turns": turns}))
        }
        "session.rename" => {
            let p: RenameParams = params(map)?;
            if p.title.trim().is_empty() {
                return Err(BridgeError::InvalidParams(
                    "`title` must not be empty".into(),
                ));
            }
            let workspace = context.workspaces.get(&p.workspace)?;
            session_in(&workspace, &p.session_id).await?;
            blocking({
                let (workspace, id) = (Arc::clone(&workspace), p.session_id.clone());
                move || workspace.harness.rename_session(&id, &p.title)
            })
            .await?;
            let session = session_in(&workspace, &p.session_id).await?;
            Ok(json!({ "session": session }))
        }
        "session.switch_provider" => {
            let p: SwitchParams = params(map)?;
            let provider = provider_param(&p.provider)?;
            let workspace = context.workspaces.get(&p.workspace)?;
            session_in(&workspace, &p.session_id).await?;
            blocking({
                let (workspace, id) = (Arc::clone(&workspace), p.session_id.clone());
                move || workspace.harness.switch_provider(&id, provider)
            })
            .await?;
            let session = session_in(&workspace, &p.session_id).await?;
            Ok(json!({ "session": session }))
        }
        "changes.workspace" => {
            let p: WorkspaceParams = params(map)?;
            let workspace = context.workspaces.get(&p.workspace)?;
            let changes = blocking(move || workspace.harness.workspace_changes()).await?;
            Ok(json!(changes))
        }
        "changes.turn" => {
            let p: SessionParams = params(map)?;
            let workspace = context.workspaces.get(&p.workspace)?;
            session_in(&workspace, &p.session_id).await?;
            let changes = blocking(move || workspace.harness.turn_changes(&p.session_id)).await?;
            Ok(json!(changes))
        }
        other => Err(BridgeError::UnknownMethod(other.to_string())),
    }
}

async fn open(context: &Context, p: OpenParams) -> Result<Value, BridgeError> {
    let provider = p.provider.as_deref().map(provider_param).transpose()?;
    let (workspace, already_open) = context
        .workspaces
        .open(
            OpenRequest {
                path: p.path,
                trust_workspace_config: p.trust_workspace_config,
                provider,
            },
            &OpenEnvironment {
                db_path: &context.db_path,
                user_config: context.user_config.as_deref(),
                api_key: &context.api_key,
            },
        )
        .await?;
    Ok(json!({
        "workspace": workspace.path,
        "already_open": already_open,
        "provider": workspace.provider,
        "workspace_config": {
            "present": workspace.config_present,
            "loaded": workspace.config_loaded,
            "path": workspace.config_path,
        },
    }))
}

async fn create(context: &Context, p: CreateParams) -> Result<Value, BridgeError> {
    let provider = p.provider.as_deref().map(provider_param).transpose()?;
    let workspace = context.workspaces.get(&p.workspace)?;
    let id = blocking({
        let workspace = Arc::clone(&workspace);
        move || {
            let id = workspace.harness.create_session()?;
            if let Some(provider) = provider {
                workspace.harness.switch_provider(&id, provider)?;
            }
            Ok(id)
        }
    })
    .await?;
    Ok(json!({"session": session_in(&workspace, &id).await?}))
}

/// The session, which must exist and belong to this workspace.
async fn session_in(workspace: &Arc<OpenWorkspace>, id: &str) -> Result<Value, BridgeError> {
    let (harness, wanted) = (Arc::clone(&workspace.harness), id.to_string());
    let session = blocking(move || harness.session(&wanted))
        .await?
        .ok_or_else(|| BridgeError::SessionNotFound(id.to_string()))?;
    if session.workspace != workspace.path {
        return Err(BridgeError::WorkspaceMismatch(id.to_string()));
    }
    Ok(json!(session))
}
