#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::Path;

use harness_testkit::{FakeProvider, Scenario, Step, TestEnv, TestEnvOptions};

const EXPECTED: [&str; 10] = [
    "bash-and-test",
    "edit-and-approve",
    "edit-and-undo",
    "key-never-leaks",
    "plain-answer",
    "provider-abort-midstream",
    "provider-error-401",
    "question-and-answer",
    "slow-stream-gated",
    "subagents",
];

fn all() -> Vec<(String, Scenario)> {
    Scenario::bundled_paths()
        .unwrap()
        .into_iter()
        .map(|path| {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            (stem, Scenario::load(&path).unwrap())
        })
        .collect()
}

#[test]
fn every_bundled_scenario_parses_and_is_named_after_its_file() {
    let scenarios = all();
    let names: Vec<&str> = scenarios.iter().map(|(stem, _)| stem.as_str()).collect();
    assert_eq!(names, EXPECTED);
    for (stem, scenario) in &scenarios {
        assert_eq!(&scenario.name, stem);
        assert!(!scenario.steps.is_empty(), "{stem} has no steps");
        assert!(
            scenario.steps.iter().any(|s| matches!(s, Step::Send(_))),
            "{stem} never sends"
        );
    }
}

#[test]
fn a_malformed_scenario_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    for (file, text) in [
        (
            "unknown-field.json",
            r#"{"name":"x","workspace":"w","replies":[],"steps":[],"surprise":1}"#,
        ),
        (
            "unknown-step.json",
            r#"{"name":"x","workspace":"w","replies":[],"steps":[{"teleport":1}]}"#,
        ),
        ("not-json.json", "{"),
    ] {
        let path = dir.path().join(file);
        std::fs::write(&path, text).unwrap();
        assert!(Scenario::load(&path).is_err(), "{file} was accepted");
    }
}

#[tokio::test]
async fn every_scenario_scripts_a_stub_and_builds_its_workspace() {
    for (stem, scenario) in all() {
        let stub = FakeProvider::start().await;
        scenario.script(&stub).unwrap();
        assert_eq!(stub.status().queued, scenario.replies.len(), "{stem}");

        let template = scenario.workspace_template();
        assert!(
            template.is_dir(),
            "{stem}: missing workspace {}",
            template.display()
        );
        let env = TestEnv::new(&TestEnvOptions {
            workspace_template: Some(&template),
            stubs: BTreeMap::from([("baseline".to_string(), stub.base_url.clone())]),
        })
        .unwrap();
        assert!(env.workspace().join("hello.txt").is_file());

        // File expectations name paths a workspace-relative driver can read.
        for path in scenario.expect.files.keys() {
            assert!(!Path::new(path).is_absolute(), "{stem}: {path}");
        }
        if let Some(requests) = scenario.expect.requests {
            for contains in &scenario.expect.request_contains {
                assert!(
                    contains.index < requests,
                    "{stem}: request_contains index out of range"
                );
            }
        }
    }
}
