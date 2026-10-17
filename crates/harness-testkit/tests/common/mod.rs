//! A tiny raw HTTP/1.1 client, so the stub is tested without a client library.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Response {
    pub status: u16,
    pub content_length: usize,
    pub body: String,
}

fn addr(base_url: &str) -> String {
    base_url.trim_start_matches("http://").to_string()
}

/// Sends a request and returns the connection with the response head already read, plus any
/// body bytes that arrived with it.
pub async fn open(
    base_url: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (TcpStream, Response) {
    let mut stream = TcpStream::connect(addr(base_url)).await.unwrap();
    let mut request = format!("{method} {path} HTTP/1.1\r\nhost: x\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!("content-length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    let mut chunk = [0_u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut chunk).await.unwrap();
        assert!(n > 0, "connection closed before the response head");
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let content_length = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap_or(0);
    let body = String::from_utf8_lossy(&buf[head_end..]).into_owned();
    (
        stream,
        Response {
            status,
            content_length,
            body,
        },
    )
}

/// Reads until the server closes the connection.
pub async fn read_to_end(stream: &mut TcpStream, response: &mut Response) {
    let mut rest = String::new();
    stream.read_to_string(&mut rest).await.unwrap();
    response.body.push_str(&rest);
}

pub async fn request(
    base_url: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Response {
    let (mut stream, mut response) = open(base_url, method, path, headers, body).await;
    read_to_end(&mut stream, &mut response).await;
    response
}

/// `POST /chat/completions` with the fixed test key.
pub async fn chat(base_url: &str, body: &str) -> Response {
    request(
        base_url,
        "POST",
        "/chat/completions",
        &[("authorization", "Bearer sk-test-0123456789abcdef")],
        body,
    )
    .await
}
