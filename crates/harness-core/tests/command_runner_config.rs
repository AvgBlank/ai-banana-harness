//! Where `bash` runs is chosen only by the embedded default and the user's config: the host
//! unless a container is asked for, validated on load, and never by a workspace config.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use common::TestHarness;
use harness_core::{CommandRunnerConfig, Config, ConfigPaths, ContainerNetwork, Provider};
use harness_testkit::{FakeDocker, fixture};
use serde_json::json;

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn load_user(text: &str) -> anyhow::Result<Config> {
    let dir = tempfile::tempdir().unwrap();
    let user = write(dir.path(), "user.toml", text);
    Config::load(ConfigPaths {
        user: Some(&user),
        workspace: None,
    })
}

fn container(config: &Config) -> &harness_core::ContainerConfig {
    match &config.command_runner {
        CommandRunnerConfig::Container(container) => container,
        CommandRunnerConfig::Host => panic!("expected a container runner"),
    }
}

#[test]
fn the_host_runner_is_the_default() {
    assert!(matches!(
        Config::embedded_default().unwrap().command_runner,
        CommandRunnerConfig::Host
    ));
    assert!(matches!(
        load_user("[tools]\nbash_timeout_secs = 5\n")
            .unwrap()
            .command_runner,
        CommandRunnerConfig::Host
    ));
}

#[test]
fn a_container_is_chosen_only_when_asked_for_with_an_image() {
    let config = load_user(
        "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"dev:1\"\n",
    )
    .unwrap();
    let container = container(&config);
    assert_eq!(container.image, "dev:1");
    assert_eq!(container.network, ContainerNetwork::None);
    assert_eq!(container.memory, "2g");
}

#[test]
fn every_container_setting_maps_to_the_runner() {
    let config = load_user(
        "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"dev:1\"\n\
         network = \"bridge\"\nmemory = \"512m\"\npids_limit = 64\ntmp_size = \"64m\"\n",
    )
    .unwrap();
    let container = container(&config);
    assert_eq!(container.network, ContainerNetwork::Bridge);
    assert_eq!(container.memory, "512m");
    assert_eq!(container.pids_limit, 64);
    assert_eq!(container.tmp_size, "64m");
}

#[test]
fn a_host_runner_ignores_a_leftover_container_table() {
    let config =
        load_user("[tools.bash]\nrunner = \"host\"\n[tools.bash.container]\nimage = \"dev:1\"\n")
            .unwrap();
    assert!(matches!(config.command_runner, CommandRunnerConfig::Host));
}

#[test]
fn invalid_values_are_rejected_with_a_message_that_names_the_fix() {
    for (text, expected) in [
        (
            "[tools.bash]\nrunner = \"container\"\n",
            "[tools.bash.container]",
        ),
        (
            "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \" \"\n",
            "image",
        ),
        (
            "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"a\"\nnetwork = \"host\"\n",
            "none",
        ),
        (
            "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"a\"\nmemory = \"lots\"\n",
            "memory",
        ),
        (
            "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"a\"\npids_limit = 0\n",
            "pids_limit",
        ),
        ("[tools.bash]\nrunner = \"vm\"\n", "vm"),
        ("[tools.bash]\nsurprise = 1\n", "surprise"),
    ] {
        let error = format!("{:#}", load_user(text).unwrap_err());
        assert!(error.contains(expected), "{text}: {error}");
    }
}

#[test]
fn a_workspace_config_cannot_choose_the_runner() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = write(
        dir.path(),
        "workspace.toml",
        "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"evil:1\"\n\
         network = \"bridge\"\n",
    );
    let config = Config::load(ConfigPaths {
        user: None,
        workspace: Some(&workspace),
    })
    .unwrap();
    assert!(matches!(config.command_runner, CommandRunnerConfig::Host));
}

#[test]
fn a_workspace_config_cannot_loosen_what_the_user_chose() {
    let dir = tempfile::tempdir().unwrap();
    let user = write(
        dir.path(),
        "user.toml",
        "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"dev:1\"\n",
    );
    let workspace = write(
        dir.path(),
        "workspace.toml",
        "[tools.bash]\nrunner = \"host\"\n[tools.bash.container]\nnetwork = \"bridge\"\n",
    );
    let config = Config::load(ConfigPaths {
        user: Some(&user),
        workspace: Some(&workspace),
    })
    .unwrap();
    let container = container(&config);
    assert_eq!(container.image, "dev:1");
    assert_eq!(container.network, ContainerNetwork::None);
}

#[tokio::test]
async fn a_user_container_choice_runs_bash_in_the_container_and_a_workspace_one_does_not() {
    let docker = FakeDocker::new();
    docker.add_image("dev:1");
    let section = |docker: &FakeDocker| {
        format!(
            "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"dev:1\"\n\
             docker = {:?}\n",
            docker.executable()
        )
    };
    let call = |command: &str| {
        let args = serde_json::to_string(&json!({"command": command}).to_string()).unwrap();
        fixture("deepseek_tool_call.sse")
            .replace("__NAME__", "bash")
            .replace("__ARGS__", args.trim_matches('"'))
    };

    let user = TestHarness::open_layered(Provider::DeepSeek, Some(&section(&docker)), None).await;
    user.provider.push_sse(call("echo hi"));
    user.provider.push_sse(fixture("deepseek_plain.sse"));
    user.run_turn("go").await;
    assert!(!docker.calls_of("create").is_empty());

    let calls_before = docker.calls().len();
    let workspace =
        TestHarness::open_layered(Provider::DeepSeek, None, Some(&section(&docker))).await;
    workspace.provider.push_sse(call("echo hi"));
    workspace.provider.push_sse(fixture("deepseek_plain.sse"));
    workspace.run_turn("go").await;
    assert_eq!(docker.calls().len(), calls_before);
}

#[tokio::test]
async fn the_status_reports_the_host_a_working_container_and_each_failure() {
    let host = TestHarness::open(Provider::DeepSeek).await;
    let status = host.harness.command_runner_status();
    assert!(status.container.is_none() && status.problem.is_none());

    let docker = FakeDocker::new();
    let mut config = harness_core::ContainerConfig::new("dev:1");
    config.docker = docker.executable().to_path_buf();

    let missing = TestHarness::open_container(Provider::DeepSeek, config.clone()).await;
    let status = missing.harness.command_runner_status();
    assert_eq!(status.container.unwrap().image, "dev:1");
    assert!(status.problem.unwrap().contains("docker pull dev:1"));

    docker.add_image("dev:1");
    let ready = TestHarness::open_container(Provider::DeepSeek, config.clone()).await;
    assert!(ready.harness.command_runner_status().problem.is_none());

    docker.set_flag("daemon_down");
    let down = TestHarness::open_container(Provider::DeepSeek, config).await;
    let problem = down.harness.command_runner_status().problem.unwrap();
    assert!(problem.to_lowercase().contains("daemon"), "{problem}");
}
