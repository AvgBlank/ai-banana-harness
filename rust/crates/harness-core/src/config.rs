use std::fmt;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const DEFAULT_CONFIG: &str = include_str!("../../../config/default.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub providers: Providers,
    pub agent: AgentConfig,
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
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentConfig {
    pub instructions: String,
}

impl Config {
    /// The committed default config compiled into the binary.
    pub fn embedded_default() -> Result<Self> {
        toml::from_str(DEFAULT_CONFIG).context("parsing the embedded default config")
    }

    pub fn provider(&self, provider: Provider) -> &ProviderConfig {
        match provider {
            Provider::DeepSeek => &self.providers.deepseek,
            Provider::Qwen => &self.providers.qwen,
        }
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
