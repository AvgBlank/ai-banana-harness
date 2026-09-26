#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{TestHarness, collect, fixture};
use harness_core::{EventKind, Provider, TurnState};

#[tokio::test]
async fn cancelling_a_running_turn_ends_it_as_cancelled() {
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider
        .push_sse_slow(fixture("deepseek_slow.sse"), Duration::from_millis(30));

    let session_id = t.harness.create_session().unwrap();
    let mut turn = t
        .harness
        .start_turn(&session_id, "count to five".to_string());

    // Wait for the stream to actually start before cancelling it, so the runner is inside
    // the delta-reading loop rather than still waiting on the provider's initial response.
    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), turn.events.recv())
            .await
            .expect("timed out waiting for the stream to start")
            .expect("turn ended before it started streaming");
        let started = matches!(event.kind, EventKind::ModelMessage { message: None, .. });
        events.push(event);
        if started {
            break;
        }
    }

    turn.cancel();
    events.extend(collect(turn).await);

    assert!(
        matches!(
            events.last().map(|e| &e.kind),
            Some(EventKind::TurnDone {
                state: TurnState::Cancelled,
                error: None
            })
        ),
        "expected turn.done{{state: cancelled}}, got {:?}",
        events.last()
    );

    // The stream never reached its final chunk, so it can't have persisted a "done" turn.
    let turn_row = &t.reopen_store().turns(&session_id).unwrap()[0];
    assert_eq!(turn_row.state, "cancelled");
}
