//! Config, events and the turn runner behind both surfaces.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use etcetera::BaseStrategy;

mod config;
mod event;
mod harness;

pub use config::{AgentConfig, Config, Provider, ProviderConfig, Providers};
pub use event::{Event, EventKind, ModelOutput, TurnState};
pub use harness::{Harness, HarnessOptions, TurnHandle};
pub use harness_llm::Usage;
pub use harness_store::Store;

/// The product name. Renaming the product touches these two constants and the binary's package name.
pub const APP_NAME: &str = "Banana Harness";
pub const APP_SLUG: &str = "banana-harness";

pub const API_KEY_ENV: &str = "AI_API_KEY";

/// The provider API key. It is read only from the environment, never from a config file.
pub fn api_key_from_env() -> Result<String> {
    match std::env::var(API_KEY_ENV) {
        Ok(key) if !key.trim().is_empty() => Ok(key.trim().to_string()),
        _ => bail!("{API_KEY_ENV} is not set; export your DeepSeek or Qwen API key"),
    }
}

/// `<data dir>/banana-harness/harness.db`, using XDG-style paths on macOS too.
pub fn default_db_path() -> Result<PathBuf> {
    let base = etcetera::choose_base_strategy().context("locating the home directory")?;
    Ok(base.data_dir().join(APP_SLUG).join("harness.db"))
}
