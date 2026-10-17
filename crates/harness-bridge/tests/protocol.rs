#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;
use std::time::Duration;

use common::{API_KEY, Client, create_session, golden, normalise_sessions};
use serde_json::{Value, json};

fn error_code(frame: &Value) -> &str {
    assert_eq!(frame["kind"], "response");
    assert_eq!(frame["ok"], false, "{frame}");
    frame["error"]["code"].as_str().unwrap()
}

#[tokio::test]
async fn the_first_frame_is_ready() {
    let mut client = Client::start().await;
    let ready = client.next().await;
    assert_eq!(
        ready,
        json!({
            "kind": "ready", "protocol": 1, "app": "Banana Harness",
            "version": env!("CARGO_PKG_VERSION"), "api_key_present": true,
        })
    );
    golden("ready", &[ready]);

    let mut keyless = Client::start_with("  ", 1 << 16).await;
    assert_eq!(keyless.next().await["api_key_present"], false);
}

#[tokio::test]
async fn a_startup_failure_is_a_fatal_frame_then_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "a file where a directory must be").unwrap();
    let mut client = Client::start_at(dir, blocker.join("harness.db"), API_KEY, 1 << 16, None);

    let mut fatal = client.next().await;
    assert_eq!(fatal["kind"], "fatal");
    assert_eq!(fatal["error"]["code"], "startup_failed");
    assert!(client.try_next().await.is_none(), "frames followed a fatal");
    assert!(client.finish().await.is_err());

    let message = fatal["error"]["message"].as_str().unwrap();
    assert!(message.contains("blocker"));
    fatal["error"]["message"] = json!("<message naming the database path>");
    golden("fatal", &[fatal]);
}

#[tokio::test]
async fn requests_correlate_by_id() {
    let mut client = Client::start().await;
    client.next().await;
    for n in 0..50 {
        client
            .send(&format!("r{n}"), "session.list", json!({}))
            .await;
    }
    let mut seen = HashSet::new();
    for _ in 0..50 {
        let frame = client.next().await;
        assert_eq!(frame["ok"], true);
        assert!(
            seen.insert(frame["id"].as_str().unwrap().to_string()),
            "{frame}"
        );
    }
    assert_eq!(seen.len(), 50);
}

#[tokio::test]
async fn a_duplicate_in_flight_id_is_invalid_request() {
    let mut client = Client::start().await;
    client.next().await;
    let line = json!({"kind": "request", "id": "same", "method": "session.list"}).to_string();
    // One write, so the second line is read before the first request has had a chance to finish.
    client
        .send_raw(format!("{line}\n{line}\n").as_bytes())
        .await;
    let first = client.next().await;
    let second = client.next().await;
    let (ok, dup) = if first["ok"] == true {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(ok["id"], "same");
    assert_eq!(error_code(&dup), "invalid_request");
    assert_eq!(dup["id"], "same");
    // Once answered the id is free again.
    assert_eq!(
        client.call("same", "session.list", json!({})).await["ok"],
        true
    );
}

#[tokio::test]
async fn malformed_input_gets_an_error_and_the_connection_survives() {
    let mut client = Client::start().await;
    client.next().await;
    let mut responses = Vec::new();

    client.send_line("not json").await;
    responses.push(client.next().await);
    client
        .send_raw(b"{\"kind\":\"request\",\"id\":\"x\",\"method\":\"\xff\xfe\"}\n")
        .await;
    responses.push(client.next().await);
    client.send_line("[1,2]").await;
    responses.push(client.next().await);
    client.send_line("").await;
    responses.push(client.next().await);
    client
        .send_line(r#"{"kind":"request","method":"session.list"}"#)
        .await;
    responses.push(client.next().await);
    client
        .send_line(r#"{"kind":"request","id":7,"method":"session.list"}"#)
        .await;
    responses.push(client.next().await);
    client.send_line(r#"{"kind":"response","id":"w1"}"#).await;
    responses.push(client.next().await);
    client.send_line(r#"{"kind":"request","id":"m1"}"#).await;
    responses.push(client.next().await);
    client
        .send_line(r#"{"kind":"request","id":"u1","method":"file.read","params":{}}"#)
        .await;
    responses.push(client.next().await);
    client
        .send_line(r#"{"kind":"request","id":"p1","method":"session.list","params":[1]}"#)
        .await;
    responses.push(client.next().await);

    let codes: Vec<&str> = responses.iter().map(error_code).collect();
    assert_eq!(
        codes,
        [
            "invalid_request",
            "invalid_request",
            "invalid_request",
            "invalid_request",
            "invalid_request",
            "invalid_request",
            "invalid_request",
            "invalid_request",
            "unknown_method",
            "invalid_params",
        ]
    );
    let ids: Vec<&Value> = responses.iter().map(|r| &r["id"]).collect();
    assert_eq!(
        ids,
        [
            &Value::Null,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            &Value::Null,
            &json!("w1"),
            &json!("m1"),
            &json!("u1"),
            &json!("p1")
        ]
    );

    // Extra fields and params fields are ignored, and the connection still serves.
    let ok = client
        .call("fine", "session.list", json!({"unknown": true}))
        .await;
    assert_eq!(ok["ok"], true);
    golden("errors", &responses);
}

#[tokio::test]
async fn an_oversized_line_is_discarded_to_its_newline() {
    let mut client = Client::start().await;
    client.next().await;
    let big = vec![b'a'; 8 * 1024 * 1024 + 10];
    client.send_raw(&big).await;
    client.send_raw(b"\n").await;
    let error = client.next().await;
    assert_eq!(error_code(&error), "invalid_request");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("longer than"),
        "{error}"
    );
    assert_eq!(error["id"], Value::Null);
    assert_eq!(
        client.call("after", "session.list", json!({})).await["ok"],
        true
    );

    // Exactly at the limit is accepted as a line (and then rejected as not JSON, not as too long).
    let at_limit = vec![b'a'; 8 * 1024 * 1024];
    client.send_raw(&at_limit).await;
    client.send_raw(b"\n").await;
    let error = client.next().await;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not valid JSON"),
        "{error}"
    );
}

#[tokio::test]
async fn session_list_works_with_no_workspace_open_newest_first() {
    let mut client = Client::start().await;
    client.next().await;

    let empty = client.call("l0", "session.list", json!({})).await;
    assert_eq!(empty["result"], json!({"sessions": []}));

    let base = client.dir.path().to_path_buf();
    let first = create_session(&client.db_path, &base, "alpha").await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let second = create_session(&client.db_path, &base, "beta").await;

    client.send("l1", "session.list", json!({})).await;
    let mut response = client.next().await;
    let ids: Vec<&str> = response["result"]["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [second.as_str(), first.as_str()]);
    assert!(
        response["result"]["sessions"][0]["workspace"]
            .as_str()
            .unwrap()
            .ends_with("beta")
    );

    normalise_sessions(&mut response, &base);
    let request = json!({"kind": "request", "id": "l1", "method": "session.list", "params": {}});
    golden("session-list", &[request, response]);
}

#[tokio::test]
async fn shutdown_replies_then_the_server_returns() {
    let mut client = Client::start().await;
    client.next().await;
    let request = json!({"kind": "request", "id": "bye", "method": "shutdown", "params": {}});
    client.send_line(&request.to_string()).await;
    let response = client.next().await;
    assert_eq!(
        response,
        json!({"kind": "response", "id": "bye", "ok": true, "result": {}})
    );
    assert!(client.try_next().await.is_none());
    client.finish().await.unwrap();
    golden("shutdown", &[request, response]);
}

#[tokio::test]
async fn end_of_input_ends_the_server_cleanly() {
    let mut client = Client::start().await;
    client.next().await;
    // An unterminated final line still counts as a request.
    client
        .send_raw(br#"{"kind":"request","id":"last","method":"session.list"}"#)
        .await;
    client.close_input().await;
    assert_eq!(client.next().await["id"], "last");
    assert!(client.try_next().await.is_none());
    client.finish().await.unwrap();
}

#[tokio::test]
async fn a_client_that_stops_reading_delays_frames_without_losing_any() {
    // A tiny pipe: the writer blocks after a few frames, the queue fills behind it.
    let mut client = Client::start_with(API_KEY, 256).await;
    let base = client.dir.path().to_path_buf();
    for n in 0..5 {
        create_session(&client.db_path, &base, &format!("w{n}")).await;
    }
    for n in 0..300 {
        client
            .send(&format!("b{n}"), "session.list", json!({}))
            .await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !client.server.is_finished(),
        "the server gave up on a slow reader"
    );

    let mut seen = HashSet::new();
    assert_eq!(client.next().await["kind"], "ready");
    for _ in 0..300 {
        let frame = client.next().await;
        assert_eq!(frame["result"]["sessions"].as_array().unwrap().len(), 5);
        assert!(seen.insert(frame["id"].as_str().unwrap().to_string()));
    }
    assert_eq!(seen.len(), 300);
}

#[tokio::test]
async fn a_client_that_hangs_up_ends_the_server() {
    let mut client = Client::start().await;
    client.next().await;
    client.hang_up();
    // Provoke a write to the closed pipe.
    client.send("x", "session.list", json!({})).await;
    client.finish().await.unwrap();
}

#[tokio::test]
async fn the_key_is_redacted_from_error_messages_and_absent_from_frames() {
    let mut client = Client::start().await;
    let mut frames = vec![client.next().await];
    frames.push(
        client
            .call("k1", &format!("echo-{API_KEY}"), json!({}))
            .await,
    );
    frames.push(client.call("k2", "session.list", json!({})).await);
    let all = serde_json::to_string(&frames).unwrap();
    assert!(!all.contains(API_KEY), "the key reached a frame: {all}");
    assert!(
        frames[1]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("[REDACTED]")
    );
}
