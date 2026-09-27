//! SQLite persistence for sessions, turns and durable events.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, params};

/// Applied in order; `PRAGMA user_version` records how many have run. Never edit a shipped one.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001.sql"),
    include_str!("../migrations/0002.sql"),
    include_str!("../migrations/0003.sql"),
    include_str!("../migrations/0004.sql"),
];

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

// ponytail: one connection behind a mutex, called inline from async code; writes are small.
// Move to a dedicated DB thread if lock contention or blocking shows up.
pub struct Store {
    conn: Mutex<Connection>,
}

pub struct NewSession<'a> {
    pub id: &'a str,
    pub workspace: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: String,
    pub title: Option<String>,
    pub workspace: String,
    pub provider: String,
    pub model: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewTurn<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub input: &'a str,
}

pub struct FinishTurn<'a> {
    pub id: &'a str,
    pub state: &'a str,
    pub error: Option<&'a str>,
    pub usage_json: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub input: String,
    pub state: String,
    pub error: Option<String>,
    pub usage_json: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// A thread-context snapshot to upsert after an LLM or tool step. `parent_id: None` marks the
/// session's root thread; a subagent thread names its parent and carries a `title`.
pub struct UpsertThread<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub parent_id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub context_json: &'a str,
    pub context_usage_json: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Thread {
    pub id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub title: Option<String>,
    pub context_json: String,
    pub context_usage_json: Option<String>,
    pub updated_at: String,
}

pub struct NewEvent<'a> {
    pub id: &'a str,
    pub turn_id: &'a str,
    pub thread_id: Option<&'a str>,
    pub kind: &'a str,
    pub payload_json: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: String,
    pub turn_id: String,
    pub thread_id: Option<String>,
    pub kind: String,
    pub payload_json: String,
    pub created_at: String,
}

impl Store {
    /// Opens (creating if needed) the database file and brings its schema up to date.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let mut conn = Connection::open(path)
            .with_context(|| format!("opening database {}", path.display()))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow!("database lock poisoned"))
    }

    pub fn insert_session(&self, s: &NewSession) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO session (id, workspace, provider, model) VALUES (?1, ?2, ?3, ?4)",
            params![s.id, s.workspace, s.provider, s.model],
        )?;
        Ok(())
    }

    pub fn session(&self, id: &str) -> Result<Option<Session>> {
        let session = self
            .conn()?
            .query_row(
                "SELECT id, title, workspace, provider, model, created_at, updated_at
                 FROM session WHERE id = ?1",
                [id],
                |r| {
                    Ok(Session {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        workspace: r.get(2)?,
                        provider: r.get(3)?,
                        model: r.get(4)?,
                        created_at: r.get(5)?,
                        updated_at: r.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(session)
    }

    /// All sessions, most recently updated first, for `/sessions` to list.
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, workspace, provider, model, created_at, updated_at
             FROM session ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Session {
                id: r.get(0)?,
                title: r.get(1)?,
                workspace: r.get(2)?,
                provider: r.get(3)?,
                model: r.get(4)?,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Sets the session's title if it doesn't have one yet; later calls are no-ops, matching
    /// the reference implementation's first-write-wins semantics.
    pub fn set_session_title_if_unset(&self, session_id: &str, title: &str) -> Result<()> {
        self.conn()?.execute(
            "UPDATE session SET title = ?2 WHERE id = ?1 AND title IS NULL",
            params![session_id, title],
        )?;
        Ok(())
    }

    /// Switches the session's provider and model, e.g. for `/provider`.
    pub fn set_session_provider(
        &self,
        session_id: &str,
        provider: &str,
        model: &str,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            &format!(
                "UPDATE session SET provider = ?2, model = ?3, updated_at = {NOW} WHERE id = ?1"
            ),
            params![session_id, provider, model],
        )?;
        Ok(())
    }

    /// Inserts a running turn and returns its 1-based sequence number within the session.
    pub fn insert_turn(&self, t: &NewTurn) -> Result<i64> {
        let seq = self.conn()?.query_row(
            "INSERT INTO turn (id, session_id, seq, input, state)
             VALUES (?1, ?2, (SELECT COALESCE(MAX(seq), 0) + 1 FROM turn WHERE session_id = ?2), ?3, 'running')
             RETURNING seq",
            params![t.id, t.session_id, t.input],
            |r| r.get(0),
        )?;
        Ok(seq)
    }

    pub fn finish_turn(&self, t: &FinishTurn) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            &format!(
                "UPDATE turn SET state = ?2, error = ?3, usage_json = ?4, finished_at = {NOW}
                 WHERE id = ?1"
            ),
            params![t.id, t.state, t.error, t.usage_json],
        )?;
        conn.execute(
            &format!(
                "UPDATE session SET updated_at = {NOW}
                 WHERE id = (SELECT session_id FROM turn WHERE id = ?1)"
            ),
            [t.id],
        )?;
        Ok(())
    }

    pub fn turns(&self, session_id: &str) -> Result<Vec<Turn>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, seq, input, state, error, usage_json, started_at, finished_at
             FROM turn WHERE session_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([session_id], |r| {
            Ok(Turn {
                id: r.get(0)?,
                session_id: r.get(1)?,
                seq: r.get(2)?,
                input: r.get(3)?,
                state: r.get(4)?,
                error: r.get(5)?,
                usage_json: r.get(6)?,
                started_at: r.get(7)?,
                finished_at: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn insert_event(&self, e: &NewEvent) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO event (id, turn_id, thread_id, type, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![e.id, e.turn_id, e.thread_id, e.kind, e.payload_json],
        )?;
        Ok(())
    }

    /// Reads a setting, e.g. `last_provider`.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn()?
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(value)
    }

    /// Upserts a setting.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Upserts a thread's context snapshot after an LLM or tool step.
    pub fn upsert_thread(&self, t: &UpsertThread) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            &format!(
                "INSERT INTO thread (id, session_id, parent_id, title, context_json, context_usage_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, {NOW})
                 ON CONFLICT (id) DO UPDATE SET
                     context_json = excluded.context_json,
                     context_usage_json = excluded.context_usage_json,
                     updated_at = {NOW}"
            ),
            params![
                t.id,
                t.session_id,
                t.parent_id,
                t.title,
                t.context_json,
                t.context_usage_json
            ],
        )?;
        Ok(())
    }

    /// The session's root thread (no parent) — the one turns continue from and `/sessions`
    /// resumes.
    pub fn root_thread(&self, session_id: &str) -> Result<Option<Thread>> {
        let thread = self
            .conn()?
            .query_row(
                &format!("SELECT {THREAD_COLUMNS} FROM thread WHERE session_id = ?1 AND parent_id IS NULL"),
                [session_id],
                thread_row,
            )
            .optional()?;
        Ok(thread)
    }

    /// Every thread of a session, root and subagents, in creation order.
    pub fn threads(&self, session_id: &str) -> Result<Vec<Thread>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {THREAD_COLUMNS} FROM thread WHERE session_id = ?1 ORDER BY rowid"
        ))?;
        let rows = stmt.query_map([session_id], thread_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A turn's durable events in the order they were written.
    pub fn events(&self, turn_id: &str) -> Result<Vec<Event>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, turn_id, thread_id, type, payload_json, created_at
             FROM event WHERE turn_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map([turn_id], |r| {
            Ok(Event {
                id: r.get(0)?,
                turn_id: r.get(1)?,
                thread_id: r.get(2)?,
                kind: r.get(3)?,
                payload_json: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

const THREAD_COLUMNS: &str =
    "id, session_id, parent_id, title, context_json, context_usage_json, updated_at";

fn thread_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: r.get(0)?,
        session_id: r.get(1)?,
        parent_id: r.get(2)?,
        title: r.get(3)?,
        context_json: r.get(4)?,
        context_usage_json: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let applied: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (version, sql) in (1_i64..).zip(MIGRATIONS).skip_while(|(v, _)| *v <= applied) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("applying migration {version:04}"))?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}
