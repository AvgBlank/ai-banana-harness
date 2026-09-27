-- One remote MCP server's OAuth credentials, as rmcp's own `StoredCredentials` JSON so a
-- refreshed or re-issued token round-trips without a schema change here.
CREATE TABLE mcp_oauth_token (
    server            TEXT PRIMARY KEY,
    credentials_json  TEXT NOT NULL,
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
