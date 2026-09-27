-- A subagent thread's title (the name its parent gave it); NULL for a session's root thread.
ALTER TABLE thread ADD COLUMN title TEXT;
