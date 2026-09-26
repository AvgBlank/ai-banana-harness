# 08: Sessions persist and resume

**What to build:** complete persistence of durable state.

- Thread context snapshots are upserted after every LLM or tool step.
- Turn state and usage are recorded.
- Settings are stored (the last provider).

**Slash commands:**
- `/new` starts a fresh session.
- `/sessions` lists earlier sessions (title, workspace, time) and resumes the chosen one from its root thread's last snapshot.
- `/provider` switches provider for the session.

Only the turn runner writes to the database.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] Seam-1 test: after one turn, opening a new harness on the same database and starting a turn in that session sends the earlier messages to the model.
- [ ] Seam-1 test: the schema is created on a fresh database, and reopening an existing one doesn't fail or lose data.
- [ ] Seam-1 test: the last provider chosen is read back from settings.
- [ ] Seam-2 test: the sessions list renders.
