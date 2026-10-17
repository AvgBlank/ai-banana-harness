#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use common::{chat, open, read_to_end, request};
use harness_testkit::{FakeProvider, ReplySpec, StubOptions, sse_text};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

fn text(content: &str) -> ReplySpec {
    ReplySpec {
        sse: Some(harness_testkit::SseSpec {
            text: Some(content.to_string()),
            tool_calls: Vec::new(),
        }),
        ..ReplySpec::default()
    }
}

#[tokio::test]
async fn sse_is_written_incrementally() {
    let stub = FakeProvider::start().await;
    stub.push_sse_slow(sse_text("hi"), Duration::from_millis(150));
    let started = Instant::now();
    let (mut stream, mut response) =
        open(&stub.base_url, "POST", "/chat/completions", &[], "{}").await;
    let mut first = [0_u8; 64];
    let n = stream.read(&mut first).await.unwrap();
    let first_part_at = started.elapsed();
    assert!(n > 0);
    read_to_end(&mut stream, &mut response).await;
    assert!(
        started.elapsed() >= first_part_at + Duration::from_millis(300),
        "later parts did not arrive after the first"
    );
    assert!(response.body.ends_with("data: [DONE]\n\n"));
}

#[tokio::test]
async fn matchers_and_queue_order_pick_the_reply() {
    let stub = FakeProvider::start().await;
    stub.push_sse_matching("needle", sse_text("for needle"), Duration::ZERO);
    stub.push_sse(sse_text("first unmatched"));
    stub.push_sse(sse_text("second unmatched"));

    let plain = chat(&stub.base_url, r#"{"q":"hay"}"#).await;
    assert!(plain.body.contains("first unmatched"), "{}", plain.body);
    let needle = chat(&stub.base_url, r#"{"q":"needle"}"#).await;
    assert!(needle.body.contains("for needle"));
    let rest = chat(&stub.base_url, r#"{"q":"hay"}"#).await;
    assert!(rest.body.contains("second unmatched"));
    stub.assert_drained();
}

#[tokio::test]
async fn an_empty_queue_answers_500_and_is_recorded_as_unmatched() {
    let stub = FakeProvider::start().await;
    let response = chat(&stub.base_url, "{}").await;
    assert_eq!(response.status, 500);
    assert!(response.body.contains("no reply queued"));
    let leftover = stub.leftover();
    assert_eq!(leftover.queued, 0);
    assert_eq!(leftover.unmatched, vec!["/chat/completions".to_string()]);
    assert_eq!(stub.status().unmatched, 1);
}

#[tokio::test]
#[should_panic(expected = "fake provider leftovers")]
async fn assert_drained_fails_on_a_reply_never_served() {
    let stub = FakeProvider::start().await;
    stub.push_sse(sse_text("never asked for"));
    stub.assert_drained();
}

#[tokio::test]
async fn failure_replies_carry_their_status_and_body() {
    let stub = FakeProvider::start().await;
    stub.push_spec(&ReplySpec {
        status: Some(429),
        error: Some(r#"{"error":{"message":"slow down"}}"#.to_string()),
        ..ReplySpec::default()
    })
    .unwrap();
    let response = chat(&stub.base_url, "{}").await;
    assert_eq!(response.status, 429);
    assert!(response.body.contains("slow down"));
    assert!(
        stub.push_spec(&ReplySpec::default()).is_err(),
        "an empty spec is rejected"
    );
}

#[tokio::test]
async fn a_gate_holds_the_stream_until_released_over_the_control_api() {
    let stub = FakeProvider::start().await;
    stub.push_spec(&ReplySpec {
        gate: Some("g".to_string()),
        gate_after_parts: Some(1),
        ..text("gated")
    })
    .unwrap();
    let (mut stream, mut response) =
        open(&stub.base_url, "POST", "/chat/completions", &[], "{}").await;
    // Drain until the stream goes quiet: it must stop at the gate, short of the end.
    let mut buf = [0_u8; 4096];
    while let Ok(read) =
        tokio::time::timeout(Duration::from_millis(300), stream.read(&mut buf)).await
    {
        let n = read.unwrap();
        assert!(n > 0, "the stream ended at a closed gate");
        response.body.push_str(&String::from_utf8_lossy(&buf[..n]));
    }
    assert!(!response.body.contains("[DONE]"));
    let released = request(&stub.base_url, "POST", "/__fake/release/g", &[], "").await;
    assert_eq!(released.status, 200);
    read_to_end(&mut stream, &mut response).await;
    assert!(response.body.ends_with("data: [DONE]\n\n"));
}

#[tokio::test]
async fn a_gate_released_before_the_request_does_not_block() {
    let stub = FakeProvider::start().await;
    stub.release("early");
    stub.push_spec(&ReplySpec {
        gate: Some("early".to_string()),
        ..text("fine")
    })
    .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), chat(&stub.base_url, "{}"))
        .await
        .unwrap();
    assert!(response.body.contains("fine"));
}

#[tokio::test]
async fn abort_after_parts_cuts_the_body_short() {
    let stub = FakeProvider::start().await;
    stub.push_spec(&ReplySpec {
        abort_after_parts: Some(1),
        ..text("cut off")
    })
    .unwrap();
    let response = chat(&stub.base_url, "{}").await;
    assert!(
        response.body.len() < response.content_length,
        "{} bytes of {}",
        response.body.len(),
        response.content_length
    );
    assert!(!response.body.contains("[DONE]"));
}

#[tokio::test]
async fn the_watchdog_ends_a_hanging_reply() {
    let stub = FakeProvider::start_with(StubOptions {
        watchdog: Duration::from_millis(300),
        ..StubOptions::default()
    })
    .await;
    stub.push_spec(&ReplySpec {
        hang: true,
        ..ReplySpec::default()
    })
    .unwrap();
    let started = Instant::now();
    let (mut stream, _) = {
        // A hanging reply sends no head, so open() would wait; send by hand.
        let mut stream =
            tokio::net::TcpStream::connect(stub.base_url.trim_start_matches("http://"))
                .await
                .unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"POST /chat/completions HTTP/1.1\r\ncontent-length: 2\r\n\r\n{}",
        )
        .await
        .unwrap();
        (stream, ())
    };
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut rest))
        .await
        .expect("the watchdog did not close the connection")
        .unwrap();
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert!(rest.is_empty(), "a hung reply sent bytes");
    assert_eq!(stub.status().watchdog_aborts, 1);
}

#[tokio::test]
async fn the_watchdog_also_ends_a_stalled_gate_but_not_a_normal_reply() {
    let stub = FakeProvider::start_with(StubOptions {
        watchdog: Duration::from_millis(400),
        ..StubOptions::default()
    })
    .await;
    stub.push_sse(sse_text("quick"));
    chat(&stub.base_url, "{}").await;
    assert_eq!(stub.status().watchdog_aborts, 0);

    stub.push_spec(&ReplySpec {
        gate: Some("never".to_string()),
        ..text("stuck")
    })
    .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), chat(&stub.base_url, "{}"))
        .await
        .expect("the watchdog did not end the stalled reply");
    assert!(response.body.len() < response.content_length);
    assert_eq!(stub.status().watchdog_aborts, 1);
}

#[tokio::test]
async fn the_control_api_scripts_and_reads_the_stub() {
    let stub = FakeProvider::start().await;
    let base = &stub.base_url;

    assert_eq!(
        request(base, "GET", "/__fake/health", &[], "").await.status,
        200
    );
    assert_eq!(
        request(base, "GET", "/__fake/nope", &[], "").await.status,
        404
    );
    let bad = request(base, "POST", "/__fake/replies", &[], "not json").await;
    assert_eq!(bad.status, 400);
    let invalid = request(base, "POST", "/__fake/replies", &[], "[{}]").await;
    assert_eq!(invalid.status, 400);

    let queued = request(
        base,
        "POST",
        "/__fake/replies",
        &[],
        &json!([{"sse": {"text": "scripted"}}]).to_string(),
    )
    .await;
    assert_eq!(queued.status, 200);
    let status: Value =
        serde_json::from_str(&request(base, "GET", "/__fake/status", &[], "").await.body).unwrap();
    assert_eq!(status["queued"], 1);

    assert!(chat(base, r#"{"q":1}"#).await.body.contains("scripted"));
    let requests: Value =
        serde_json::from_str(&request(base, "GET", "/__fake/requests", &[], "").await.body)
            .unwrap();
    assert_eq!(
        requests.as_array().unwrap().len(),
        1,
        "control calls are not recorded"
    );
    assert_eq!(requests[0]["body"], json!({"q": 1}));

    request(base, "POST", "/__fake/reset", &[], "").await;
    let status: Value =
        serde_json::from_str(&request(base, "GET", "/__fake/status", &[], "").await.body).unwrap();
    assert_eq!(
        status,
        json!({"queued": 0, "consumed": 0, "unmatched": 0, "watchdog_aborts": 0})
    );
    assert!(stub.requests().is_empty());
}

#[tokio::test]
async fn the_authorization_header_is_redacted_and_checked() {
    let stub = FakeProvider::start().await;
    stub.push_sse(sse_text("a"));
    stub.push_sse(sse_text("b"));
    stub.push_sse(sse_text("c"));
    chat(&stub.base_url, "{}").await;
    request(
        &stub.base_url,
        "POST",
        "/chat/completions",
        &[("authorization", "Bearer wrong")],
        "{}",
    )
    .await;
    request(&stub.base_url, "POST", "/chat/completions", &[], "{}").await;

    let requests = stub.requests();
    assert!(requests[0].auth_ok);
    assert!(!requests[1].auth_ok);
    assert!(!requests[2].auth_ok);
    assert_eq!(requests[0].headers["authorization"], "[redacted]");
    assert_eq!(requests[1].headers["authorization"], "[redacted]");
    assert!(!requests[2].headers.contains_key("authorization"));
    let dump = serde_json::to_string(&requests).unwrap();
    assert!(
        !dump.contains("sk-test-0123456789abcdef"),
        "the key leaked into the log"
    );
    assert!(!dump.contains("wrong"));
}

#[tokio::test]
async fn two_stubs_run_independently() {
    let (a, b) = (FakeProvider::start().await, FakeProvider::start().await);
    assert_ne!(a.base_url, b.base_url);
    a.push_sse(sse_text("from a"));
    b.push_sse(sse_text("from b"));
    assert!(chat(&b.base_url, "{}").await.body.contains("from b"));
    assert!(chat(&a.base_url, "{}").await.body.contains("from a"));
    assert_eq!((a.requests().len(), b.requests().len()), (1, 1));
}

#[tokio::test]
async fn in_flight_is_released_when_the_client_hangs_up() {
    let stub = FakeProvider::start().await;
    stub.push_spec(&ReplySpec {
        delay_ms: Some(50),
        ..text("long enough")
    })
    .unwrap();
    let (stream, _) = open(&stub.base_url, "POST", "/chat/completions", &[], "{}").await;
    drop(stream);
    tokio::time::sleep(Duration::from_millis(500)).await;
    // A second request would see max_in_flight 2 if the first were still counted.
    stub.push_sse(sse_text("next"));
    chat(&stub.base_url, "{}").await;
    assert_eq!(stub.max_in_flight(), 1);
}
