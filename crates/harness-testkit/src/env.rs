//! `TestEnv`: one temp root per test and the exact environment a process under test runs in, so
//! the developer's real keys, proxies, git settings and data directories can never reach it.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use tempfile::TempDir;

use crate::TEST_API_KEY;

/// A dead outbound proxy: with loopback excluded through `NO_PROXY`, any request that is not to
/// a stub fails at once instead of reaching the internet.
const DEAD_PROXY: &str = "http://127.0.0.1:9";

#[derive(Clone, Debug, Default)]
pub struct TestEnvOptions<'a> {
    /// A workspace template directory to copy; `None` gives an empty repository.
    pub workspace_template: Option<&'a Path>,
    /// Stub name to its provider URL. The `baseline` stub (else the first by name) is the one
    /// both providers' `base_url` point at; every stub is available as `{{stub:name}}`.
    pub stubs: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySetup {
    Fixed,
    /// No key anywhere: the process must report that it needs one.
    Missing,
    /// No `AI_API_KEY`; `SHELL` is a script that prints one, for the login-shell probe.
    Shell(PathBuf),
}

pub struct TestEnv {
    root: Option<TempDir>,
    root_path: PathBuf,
    workspace: PathBuf,
    vars: BTreeMap<String, String>,
}

impl TestEnv {
    pub fn new(options: &TestEnvOptions) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("bh-test-")
            .tempdir()
            .context("creating the test root")?;
        // Canonical: a temp dir on macOS resolves through a symlink, and the product identifies
        // a workspace by its canonical path.
        let root_path = dir
            .path()
            .canonicalize()
            .context("canonicalizing the root")?;
        let home = root_path.join("home");
        let config = root_path.join("config");
        let data = root_path.join("data");
        for path in [&home, &config, &data] {
            std::fs::create_dir_all(path)
                .with_context(|| format!("creating {}", path.display()))?;
        }

        let mut vars = BTreeMap::new();
        for name in ["PATH", "DISPLAY"] {
            if let Ok(value) = std::env::var(name) {
                vars.insert(name.to_string(), value);
            }
        }
        let mut set = |name: &str, value: String| vars.insert(name.to_string(), value);
        set("HOME", home.display().to_string());
        set("XDG_CONFIG_HOME", config.display().to_string());
        set("XDG_DATA_HOME", data.display().to_string());
        set(
            "XDG_CACHE_HOME",
            root_path.join("cache").display().to_string(),
        );
        set(
            "XDG_STATE_HOME",
            root_path.join("state").display().to_string(),
        );
        for name in ["NO_PROXY", "no_proxy"] {
            set(name, "127.0.0.1,localhost".to_string());
        }
        for name in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            set(name, DEAD_PROXY.to_string());
        }
        set("AI_API_KEY", TEST_API_KEY.to_string());

        let workspace_dir = root_path.join("workspace");
        std::fs::create_dir_all(&workspace_dir).context("creating the workspace")?;
        let workspace = workspace_dir
            .canonicalize()
            .context("canonicalizing the workspace")?;
        let substitutions = Substitutions {
            stubs: &options.stubs,
            workspace: &workspace,
            root: &root_path,
        };
        if let Some(template) = options.workspace_template {
            copy_template(template, &workspace, &substitutions)?;
        }
        set("WORKSPACE", workspace.display().to_string());

        let baseline = options
            .stubs
            .get("baseline")
            .or_else(|| options.stubs.values().next());
        if let Some(url) = baseline {
            let app_config = config.join(harness_slug());
            std::fs::create_dir_all(&app_config).context("creating the app config dir")?;
            std::fs::write(
                app_config.join("config.toml"),
                format!(
                    "[providers.deepseek]\nbase_url = \"{url}\"\n\n[providers.qwen]\nbase_url = \"{url}\"\n"
                ),
            )
            .context("writing the user config")?;
        }

        init_repository(&workspace, &vars)?;
        Ok(Self {
            root: Some(dir),
            root_path,
            workspace,
            vars,
        })
    }

    /// No `AI_API_KEY` at all.
    pub fn without_key(mut self) -> Self {
        self.vars.remove("AI_API_KEY");
        self
    }

    /// No `AI_API_KEY`, and `SHELL` pointing at a script that prints one.
    pub fn shell(mut self, script: &Path) -> Self {
        self.vars.remove("AI_API_KEY");
        self.vars
            .insert("SHELL".to_string(), script.display().to_string());
        self
    }

    pub fn with_key(mut self, setup: KeySetup) -> Self {
        match setup {
            KeySetup::Fixed => self,
            KeySetup::Missing => {
                self.vars.remove("AI_API_KEY");
                self
            }
            KeySetup::Shell(path) => self.shell(&path),
        }
    }

    /// The temp root (canonical).
    pub fn root(&self) -> &Path {
        &self.root_path
    }

    /// The workspace's canonical path.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// The complete environment of the process under test.
    pub fn vars(&self) -> &BTreeMap<String, String> {
        &self.vars
    }

    /// The marker file `{{marker}}` expands to; a test checks it was never created.
    pub fn marker(&self) -> PathBuf {
        self.root_path.join("marker")
    }

    /// A command that runs in the workspace with exactly this environment and nothing else.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .envs(&self.vars)
            .current_dir(&self.workspace);
        command
    }

    /// Keeps the root on disk (for `BH_TEST_KEEP=1`) and returns where it is.
    pub fn keep(mut self) -> PathBuf {
        if let Some(dir) = self.root.take() {
            let _ = dir.keep();
        }
        self.root_path.clone()
    }
}

fn harness_slug() -> &'static str {
    // The product's data and config folder name; the testkit may not depend on harness-core.
    "banana-harness"
}

struct Substitutions<'a> {
    stubs: &'a BTreeMap<String, String>,
    workspace: &'a Path,
    root: &'a Path,
}

impl Substitutions<'_> {
    fn apply(&self, text: &str) -> String {
        let mut out = text
            .replace("{{workspace}}", &self.workspace.display().to_string())
            .replace("{{root}}", &self.root.display().to_string())
            .replace(
                "{{marker}}",
                &self.root.join("marker").display().to_string(),
            );
        for (name, url) in self.stubs {
            out = out.replace(&format!("{{{{stub:{name}}}}}"), url);
        }
        out
    }
}

fn copy_template(from: &Path, to: &Path, substitutions: &Substitutions) -> Result<()> {
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_template(&entry.path(), &target, substitutions)?;
        } else {
            match std::fs::read_to_string(entry.path()) {
                Ok(text) => std::fs::write(&target, substitutions.apply(&text))?,
                // Not text: copied as is.
                Err(_) => {
                    std::fs::copy(entry.path(), &target)?;
                }
            }
            std::fs::set_permissions(&target, entry.metadata()?.permissions())?;
        }
    }
    Ok(())
}

/// `git init`, a local identity, and one commit, under the test environment so no host git
/// configuration applies.
fn init_repository(workspace: &Path, vars: &BTreeMap<String, String>) -> Result<()> {
    let run = |args: &[&str]| -> Result<()> {
        let status = Command::new("git")
            .env_clear()
            .envs(vars)
            .current_dir(workspace)
            .args(args)
            .stdout(std::process::Stdio::null())
            .status()
            .context("running git")?;
        if !status.success() {
            bail!("git {args:?} failed in {}", workspace.display());
        }
        Ok(())
    };
    run(&["init", "-q", "-b", "main"])?;
    run(&["config", "user.name", "test"])?;
    run(&["config", "user.email", "test@example.com"])?;
    run(&["add", "-A"])?;
    run(&["commit", "-q", "--allow-empty", "-m", "initial"])
}
