-- Small key/value settings, e.g. the last provider chosen in the TUI popup.
-- Full session/thread persistence (08) adds its own tables alongside this one.
CREATE TABLE setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
