use std::fmt;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const DEFAULT_CONFIG: &str = include_str!("../../../config/default.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub providers: Providers,
    pub agent: AgentConfig,
    pub skills: SkillsConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Providers {
    pub deepseek: ProviderConfig,
    pub qwen: ProviderConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub seed: Option<u64>,
    /// The model's context window in tokens. Unset, compaction falls back to a fixed threshold.
    pub context_length: Option<u64>,
    /// Tokens reserved for the model's reply, which lowers the compaction threshold.
    pub max_output: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentConfig {
    pub instructions: String,
    pub subagents: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SkillsConfig {
    pub paths: Vec<String>,
}

/// Optional overlay files, later ones winning. Both are skipped silently when absent.
#[derive(Default)]
pub struct ConfigPaths<'a> {
    pub user: Option<&'a Path>,
    pub workspace: Option<&'a Path>,
}

impl Config {
    /// The committed default config compiled into the binary.
    pub fn embedded_default() -> Result<Self> {
        toml::from_str(DEFAULT_CONFIG).context("parsing the embedded default config")
    }

    /// Layers the embedded default, then the user config, then the workspace config, each
    /// later layer overriding matching keys in the one before it. A missing overlay file is
    /// not an error; a malformed one is.
    pub fn load(paths: ConfigPaths<'_>) -> Result<Self> {
        let mut merged: toml::Value =
            toml::from_str(DEFAULT_CONFIG).context("parsing the embedded default config")?;
        for path in [paths.user, paths.workspace].into_iter().flatten() {
            if !path.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading config {}", path.display()))?;
            let overlay: toml::Value = toml::from_str(&text)
                .with_context(|| format!("parsing config {}", path.display()))?;
            merge_toml(&mut merged, overlay);
        }
        merged.try_into().context("validating the merged config")
    }

    pub fn provider(&self, provider: Provider) -> &ProviderConfig {
        match provider {
            Provider::DeepSeek => &self.providers.deepseek,
            Provider::Qwen => &self.providers.qwen,
        }
    }
}

/// Deep-merges TOML tables; `overlay` wins on every key it sets, recursing into nested tables
/// so a layer can override a single field without repeating its siblings.
fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                match base_table.get_mut(&key) {
                    Some(existing) => merge_toml(existing, value),
                    None => {
                        base_table.insert(key, value);
                    }
                }
            }
        }
        (base_slot, overlay_value) => *base_slot = overlay_value,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    DeepSeek,
    Qwen,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::DeepSeek => "deepseek",
            Provider::Qwen => "qwen",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "deepseek" => Ok(Provider::DeepSeek),
            "qwen" => Ok(Provider::Qwen),
            other => bail!("unknown provider `{other}`; expected `deepseek` or `qwen`"),
        }
    }
}
