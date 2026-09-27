-- A turn's pre-mutation git snapshot, taken before its first mutating tool call. `stash_ref` is
-- NULL when the worktree matched `head_sha` exactly, so there was nothing to stash.
CREATE TABLE checkpoint (
    turn_id        TEXT PRIMARY KEY REFERENCES turn (id) ON DELETE CASCADE,
    session_id     TEXT NOT NULL REFERENCES session (id) ON DELETE CASCADE,
    head_sha       TEXT NOT NULL,
    stash_ref      TEXT,
    untracked_json TEXT NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
