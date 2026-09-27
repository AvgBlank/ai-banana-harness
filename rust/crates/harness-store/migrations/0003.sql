-- The session's thread context: upserted after every LLM or tool step so a session can
-- continue (in this process or a later one) from its last snapshot. `parent_id IS NULL`
-- identifies the session's root thread; subagent threads (added later) will set it.
CREATE TABLE thread (
    id                 TEXT PRIMARY KEY,
    session_id         TEXT NOT NULL REFERENCES session (id) ON DELETE CASCADE,
    parent_id          TEXT REFERENCES thread (id) ON DELETE CASCADE,
    context_json       TEXT NOT NULL,
    context_usage_json TEXT,
    updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX thread_session ON thread (session_id);
