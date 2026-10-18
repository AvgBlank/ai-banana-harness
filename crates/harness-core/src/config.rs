use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use harness_mcp::McpServerConfig;
use harness_tools::{ContainerConfig, ContainerNetwork};
use serde::{Deserialize, Serialize};

const DEFAULT_CONFIG: &str = include_str!("../../../config/default.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub providers: Providers,
    pub agent: AgentConfig,
    pub skills: SkillsConfig,
    pub tools: harness_tools::ToolsConfig,
    pub permission: harness_tools::PermissionConfig,
    /// `[mcp.<name>]` servers, keyed by name.
    #[serde(default)]
    pub mcp: BTreeMap<String, McpServerConfig>,
    /// Where `bash` commands run (`[tools.bash]`): read from the embedded defaults and the user
    /// config only, never from a workspace config. Not part of the serde shape of [`Config`].
    #[serde(skip)]
    pub command_runner: CommandRunnerConfig,
}

/// Where `bash` commands run. The host unless the user config asks for a container.
#[derive(Clone, Debug, Default)]
pub enum CommandRunnerConfig {
    #[default]
    Host,
    Container(ContainerConfig),
}

/// `[tools.bash]`: `runner = "host" | "container"` and, for a container, `[tools.bash.container]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BashSettings {
    #[serde(default)]
    runner: RunnerKind,
    container: Option<ContainerSettings>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RunnerKind {
    #[default]
    Host,
    Container,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContainerSettings {
    image: Option<String>,
    network: Option<String>,
    memory: Option<String>,
    pids_limit: Option<u32>,
    tmp_size: Option<String>,
    docker: Option<String>,
}

impl BashSettings {
    fn into_config(self) -> Result<CommandRunnerConfig> {
        let RunnerKind::Container = self.runner else {
            return Ok(CommandRunnerConfig::Host);
        };
        let settings = self.container.ok_or_else(|| {
            anyhow!(
                "[tools.bash] runner = \"container\" needs a [tools.bash.container] table with \
                 `image`, an image already on this machine (Banana Harness never pulls one)"
            )
        })?;
        let image = settings
            .image
            .filter(|image| !image.trim().is_empty())
            .ok_or_else(|| {
                anyhow!(
                    "[tools.bash.container] needs `image`, an image already on this machine \
                     (Banana Harness never pulls one)"
                )
            })?;
        let mut container = ContainerConfig::new(image);
        match settings.network.as_deref() {
            None | Some("none") => container.network = ContainerNetwork::None,
            Some("bridge") => container.network = ContainerNetwork::Bridge,
            Some(other) => bail!(
                "[tools.bash.container] network = \"{other}\" is not valid: use \"none\" (the \
                 default) or \"bridge\""
            ),
        }
        if let Some(memory) = settings.memory {
            container.memory = docker_size("memory", memory)?;
        }
        if let Some(size) = settings.tmp_size {
            container.tmp_size = docker_size("tmp_size", size)?;
        }
        if let Some(limit) = settings.pids_limit {
            if limit == 0 {
                bail!("[tools.bash.container] pids_limit must be at least 1");
            }
            container.pids_limit = limit;
        }
        if let Some(docker) = settings.docker {
            if docker.trim().is_empty() {
                bail!("[tools.bash.container] docker must name the docker command");
            }
            container.docker = docker.into();
        }
        Ok(CommandRunnerConfig::Container(container))
    }
}

/// A size in Docker's notation: a positive number with an optional `b`, `k`, `m` or `g`.
fn docker_size(key: &str, value: String) -> Result<String> {
    let digits = value.trim_end_matches(|c: char| "bBkKmMgG".contains(c));
    let unit_len = value.len() - digits.len();
    let valid = unit_len <= 1
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
        && digits.parse::<u64>().is_ok_and(|n| n > 0);
    if valid {
        Ok(value)
    } else {
        bail!(
            "[tools.bash.container] {key} = \"{value}\" is not a size: use a number with an optional b, k, m or g, such as \"2g\""
        )
    }
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
    /// Tokens reserved for the model's reply: lowers the compaction threshold, and is also sent
    /// to the provider as `max_tokens` so a long reply doesn't breach the provider's own default.
    pub max_output: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentConfig {
    pub instructions: String,
    pub subagents: bool,
    /// A turn's LLM-call budget, shared across its root thread and every subagent.
    pub max_llm_calls: u32,
    /// A turn's wall-clock budget in minutes, from the turn's start.
    pub max_turn_minutes: u32,
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
        for (path, is_workspace) in [(paths.user, false), (paths.workspace, true)]
            .into_iter()
            .filter_map(|(path, is_workspace)| path.map(|p| (p, is_workspace)))
        {
            if !path.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading config {}", path.display()))?;
            let mut overlay: toml::Value = toml::from_str(&text)
                .with_context(|| format!("parsing config {}", path.display()))?;
            if is_workspace {
                // Where commands run is the user's choice: a workspace cannot pick or loosen it.
                if let Some(tools) = overlay.get_mut("tools").and_then(toml::Value::as_table_mut) {
                    tools.remove("bash");
                }
            }
            merge_toml(&mut merged, overlay);
        }
        let command_runner = match merged.get("tools").and_then(|tools| tools.get("bash")) {
            Some(bash) => bash
                .clone()
                .try_into::<BashSettings>()
                .context("reading [tools.bash]")?
                .into_config()?,
            None => CommandRunnerConfig::Host,
        };
        let mut config: Self = merged.try_into().context("validating the merged config")?;
        config.command_runner = command_runner;
        Ok(config)
    }

    pub fn provider(&self, provider: Provider) -> &ProviderConfig {
        match provider {
            Provider::DeepSeek => &self.providers.deepseek,
            Provider::Qwen => &self.providers.qwen,
        }
    }

    /// The providers in the picker's order, each with where its API key would be sent. Fails
    /// when a `base_url` is not a usable URL, because its destination could not be shown or
    /// checked; the message never repeats the URL, which may carry credentials.
    pub(crate) fn provider_options(&self) -> Result<Vec<ProviderOption>> {
        [Provider::DeepSeek, Provider::Qwen]
            .into_iter()
            .map(|provider| {
                let config = self.provider(provider);
                let (scheme, host) = key_destination(&config.base_url)
                    .with_context(|| format!("provider `{provider}` has an unusable base_url"))?;
                Ok(ProviderOption {
                    provider,
                    model: config.model.clone(),
                    host,
                    scheme,
                })
            })
            .collect()
    }
}

/// One selectable provider as a picker shows it: what a session would run and where the API
/// key would be sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderOption {
    pub provider: Provider,
    pub model: String,
    /// The `base_url` authority: the host plus an explicit non-default port, never userinfo.
    pub host: String,
    /// `https` for the default endpoints; shown so a downgrade to `http` is visible.
    pub scheme: String,
}

/// The scheme and authority of `base_url`, which together name where a request, and the key
/// in its header, would go.
fn key_destination(base_url: &str) -> Result<(String, String)> {
    let url = url::Url::parse(base_url).context("not a valid URL")?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("the URL has no host"))?;
    let authority = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    Ok((url.scheme().to_string(), authority))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
