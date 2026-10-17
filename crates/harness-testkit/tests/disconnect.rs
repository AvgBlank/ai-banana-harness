//! A client that hangs up mid-reply must not panic the fake provider's server task.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use harness_testkit::{FakeProvider, sse_text};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

static PANICKED: AtomicBool = AtomicBool::new(false);

#[tokio::test]
async fn client_hanging_up_mid_reply_is_not_a_panic() {
    std::panic::set_hook(Box::new(|_| PANICKED.store(true, Ordering::SeqCst)));
    let provider = FakeProvider::start().await;
    provider.push_sse_slow(sse_text("hello"), Duration::from_millis(100));

    let addr = provider.base_url.trim_start_matches("http://").to_string();
    let mut client = TcpStream::connect(addr).await.unwrap();
    let body = "{}";
    let request = format!(
        "POST /chat HTTP/1.1\r\nhost: x\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    client.write_all(request.as_bytes()).await.unwrap();
    let mut head = [0_u8; 16];
    client.read_exact(&mut head).await.unwrap();
    drop(client);

    // Long enough for the server to try writing the next event to the closed socket.
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!PANICKED.load(Ordering::SeqCst), "the server task panicked");
    assert_eq!(provider.requests().len(), 1);
    assert_eq!(provider.max_in_flight(), 1);
}
