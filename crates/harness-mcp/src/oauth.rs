//! Remote MCP over streamable HTTP, including the OAuth 2.1 login flow (dynamic client
//! registration, PKCE and a one-shot localhost callback listener). Kept separate from `lib.rs`
//! because the stdio transport never touches any of this.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use rmcp::service::RunningService;
use rmcp::transport::auth::{
    AuthClient, AuthError, AuthorizationManager, CredentialStore, InMemoryStateStore,
    StoredCredentials,
};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{RoleClient, ServiceExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Persists one server's OAuth credentials as an opaque JSON blob. Implemented by harness-core
/// against `harness-store` so this crate never depends on it.
pub trait OAuthTokenStore: Send + Sync {
    fn load(&self, server: &str) -> Result<Option<String>>;
    fn save(&self, server: &str, credentials_json: &str) -> Result<()>;
    fn clear(&self, server: &str) -> Result<()>;
}

/// What a caller must supply to connect to an `oauth = true` remote server.
pub struct OAuthContext<'a> {
    pub server_name: &'a str,
    pub token_store: Arc<dyn OAuthTokenStore>,
    /// Called with the login URL only when a fresh browser flow is actually needed; a
    /// persisted, still-valid token skips this (and the browser) entirely.
    pub on_auth_url: Box<dyn Fn(&str) + Send + Sync + 'a>,
}

struct StoreAdapter {
    server: String,
    inner: Arc<dyn OAuthTokenStore>,
}

#[async_trait::async_trait]
impl CredentialStore for StoreAdapter {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let json = self
            .inner
            .load(&self.server)
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))?;
        json.map(|s| {
            serde_json::from_str(&s).map_err(|e| AuthError::CredentialStoreError(e.to_string()))
        })
        .transpose()
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let json = serde_json::to_string(&credentials)
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))?;
        self.inner
            .save(&self.server, &json)
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))
    }

    async fn clear(&self) -> Result<(), AuthError> {
        self.inner
            .clear(&self.server)
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))
    }
}

/// Connects to a remote MCP server at `url`, running the OAuth login flow only when no valid
/// stored token covers it. `static_headers` (already `{env:VAR}`-expanded) are sent on every
/// request alongside the OAuth bearer token.
pub async fn connect(
    url: &str,
    static_headers: HashMap<http::HeaderName, http::HeaderValue>,
    ctx: OAuthContext<'_>,
) -> Result<RunningService<RoleClient, ()>> {
    let mut manager = AuthorizationManager::new(url)
        .await
        .map_err(|e| anyhow!("setting up OAuth for {url}: {e}"))?;
    manager.set_credential_store(StoreAdapter {
        server: ctx.server_name.to_string(),
        inner: ctx.token_store.clone(),
    });
    manager.set_state_store(InMemoryStateStore::new());

    let already_authorized = manager.initialize_from_store().await.map_err(|e| {
        anyhow!(
            "loading the stored OAuth token for {}: {e}",
            ctx.server_name
        )
    })?;
    if !already_authorized {
        log_in(&mut manager, &ctx).await?;
    }

    let auth_client = AuthClient::new(reqwest::Client::new(), manager);
    let config = StreamableHttpClientTransportConfig::with_uri(url.to_string())
        .custom_headers(static_headers);
    let transport = StreamableHttpClientTransport::with_client(auth_client, config);
    ().serve(transport).await.context("MCP handshake failed")
}

/// Runs one authorization-code + PKCE round trip: discover metadata, register a client,
/// open the browser on the authorization URL, wait for the localhost callback, then exchange
/// the code for a token (which `AuthorizationManager` persists via the credential store).
async fn log_in(manager: &mut AuthorizationManager, ctx: &OAuthContext<'_>) -> Result<()> {
    let resolution = manager
        .resolve_metadata()
        .await
        .map_err(|e| anyhow!("discovering OAuth metadata for {}: {e}", ctx.server_name))?;
    manager.set_metadata(resolution.metadata);

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("binding the OAuth callback listener")?;
    let port = listener
        .local_addr()
        .context("reading the OAuth callback listener's port")?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let client_config = manager
        .register_client("Banana Harness", &redirect_uri, &[])
        .await
        .map_err(|e| anyhow!("registering an OAuth client with {}: {e}", ctx.server_name))?;
    manager
        .configure_client(client_config)
        .map_err(|e| anyhow!("configuring the OAuth client for {}: {e}", ctx.server_name))?;

    let auth_url = manager.get_authorization_url(&[]).await.map_err(|e| {
        anyhow!(
            "building the OAuth authorization URL for {}: {e}",
            ctx.server_name
        )
    })?;
    (ctx.on_auth_url)(&auth_url);
    let _ = open::that(&auth_url); // best-effort: the URL is shown either way

    let (code, csrf_token) = accept_callback(listener).await?;
    manager
        .exchange_code_for_token(&code, &csrf_token)
        .await
        .map_err(|e| anyhow!("exchanging the OAuth code for {}: {e}", ctx.server_name))?;
    Ok(())
}

/// Accepts exactly one HTTP request (the OAuth redirect), pulls `code`/`state` off its query
/// string, and answers with a page telling the user it's done. No server framework: this is one
/// GET, once, then the listener is dropped.
async fn accept_callback(listener: TcpListener) -> Result<(String, String)> {
    let (mut socket, _) = listener
        .accept()
        .await
        .context("accepting the OAuth callback connection")?;
    let mut buf = vec![0u8; 8192];
    let n = socket
        .read(&mut buf)
        .await
        .context("reading the OAuth callback request")?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| anyhow!("malformed OAuth callback request"))?;
    let query = path.split_once('?').map(|(_, q)| q).unwrap_or_default();

    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "code" => code = Some(percent_decode(value)),
            "state" => state = Some(percent_decode(value)),
            _ => {}
        }
    }

    let body = "<html><body>Signed in. You can close this tab.</body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = socket.write_all(response.as_bytes()).await;

    let code = code.ok_or_else(|| anyhow!("OAuth callback had no `code` parameter"))?;
    let state = state.ok_or_else(|| anyhow!("OAuth callback had no `state` parameter"))?;
    Ok((code, state))
}

/// A minimal `application/x-www-form-urlencoded` value decoder: `+` to space, `%XX` to that
/// byte, reassembled as UTF-8 at the end so a multi-byte encoded character round-trips.
fn percent_decode(value: &str) -> String {
    let mut out = Vec::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'+' => out.push(b' '),
            b'%' => {
                let hex: String = bytes.by_ref().take(2).map(|b| b as char).collect();
                match u8::from_str_radix(&hex, 16) {
                    Ok(byte) => out.push(byte),
                    Err(_) => {
                        out.push(b'%');
                        out.extend(hex.bytes());
                    }
                }
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn percent_decode_handles_plus_and_escapes() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        assert_eq!(percent_decode("100%25"), "100%");
        assert_eq!(percent_decode("no-escapes"), "no-escapes");
    }
}
