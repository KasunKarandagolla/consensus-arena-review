use crate::context_manager::TurnRecord;
use crate::errors::AgentError;
use crate::orchestrator::SessionConfig;
use rusqlite::{params, Connection, Error as SqliteError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub project_brief: String,
    pub session_type: String,
    pub status: String,
    pub created_at: i64,
}

pub struct TranscriptStore {
    conn: Connection,
}

impl TranscriptStore {
    pub fn new() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory db failed");
        let store = Self { conn };
        store.init_schema().expect("schema init failed");
        store
    }

    pub fn open(db_path: &str) -> Result<Self, AgentError> {
        let conn = Connection::open(db_path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), AgentError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                project_brief TEXT NOT NULL,
                session_type TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                completed_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS turns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                role TEXT NOT NULL,
                iteration INTEGER NOT NULL,
                response TEXT NOT NULL,
                consensus_signal TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );",
        )
        .map_err(AgentError::from)
    }

    pub fn create_session(&mut self, config: &SessionConfig) -> Result<(), AgentError> {
        let session_type = serde_json::to_string(&config.session_type)
            .unwrap_or_else(|_| "custom".to_string())
            .trim_matches('"')
            .to_string();
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (id, project_brief, session_type, status, created_at)
             VALUES (?1, ?2, ?3, 'setup', ?4)",
            params![
                config.session_id,
                config.project_brief,
                session_type,
                chrono::Utc::now().timestamp(),
            ],
        )?;
        Ok(())
    }

    pub fn record_turn(&mut self, session_id: &str, record: &TurnRecord) -> Result<(), AgentError> {
        let signal = match &record.consensus_signal {
            crate::context_manager::ConsensusSignal::Agrees => "agrees".to_string(),
            crate::context_manager::ConsensusSignal::Disagrees(r) => format!("disagrees:{}", r),
            crate::context_manager::ConsensusSignal::Improves(r) => format!("improves:{}", r),
            crate::context_manager::ConsensusSignal::NoSignal => "no_signal".to_string(),
        };
        self.conn.execute(
            "INSERT INTO turns (session_id, agent_id, role, iteration, response, consensus_signal, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                session_id,
                record.agent_id,
                record.role,
                record.iteration,
                record.response,
                signal,
                record.timestamp,
            ],
        )?;
        Ok(())
    }

    pub fn update_session_status(&mut self, session_id: &str, status: &str) -> Result<(), AgentError> {
        self.conn.execute(
            "UPDATE sessions SET status = ?1 WHERE id = ?2",
            params![status, session_id],
        )?;
        Ok(())
    }

    pub fn get_transcript(&self, session_id: &str) -> Result<Vec<TurnRecord>, AgentError> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, role, iteration, response, consensus_signal, timestamp
             FROM turns WHERE session_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            let signal_str: String = row.get(4)?;
            let signal = if signal_str == "agrees" {
                crate::context_manager::ConsensusSignal::Agrees
            } else if signal_str.starts_with("disagrees:") {
                crate::context_manager::ConsensusSignal::Disagrees(
                    signal_str[10..].to_string(),
                )
            } else if signal_str.starts_with("improves:") {
                crate::context_manager::ConsensusSignal::Improves(
                    signal_str[9..].to_string(),
                )
            } else {
                crate::context_manager::ConsensusSignal::NoSignal
            };
            Ok(TurnRecord {
                agent_id: row.get(0)?,
                role: row.get(1)?,
                iteration: row.get(2)?,
                response: row.get(3)?,
                consensus_signal: signal,
                timestamp: row.get(5)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(AgentError::from)?);
        }
        Ok(result)
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>, AgentError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_brief, session_type, status, created_at
             FROM sessions ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                project_brief: row.get(1)?,
                session_type: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(AgentError::from)?);
        }
        Ok(result)
    }

    /// Task 3: single-session lookup, backing `get_session_details`.
    /// Returns `Ok(None)` (not an error) when the session simply doesn't
    /// exist — callers decide whether that's an error for their purposes.
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionSummary>, AgentError> {
        let result = self.conn.query_row(
            "SELECT id, project_brief, session_type, status, created_at
             FROM sessions WHERE id = ?1",
            params![session_id],
            |row| {
                Ok(SessionSummary {
                    id: row.get(0)?,
                    project_brief: row.get(1)?,
                    session_type: row.get(2)?,
                    status: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(summary) => Ok(Some(summary)),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AgentError::from(e)),
        }
    }

    /// Task 3 (CRIT-4): backs the `rename_session` command. "Rename" updates
    /// `project_brief` — the field Sidebar.tsx already displays/truncates as
    /// the session's title; there is no separate `title` column in the
    /// schema, and adding one for a label that's otherwise identical to
    /// `project_brief` would just be two sources of truth for the same text.
    /// Returns an error if no row matched (session_id not found) rather than
    /// silently succeeding on a no-op UPDATE.
    pub fn rename_session(&mut self, session_id: &str, new_title: &str) -> Result<(), AgentError> {
        let rows_affected = self.conn.execute(
            "UPDATE sessions SET project_brief = ?1 WHERE id = ?2",
            params![new_title, session_id],
        )?;
        if rows_affected == 0 {
            return Err(AgentError::DatabaseError(format!(
                "rename_session: no session found with id '{}'",
                session_id
            )));
        }
        Ok(())
    }

    /// Task 3 (CRIT-4): backs the `delete_session` command's transcript-side
    /// cascade. Deletes the session's turns before the session row itself.
    /// `rusqlite` is compiled without foreign-key cascade here (no `FOREIGN
    /// KEY ... ON DELETE CASCADE` in the schema), so the order is explicit
    /// rather than relied-upon. Returns an error if the session row itself
    /// didn't exist, even if (hypothetically) some orphaned turns rows did.
    pub fn delete_session(&mut self, session_id: &str) -> Result<(), AgentError> {
        self.conn.execute(
            "DELETE FROM turns WHERE session_id = ?1",
            params![session_id],
        )?;
        let rows_affected = self.conn.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![session_id],
        )?;
        if rows_affected == 0 {
            return Err(AgentError::DatabaseError(format!(
                "delete_session: no session found with id '{}'",
                session_id
            )));
        }
        Ok(())
    }
}
