//! Remote (streamable-HTTP) MCP with OAuth: a persisted token from a previous run is loaded and
//! sent without any browser flow or `mcp.auth_required` event.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "../../harness-mcp/tests/fixture/http_fixture.rs"]
mod http_fixture;

use common::{TestHarness, sse_text, sse_tool_calls};
use harness_core::{EventKind, Provider};
use oauth2::basic::BasicTokenType;
use oauth2::{AccessToken, StandardTokenResponse};
use rmcp::transport::auth::{StoredCredentials, VendorExtraTokenFields};

fn server_toml(url: &str) -> String {
    format!("[mcp.remote]\ntype = \"remote\"\nurl = {url:?}\noauth = true\npreload = true\n")
}

#[tokio::test]
async fn a_persisted_oauth_token_is_reused_with_no_auth_event() {
    let fixture = http_fixture::spawn().await;
    let toml = server_toml(&fixture.url);
    let t = TestHarness::open_layered(Provider::DeepSeek, None, Some(&toml)).await;

    // Seed a valid token, as a previous run's completed login would have left one.
    let token = StandardTokenResponse::new(
        AccessToken::new("persisted-access-token".to_string()),
        BasicTokenType::Bearer,
        VendorExtraTokenFields::default(),
    );
    let stored = StoredCredentials::new("test-client-id".to_string(), Some(token), vec![], Some(0));
    t.reopen_store()
        .set_mcp_oauth_token("remote", &serde_json::to_string(&stored).unwrap())
        .unwrap();

    t.provider
        .push_sse(sse_tool_calls(&[("call_1", "echo", r#"{"text":"hi"}"#)]));
    t.provider.push_sse(sse_text("done"));
    let (_, events) = t.run_turn("use the remote echo tool").await;

    assert!(
        !events
            .iter()
            .any(|e| matches!(e.kind, EventKind::McpAuthRequired { .. })),
        "unexpected mcp.auth_required: {events:?}"
    );
    let response = events.iter().find_map(|e| match &e.kind {
        EventKind::ToolResponse { content, .. } => Some(content.clone()),
        _ => None,
    });
    assert_eq!(response.as_deref(), Some("echo: hi"));

    let seen = fixture.seen_authorization.lock().unwrap();
    assert!(
        seen.iter()
            .any(|h| h.as_deref() == Some("Bearer persisted-access-token")),
        "{seen:?}"
    );
}
