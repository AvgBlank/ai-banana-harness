//! Shared seam-1 test support: a harness wired to the testkit's fake provider.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::Duration;

use harness_core::{
    Config, ConfigPaths, Event, EventKind, Harness, HarnessOptions, Provider, Store,
};
use harness_testkit::{FakeProvider, TEST_API_KEY};
use tempfile::TempDir;

/// A harness whose provider base URL points at a [`FakeProvider`], with a temp workspace and database.
pub struct TestHarness {
    pub harness: Harness,
    pub provider: FakeProvider,
    pub db_path: PathBuf,
    pub workspace: PathBuf,
    _dir: TempDir,
}

impl TestHarness {
    pub async fn open(provider: Provider) -> Self {
        Self::open_configured(provider, |_| {}).await
    }

    /// Like [`open`](Self::open), but a headless run: questions are auto-answered and `ask`
    /// rules resolve to deny, both without waiting.
    pub async fn open_headless(provider: Provider) -> Self {
        let mut config = Config::embedded_default().unwrap();
        config.skills.paths.clear();
        Self::open_with(provider, config, true).await
    }

    /// Like [`open`](Self::open), but lets the test adjust the embedded default config (e.g.
    /// `skills.paths`) before the harness is built.
    pub async fn open_configured(provider: Provider, configure: impl FnOnce(&mut Config)) -> Self {
        let mut config = Config::embedded_default().unwrap();
        // The committed default's skills paths include the real user's config directory;
        // tests must not depend on (or accidentally pick up) whatever is on the host machine.
        config.skills.paths.clear();
        configure(&mut config);
        Self::open_with(provider, config, false).await
    }

    /// Like [`open`](Self::open), but layers `user_toml` and `workspace_toml` through
    /// [`harness_core::Config::load`] before opening, to exercise config-layering precedence
    /// end to end (e.g. the model name that reaches the fake server's request body).
    pub async fn open_layered(
        provider: Provider,
        user_toml: Option<&str>,
        workspace_toml: Option<&str>,
    ) -> Self {
        let overlay_dir = tempfile::tempdir().unwrap();
        let user_path = user_toml.map(|text| {
            let path = overlay_dir.path().join("user-config.toml");
            std::fs::write(&path, text).unwrap();
            path
        });
        let workspace_path = workspace_toml.map(|text| {
            let path = overlay_dir.path().join("workspace-config.toml");
            std::fs::write(&path, text).unwrap();
            path
        });
        let mut config = Config::load(ConfigPaths {
            user: user_path.as_deref(),
            workspace: workspace_path.as_deref(),
        })
        .unwrap();
        config.skills.paths.clear();
        Self::open_with(provider, config, false).await
    }

    async fn open_with(provider: Provider, mut config: Config, headless: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        // `ignore::WalkBuilder` only honours `.gitignore` files inside an actual git repo
        // (`require_git` defaults to true), and the tool guardrail tests want a real workspace
        // to canonicalize paths against.
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&workspace)
                .status()
                .unwrap()
                .success(),
            "git init failed in the test workspace"
        );
        let db_path = dir.path().join("harness.db");
        let fake = FakeProvider::start().await;
        config.providers.deepseek.base_url = fake.base_url.clone();
        config.providers.qwen.base_url = fake.base_url.clone();
        let harness = Harness::open(HarnessOptions {
            config,
            store: Store::open(&db_path).unwrap(),
            provider,
            api_key: TEST_API_KEY.to_string(),
            workspace: workspace.clone(),
            headless,
        })
        .unwrap();
        Self {
            harness,
            provider: fake,
            db_path,
            workspace,
            _dir: dir,
        }
    }

    /// Runs one turn in a new session and collects its events through `turn.done`.
    pub async fn run_turn(&self, input: &str) -> (String, Vec<Event>) {
        let session_id = self.harness.create_session().unwrap();
        let events = collect(self.harness.start_turn(&session_id, input.to_string())).await;
        (session_id, events)
    }

    /// A second connection to the same database, as a later process would open it.
    pub fn reopen_store(&self) -> Store {
        Store::open(&self.db_path).unwrap()
    }

    /// A second harness against the same database and workspace, as a later process would open
    /// it, still pointed at this test's fake provider.
    pub async fn reopen(&self, provider: Provider) -> Harness {
        let mut config = Config::embedded_default().unwrap();
        config.skills.paths.clear();
        config.providers.deepseek.base_url = self.provider.base_url.clone();
        config.providers.qwen.base_url = self.provider.base_url.clone();
        Harness::open(HarnessOptions {
            config,
            store: Store::open(&self.db_path).unwrap(),
            provider,
            api_key: TEST_API_KEY.to_string(),
            workspace: self.workspace.clone(),
            headless: false,
        })
        .unwrap()
    }
}

/// Drains a turn's stream, failing the test if it doesn't end with `turn.done` within 10 s.
pub async fn collect(mut turn: harness_core::TurnHandle) -> Vec<Event> {
    let mut events = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = turn.events.recv().await {
            let done = matches!(event.kind, EventKind::TurnDone { .. });
            events.push(event);
            if done {
                break;
            }
        }
    })
    .await
    .expect("turn did not finish within 10 s");
    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(EventKind::TurnDone { .. })
        ),
        "stream ended without turn.done: {events:?}"
    );
    events
}
