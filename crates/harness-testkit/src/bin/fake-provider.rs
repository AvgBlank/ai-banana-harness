//! `fake-provider serve --scenario <file> [--stub name ...]`: stubs plus a `TestEnv` for tests
//! that run the product as a separate process (the sidecar tests and Playwright).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use harness_testkit::{FakeProvider, Scenario, TestEnv, TestEnvOptions};
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::signal::unix::{SignalKind, signal};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Starts the stubs, prints one JSON line describing them, and serves until stdin closes.
    Serve {
        #[arg(long)]
        scenario: PathBuf,
        /// Stub names; the scenario's replies go to the first. Defaults to `baseline`.
        #[arg(long = "stub")]
        stubs: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let Command::Serve { scenario, stubs } = Cli::parse().command;
    let scenario = Scenario::load(&scenario)?;
    let names = if stubs.is_empty() {
        vec!["baseline".to_string()]
    } else {
        stubs
    };

    let mut running = Vec::new();
    for name in &names {
        running.push((name.clone(), FakeProvider::start().await));
    }
    scenario.script(&running[0].1)?;

    let urls: BTreeMap<String, String> = running
        .iter()
        .map(|(name, stub)| (name.clone(), stub.base_url.clone()))
        .collect();
    let env = TestEnv::new(&TestEnvOptions {
        workspace_template: Some(&scenario.workspace_template()),
        stubs: urls.clone(),
    })?;

    let stubs_json: BTreeMap<_, _> = urls
        .iter()
        .map(|(name, url)| {
            (
                name,
                json!({ "provider_url": url, "control_url": format!("{url}/__fake") }),
            )
        })
        .collect();
    let line = json!({
        "root": env.root(),
        "workspace": env.workspace(),
        "env": env.vars(),
        "stubs": stubs_json,
    });
    let mut stdout = std::io::stdout();
    writeln!(stdout, "{line}").context("printing the description")?;
    stdout.flush().context("flushing the description")?;

    wait_for_exit().await;
    if std::env::var("BH_TEST_KEEP").is_ok_and(|v| v == "1") {
        env.keep();
    } else {
        drop(env);
    }
    // The stdin read may still be blocking its thread; exit rather than wait for it.
    std::process::exit(0);
}

/// Returns when stdin reaches end of file (the parent went away) or SIGTERM arrives.
async fn wait_for_exit() {
    let eof = async {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0_u8; 256];
        while matches!(stdin.read(&mut buf).await, Ok(n) if n > 0) {}
    };
    match signal(SignalKind::terminate()) {
        Ok(mut term) => {
            tokio::select! {
                () = eof => {}
                _ = term.recv() => {}
            }
        }
        Err(_) => eof.await,
    }
}
