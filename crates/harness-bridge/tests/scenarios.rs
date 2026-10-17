//! Every shared scenario file, run in-process against the bridge (the real process runs the same
//! files in the binary crate's `serve_stdio` test).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{API_KEY, Run};
use harness_testkit::{Scenario, Step, run_scenario};
use serde_json::Value;

#[tokio::test]
async fn every_scenario_runs_in_process_and_the_key_stays_out_of_every_frame() {
    let mut skipped = Vec::new();
    for path in Scenario::bundled_paths().unwrap() {
        let scenario = Scenario::load(&path).unwrap();
        // Killing the sidecar needs a process to kill.
        if scenario
            .steps
            .iter()
            .any(|s| matches!(s, Step::KillSidecar(_)))
        {
            skipped.push(scenario.name);
            continue;
        }
        let (mut run, _) = Run::for_scenario(&scenario.name).await;
        let workspace = run.env.workspace().to_path_buf();
        run_scenario(&mut run.client, &scenario, &run.stub, &workspace)
            .await
            .unwrap_or_else(|e| panic!("scenario {}: {e:#}", scenario.name));

        let received: Vec<&Value> = run
            .client
            .log
            .iter()
            .filter(|f| f["kind"] != "request")
            .collect();
        assert!(
            !serde_json::to_string(&received).unwrap().contains(API_KEY),
            "scenario {}: the key reached a frame",
            scenario.name
        );
        // A scenario leaves nothing running: input closes and the server returns.
        run.client.close_input().await;
        while run.client.try_next().await.is_some() {}
        run.client.finish().await.unwrap();
    }
    assert_eq!(
        skipped,
        ["kill-mid-turn"],
        "only the process-only scenario may be skipped"
    );
}
