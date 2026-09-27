use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use etcetera::BaseStrategy;
use harness_core::{
    APP_SLUG, Config, ConfigPaths, EventKind, Harness, HarnessOptions, Provider, Store, TurnState,
    api_key_from_env, default_db_path,
};

#[derive(Parser)]
#[command(version, about = "A local-first coding agent")]
struct Cli {
    /// With no subcommand, the TUI opens.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run one task without the TUI and print the result.
    Run {
        /// deepseek or qwen
        #[arg(long)]
        provider: Provider,
        /// The repository to work in; defaults to $WORKSPACE, then the current directory.
        #[arg(long, env = "WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Print every event as one JSON object per line.
        #[arg(long)]
        json: bool,
        task: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let Cli { command } = Cli::parse();
    let result = match command {
        None => harness_tui::run().await.map(|()| ExitCode::SUCCESS),
        Some(Command::Run {
            provider,
            workspace,
            json,
            task,
        }) => headless(provider, workspace, json, task).await,
    };
    result.unwrap_or_else(|e| {
        eprintln!("error: {e:#}");
        ExitCode::from(1)
    })
}

async fn headless(
    provider: Provider,
    workspace: Option<PathBuf>,
    json: bool,
    task: String,
) -> Result<ExitCode> {
    let api_key = api_key_from_env()?;
    let workspace = match workspace {
        Some(path) => path,
        None => std::env::current_dir()?,
    };
    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("workspace {} not found", workspace.display()))?;
    let user_config = etcetera::choose_base_strategy()
        .ok()
        .map(|base| base.config_dir().join(APP_SLUG).join("config.toml"));
    let workspace_config = workspace.join(".banana-harness").join("config.toml");
    let config = Config::load(ConfigPaths {
        user: user_config.as_deref(),
        workspace: Some(&workspace_config),
    })?;
    let harness = Harness::open(HarnessOptions {
        config,
        store: Store::open(&default_db_path()?)?,
        provider,
        api_key,
        workspace,
        headless: true,
    })?;
    let session_id = harness.create_session()?;
    let mut turn = harness.start_turn(&session_id, task);

    let mut stdout = std::io::stdout();
    let mut exit = ExitCode::from(1);
    let mut printed = false;
    // Plain output is the root thread's answer only; subagents' text would interleave with it.
    let mut subagents = std::collections::HashSet::new();
    while let Some(event) = turn.events.recv().await {
        if let EventKind::TurnDone { state, .. } = &event.kind {
            exit = match state {
                TurnState::Done => ExitCode::SUCCESS,
                TurnState::Error => ExitCode::from(1),
                TurnState::Cancelled => ExitCode::from(2),
            };
        }
        if json {
            writeln!(stdout, "{}", serde_json::to_string(&event)?)?;
            continue;
        }
        match event.kind {
            EventKind::ThreadCreated { thread_id, .. } => {
                subagents.insert(thread_id);
            }
            EventKind::ModelMessageDelta {
                thread_id, content, ..
            } if !subagents.contains(&thread_id) => {
                write!(stdout, "{content}")?;
                stdout.flush()?;
                printed = true;
            }
            EventKind::TurnDone { error, .. } => {
                if printed {
                    writeln!(stdout)?;
                }
                if let Some(error) = error {
                    eprintln!("error: {error}");
                }
            }
            _ => {}
        }
    }
    Ok(exit)
}
