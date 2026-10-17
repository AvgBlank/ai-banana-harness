#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::Path;

use harness_testkit::{KeySetup, TEST_API_KEY, TestEnv, TestEnvOptions};

fn plain() -> TestEnv {
    TestEnv::new(&TestEnvOptions::default()).unwrap()
}

fn git_output(env: &TestEnv, args: &[&str]) -> String {
    let output = env.command("git").args(args).output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn the_environment_is_exactly_the_recipe() {
    let env = plain();
    let allowed_passthrough = ["PATH", "DISPLAY"];
    let recipe = [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
        "NO_PROXY",
        "no_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "AI_API_KEY",
        "WORKSPACE",
    ];
    for name in env.vars().keys() {
        assert!(
            recipe.contains(&name.as_str()) || allowed_passthrough.contains(&name.as_str()),
            "unexpected variable {name}"
        );
    }
    for name in recipe {
        assert!(env.vars().contains_key(name), "missing {name}");
    }
    let vars = env.vars();
    assert_eq!(vars["NO_PROXY"], "127.0.0.1,localhost");
    assert_eq!(vars["no_proxy"], "127.0.0.1,localhost");
    assert_eq!(vars["HTTP_PROXY"], "http://127.0.0.1:9");
    assert_eq!(vars["AI_API_KEY"], TEST_API_KEY);
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
        assert!(
            Path::new(&vars[name]).starts_with(env.root()),
            "{name} escapes the root"
        );
    }
    assert_eq!(Path::new(&vars["WORKSPACE"]), env.workspace());
}

#[test]
fn a_child_sees_only_the_recipe() {
    let env = plain();
    let output = env.command("env").output().unwrap();
    let seen: BTreeMap<String, String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|l| {
            l.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();
    // `env` itself may add `_` or `PWD`/`SHLVL`; everything else must be ours.
    for (name, value) in &seen {
        if matches!(name.as_str(), "_" | "PWD" | "OLDPWD" | "SHLVL") {
            continue;
        }
        assert_eq!(env.vars().get(name), Some(value), "foreign variable {name}");
    }
    assert_eq!(seen["AI_API_KEY"], TEST_API_KEY);
}

#[test]
fn the_workspace_is_a_canonical_repository_with_one_commit() {
    let env = plain();
    assert_eq!(env.workspace(), env.workspace().canonicalize().unwrap());
    assert_eq!(git_output(&env, &["rev-list", "--count", "HEAD"]), "1");
    assert_eq!(
        git_output(&env, &["config", "--local", "user.email"]),
        "test@example.com"
    );
    assert_eq!(git_output(&env, &["status", "--porcelain"]), "");
}

#[test]
fn a_template_is_copied_and_its_placeholders_filled() {
    let template = tempfile::tempdir().unwrap();
    std::fs::create_dir(template.path().join("sub")).unwrap();
    std::fs::write(
        template.path().join("sub/notes.txt"),
        "stub={{stub:baseline}} evil={{stub:evil}} ws={{workspace}} root={{root}} marker={{marker}}",
    )
    .unwrap();
    let stubs = BTreeMap::from([
        ("baseline".to_string(), "http://127.0.0.1:1111".to_string()),
        ("evil".to_string(), "http://127.0.0.1:2222".to_string()),
    ]);
    let env = TestEnv::new(&TestEnvOptions {
        workspace_template: Some(template.path()),
        stubs,
    })
    .unwrap();
    let notes = std::fs::read_to_string(env.workspace().join("sub/notes.txt")).unwrap();
    assert_eq!(
        notes,
        format!(
            "stub=http://127.0.0.1:1111 evil=http://127.0.0.1:2222 ws={} root={} marker={}",
            env.workspace().display(),
            env.root().display(),
            env.marker().display()
        )
    );
    // The template is committed, so the workspace starts clean.
    assert_eq!(git_output(&env, &["status", "--porcelain"]), "");
    assert!(!env.marker().exists());
}

#[test]
fn the_user_config_points_both_providers_at_the_baseline_stub() {
    let env = TestEnv::new(&TestEnvOptions {
        workspace_template: None,
        stubs: BTreeMap::from([
            ("evil".to_string(), "http://127.0.0.1:2222".to_string()),
            ("baseline".to_string(), "http://127.0.0.1:1111".to_string()),
        ]),
    })
    .unwrap();
    let path = Path::new(&env.vars()["XDG_CONFIG_HOME"]).join("banana-harness/config.toml");
    let config: toml_like::Config = toml_like::parse(&std::fs::read_to_string(path).unwrap());
    assert_eq!(config.deepseek, "http://127.0.0.1:1111");
    assert_eq!(config.qwen, "http://127.0.0.1:1111");
}

#[test]
fn key_variants() {
    assert!(!plain().without_key().vars().contains_key("AI_API_KEY"));
    let shell = plain().shell(Path::new("/tmp/login-shell.sh"));
    assert!(!shell.vars().contains_key("AI_API_KEY"));
    assert_eq!(shell.vars()["SHELL"], "/tmp/login-shell.sh");
    assert_eq!(
        plain().with_key(KeySetup::Fixed).vars()["AI_API_KEY"],
        TEST_API_KEY
    );
    assert!(
        !plain()
            .with_key(KeySetup::Missing)
            .vars()
            .contains_key("AI_API_KEY")
    );
}

#[test]
fn dropping_removes_the_root_and_keep_retains_it() {
    let env = plain();
    let root = env.root().to_path_buf();
    assert!(root.exists());
    drop(env);
    assert!(!root.exists(), "the root survived cleanup");

    let kept = plain().keep();
    assert!(kept.exists());
    std::fs::remove_dir_all(&kept).unwrap();
}

/// Just enough TOML reading for one file the testkit wrote itself.
mod toml_like {
    pub struct Config {
        pub deepseek: String,
        pub qwen: String,
    }

    pub fn parse(text: &str) -> Config {
        let value = |section: &str| {
            let after = text
                .split(&format!("[providers.{section}]"))
                .nth(1)
                .unwrap();
            let line = after.lines().find(|l| l.starts_with("base_url")).unwrap();
            line.split('"').nth(1).unwrap().to_string()
        };
        Config {
            deepseek: value("deepseek"),
            qwen: value("qwen"),
        }
    }
}
