CREATE TABLE session (
    id         TEXT PRIMARY KEY,
    title      TEXT,
    workspace  TEXT NOT NULL,
    provider   TEXT NOT NULL,
    model      TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE turn (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES session (id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,
    input       TEXT NOT NULL,
    state       TEXT NOT NULL,
    error       TEXT,
    usage_json  TEXT,
    started_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    finished_at TEXT,
    UNIQUE (session_id, seq)
);

-- Durable events only; streaming deltas are never stored.
CREATE TABLE event (
    id           TEXT PRIMARY KEY,
    turn_id      TEXT NOT NULL REFERENCES turn (id) ON DELETE CASCADE,
    thread_id    TEXT,
    type         TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX event_turn ON event (turn_id);
