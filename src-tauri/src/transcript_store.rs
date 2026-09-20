use crate::context_manager::TurnRecord;
use crate::errors::AgentError;
use crate::orchestrator::SessionConfig;
use rusqlite::{Connection, Error as SqliteError, params};
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
        self.conn
            .execute_batch(
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
            );
            CREATE TABLE IF NOT EXISTS delivery_runs (
                session_id TEXT PRIMARY KEY,
                state_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS product_authority (
                project_id TEXT PRIMARY KEY,
                records_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS product_work_orders (
                work_order_id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                status TEXT NOT NULL,
                role TEXT NOT NULL,
                work_order_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS product_coordinator_runs (
                run_id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                status TEXT NOT NULL,
                run_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tool_use_receipts (
                receipt_id TEXT PRIMARY KEY,
                work_order_id TEXT NOT NULL,
                receipt_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tool_use_receipts_work_order
                ON tool_use_receipts(work_order_id, updated_at);",
            )
            .map_err(AgentError::from)
    }

    pub fn save_delivery_state(
        &mut self,
        session_id: &str,
        state_json: &str,
        updated_at: i64,
    ) -> Result<(), AgentError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO delivery_runs (session_id, state_json, updated_at) VALUES (?1, ?2, ?3)",
            params![session_id, state_json, updated_at],
        )?;
        Ok(())
    }

    pub fn get_delivery_state(&self, session_id: &str) -> Result<Option<String>, AgentError> {
        match self.conn.query_row(
            "SELECT state_json FROM delivery_runs WHERE session_id = ?1",
            params![session_id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AgentError::from(error)),
        }
    }

    pub fn get_latest_delivery_state(&self) -> Result<Option<String>, AgentError> {
        match self.conn.query_row(
            "SELECT state_json FROM delivery_runs ORDER BY updated_at DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AgentError::from(error)),
        }
    }

    pub fn delete_delivery_state(&mut self, session_id: &str) -> Result<(), AgentError> {
        self.conn.execute(
            "DELETE FROM delivery_runs WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn save_product_authority(
        &mut self,
        project_id: &str,
        records_json: &str,
        updated_at: i64,
    ) -> Result<(), AgentError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO product_authority (project_id, records_json, updated_at) VALUES (?1, ?2, ?3)",
            params![project_id, records_json, updated_at],
        )?;
        Ok(())
    }

    pub fn get_product_authority(&self, project_id: &str) -> Result<Option<String>, AgentError> {
        match self.conn.query_row(
            "SELECT records_json FROM product_authority WHERE project_id = ?1",
            params![project_id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AgentError::from(error)),
        }
    }

    pub fn save_product_work_order(
        &mut self,
        work_order: &crate::product_os::ProductWorkOrder,
    ) -> Result<(), AgentError> {
        let work_order_json = serde_json::to_string(work_order).map_err(|error| {
            AgentError::DatabaseError(format!("serialize Product OS work order: {error}"))
        })?;
        let status = serde_json::to_string(&work_order.status)
            .map_err(|error| {
                AgentError::DatabaseError(format!("serialize work-order status: {error}"))
            })?
            .trim_matches('"')
            .to_string();
        let role = serde_json::to_string(&work_order.role)
            .map_err(|error| {
                AgentError::DatabaseError(format!("serialize work-order role: {error}"))
            })?
            .trim_matches('"')
            .to_string();
        self.conn.execute(
            "INSERT OR REPLACE INTO product_work_orders (work_order_id, project_id, status, role, work_order_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                work_order.work_order_id,
                work_order.project_id,
                status,
                role,
                work_order_json,
                work_order.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn save_product_authority_and_work_order(
        &mut self,
        project_id: &str,
        records_json: &str,
        work_order: &crate::product_os::ProductWorkOrder,
    ) -> Result<(), AgentError> {
        let work_order_json = serde_json::to_string(work_order).map_err(|error| {
            AgentError::DatabaseError(format!("serialize Product OS work order: {error}"))
        })?;
        let status = serde_json::to_string(&work_order.status)
            .map_err(|error| {
                AgentError::DatabaseError(format!("serialize work-order status: {error}"))
            })?
            .trim_matches('"')
            .to_string();
        let role = serde_json::to_string(&work_order.role)
            .map_err(|error| {
                AgentError::DatabaseError(format!("serialize work-order role: {error}"))
            })?
            .trim_matches('"')
            .to_string();
        let transaction = self.conn.transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO product_authority (project_id, records_json, updated_at) VALUES (?1, ?2, ?3)",
            params![project_id, records_json, work_order.updated_at],
        )?;
        transaction.execute(
            "INSERT OR REPLACE INTO product_work_orders (work_order_id, project_id, status, role, work_order_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                work_order.work_order_id,
                work_order.project_id,
                status,
                role,
                work_order_json,
                work_order.updated_at,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_product_projects(&self) -> Result<Vec<String>, AgentError> {
        let mut statement = self
            .conn
            .prepare("SELECT project_id FROM product_authority ORDER BY project_id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut projects = Vec::new();
        for row in rows {
            projects.push(row.map_err(AgentError::from)?);
        }
        Ok(projects)
    }

    pub fn get_latest_product_project(&self) -> Result<Option<String>, AgentError> {
        match self.conn.query_row(
            "SELECT project_id FROM product_authority ORDER BY updated_at DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AgentError::from(error)),
        }
    }

    pub fn get_product_work_order(
        &self,
        work_order_id: &str,
    ) -> Result<Option<crate::product_os::ProductWorkOrder>, AgentError> {
        match self.conn.query_row(
            "SELECT work_order_json FROM product_work_orders WHERE work_order_id = ?1",
            params![work_order_id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(raw) => serde_json::from_str(&raw).map(Some).map_err(|error| {
                AgentError::DatabaseError(format!("parse Product OS work order: {error}"))
            }),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AgentError::from(error)),
        }
    }

    pub fn list_product_work_orders(
        &self,
        project_id: &str,
    ) -> Result<Vec<crate::product_os::ProductWorkOrder>, AgentError> {
        let mut statement = self.conn.prepare(
            "SELECT work_order_json FROM product_work_orders WHERE project_id = ?1 ORDER BY updated_at, work_order_id",
        )?;
        let rows = statement.query_map(params![project_id], |row| row.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            let raw = row.map_err(AgentError::from)?;
            let value = serde_json::from_str(&raw).map_err(|error| {
                AgentError::DatabaseError(format!("parse Product OS work order: {error}"))
            })?;
            result.push(value);
        }
        Ok(result)
    }

    pub fn save_product_coordinator_run(
        &mut self,
        run: &crate::product_os_coordinator::ProductCoordinatorRun,
    ) -> Result<(), AgentError> {
        let run_json = serde_json::to_string(run).map_err(|error| {
            AgentError::DatabaseError(format!("serialize Product OS coordinator run: {error}"))
        })?;
        let status = serde_json::to_string(&run.status)
            .map_err(|error| {
                AgentError::DatabaseError(format!("serialize coordinator status: {error}"))
            })?
            .trim_matches('"')
            .to_string();
        self.conn.execute(
            "INSERT OR REPLACE INTO product_coordinator_runs (run_id, project_id, status, run_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run.run_id, run.project_id, status, run_json, run.updated_at],
        )?;
        Ok(())
    }

    pub fn get_product_coordinator_run(
        &self,
        run_id: &str,
    ) -> Result<Option<crate::product_os_coordinator::ProductCoordinatorRun>, AgentError> {
        match self.conn.query_row(
            "SELECT run_json FROM product_coordinator_runs WHERE run_id = ?1",
            params![run_id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(raw) => serde_json::from_str(&raw).map(Some).map_err(|error| {
                AgentError::DatabaseError(format!("parse Product OS coordinator run: {error}"))
            }),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AgentError::from(error)),
        }
    }

    pub fn get_latest_product_coordinator_run(
        &self,
    ) -> Result<Option<crate::product_os_coordinator::ProductCoordinatorRun>, AgentError> {
        match self.conn.query_row(
            "SELECT run_json FROM product_coordinator_runs ORDER BY updated_at DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        ) {
            Ok(raw) => serde_json::from_str(&raw).map(Some).map_err(|error| {
                AgentError::DatabaseError(format!("parse Product OS coordinator run: {error}"))
            }),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AgentError::from(error)),
        }
    }

    pub fn save_tool_use_receipt(
        &mut self,
        receipt: &crate::quality_workflows::ToolUseReceipt,
    ) -> Result<(), AgentError> {
        let receipt_json = serde_json::to_string(receipt).map_err(|error| {
            AgentError::DatabaseError(format!("serialize tool-use receipt: {error}"))
        })?;
        self.conn.execute(
            "INSERT OR REPLACE INTO tool_use_receipts (receipt_id, work_order_id, receipt_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                receipt.receipt_id,
                receipt.work_order_id,
                receipt_json,
                receipt.completed_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_tool_use_receipts(
        &self,
        work_order_id: &str,
    ) -> Result<Vec<crate::quality_workflows::ToolUseReceipt>, AgentError> {
        let mut statement = self.conn.prepare(
            "SELECT receipt_json FROM tool_use_receipts
             WHERE work_order_id = ?1 ORDER BY updated_at, receipt_id",
        )?;
        let rows = statement.query_map(params![work_order_id], |row| row.get::<_, String>(0))?;
        let mut receipts = Vec::new();
        for row in rows {
            let raw = row.map_err(AgentError::from)?;
            receipts.push(serde_json::from_str(&raw).map_err(|error| {
                AgentError::DatabaseError(format!("parse tool-use receipt: {error}"))
            })?);
        }
        Ok(receipts)
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

    pub fn update_session_status(
        &mut self,
        session_id: &str,
        status: &str,
    ) -> Result<(), AgentError> {
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
                crate::context_manager::ConsensusSignal::Disagrees(signal_str[10..].to_string())
            } else if signal_str.starts_with("improves:") {
                crate::context_manager::ConsensusSignal::Improves(signal_str[9..].to_string())
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
        let rows_affected = self
            .conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
        if rows_affected == 0 {
            return Err(AgentError::DatabaseError(format!(
                "delete_session: no session found with id '{}'",
                session_id
            )));
        }
        Ok(())
    }
}
