//! SQLite persistence for sessions, turns and durable events.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, params};

/// Applied in order; `PRAGMA user_version` records how many have run. Never edit a shipped one.
const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001.sql")];

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
