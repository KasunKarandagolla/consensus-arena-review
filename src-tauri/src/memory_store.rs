use crate::errors::AgentError;
use rusqlite::{Connection, Row, params, params_from_iter};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const MEMORY_SCHEMA_VERSION: i64 = 1;
const MEMORY_CONTEXT_BUDGET: usize = 4_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub session_id: String,
    pub project_brief: String,
    pub category: String,
    pub content: String,
    pub skill_name: Option<String>,
    pub source_agent: String,
    pub source_type: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMemoryEntry {
    pub id: String,
    pub project_brief: String,
    pub category: String,
    pub content: String,
    pub trigger_desc: Option<String>,
    pub importance: String,
    pub superseded_by: Option<String>,
    pub hard_pinned: bool,
    pub skill_name: Option<String>,
    pub source_agent: String,
    pub source_type: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_confirmed_at: i64,
    pub last_accessed_at: i64,
    pub access_count: i64,
    pub mention_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMemoryEntry {
    pub id: String,
    pub category: String,
    pub content: String,
    pub importance: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub mention_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestion {
    pub id: String,
    pub session_id: String,
    pub project_brief: String,
    pub skill_name: Option<String>,
    pub source_agent: String,
    pub question: String,
    pub question_key: String,
    pub raised_iteration: i64,
    pub resolved: bool,
    pub resolution: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStrength {
    pub model_id: String,
    pub topic: String,
    pub adoption_rate: f64,
    pub total_count: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternEntry {
    pub id: String,
    pub project_brief: String,
    pub pattern_condition: String,
    pub pattern_action: String,
    pub pattern_outcome: String,
    pub confidence: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHealth {
    pub is_healthy: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub table_counts: HashMap<String, i64>,
    pub fts_needs_repair: bool,
}

pub enum MemoryReadOutcome<T> {
    Found(T),
    Empty,
    Failed(String),
}

pub struct SessionSummaryData {
    pub investigated: Vec<String>,
    pub completed: Vec<String>,
    pub learned: Vec<String>,
    pub next_steps: Vec<String>,
}

pub struct MemoryStore {
    conn: Connection,
}

impl MemoryStore {
    pub fn new(db_path: &str) -> Result<Self, AgentError> {
        let conn = Connection::open(db_path).map_err(database_error)?;
        Self::setup_schema(&conn)?;
        Ok(Self { conn })
    }

    pub fn new_empty() -> Self {
        let conn = Connection::open_in_memory()
            .unwrap_or_else(|e| panic!("memory fallback connection could not be created: {e}"));
        if let Err(e) = Self::setup_schema(&conn) {
            panic!("memory fallback schema could not be created: {e}");
        }
        Self { conn }
    }

    fn setup_schema(conn: &Connection) -> Result<(), AgentError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA temp_store = memory;
             PRAGMA busy_timeout = 5000;

             CREATE TABLE IF NOT EXISTS session_memory (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 project_brief TEXT NOT NULL,
                 category TEXT NOT NULL,
                 content TEXT NOT NULL,
                 skill_name TEXT,
                 source_agent TEXT NOT NULL DEFAULT 'leader',
                 source_type TEXT NOT NULL DEFAULT 'llm',
                 archived INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL,
                 CHECK (source_type IN ('llm', 'user', 'tool', 'opencode', 'confirmed', 'observed', 'inferred', 'imported'))
             );
             CREATE INDEX IF NOT EXISTS idx_session_memory_session ON session_memory (session_id);
             CREATE INDEX IF NOT EXISTS idx_session_memory_active ON session_memory (project_brief, archived);

             CREATE TABLE IF NOT EXISTS project_memory (
                 id TEXT PRIMARY KEY,
                 project_brief TEXT NOT NULL,
                 category TEXT NOT NULL,
                 content TEXT NOT NULL,
                 trigger_desc TEXT,
                 importance TEXT NOT NULL DEFAULT 'normal',
                 superseded_by TEXT,
                 hard_pinned INTEGER NOT NULL DEFAULT 0,
                 skill_name TEXT,
                 source_agent TEXT NOT NULL DEFAULT 'leader',
                 source_type TEXT NOT NULL DEFAULT 'llm',
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 last_confirmed_at INTEGER NOT NULL,
                 last_accessed_at INTEGER NOT NULL DEFAULT 0,
                 access_count INTEGER NOT NULL DEFAULT 0,
                 mention_count INTEGER NOT NULL DEFAULT 1,
                 FOREIGN KEY (superseded_by) REFERENCES project_memory(id) ON DELETE SET NULL,
                 CHECK (importance IN ('high', 'normal', 'low', 'superseded')),
                 CHECK (category IN (
                     'project_config', 'decision', 'deferred', 'rejected', 'observation',
                     'pattern', 'user_preference', 'routing', 'investigated', 'completed',
                     'learned', 'next_steps'
                 )),
                 CHECK (source_type IN ('llm', 'user', 'tool', 'opencode', 'confirmed', 'observed', 'inferred', 'imported'))
             );
             CREATE INDEX IF NOT EXISTS idx_project_memory_brief ON project_memory (project_brief);
             CREATE INDEX IF NOT EXISTS idx_project_memory_superseded ON project_memory (superseded_by);
             CREATE INDEX IF NOT EXISTS idx_project_memory_rank
                 ON project_memory (project_brief, hard_pinned, importance, last_confirmed_at, last_accessed_at);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_project_memory_unique_content
                 ON project_memory (project_brief, category, content);

             CREATE VIRTUAL TABLE IF NOT EXISTS project_memory_fts
             USING fts5(
                 project_brief, category, content, trigger_desc,
                 content=project_memory, content_rowid=rowid
             );
             INSERT INTO project_memory_fts(project_memory_fts, rank)
             VALUES('rank', 'bm25(0.0, 0.2, 2.0, 1.5)');

             CREATE TRIGGER IF NOT EXISTS project_memory_ai
             AFTER INSERT ON project_memory BEGIN
                 INSERT INTO project_memory_fts(rowid, project_brief, category, content, trigger_desc)
                 VALUES (new.rowid, new.project_brief, new.category, new.content, new.trigger_desc);
             END;
             CREATE TRIGGER IF NOT EXISTS project_memory_ad
             AFTER DELETE ON project_memory BEGIN
                 INSERT INTO project_memory_fts(project_memory_fts, rowid, project_brief, category, content, trigger_desc)
                 VALUES ('delete', old.rowid, old.project_brief, old.category, old.content, old.trigger_desc);
             END;
             CREATE TRIGGER IF NOT EXISTS project_memory_au
             AFTER UPDATE ON project_memory BEGIN
                 INSERT INTO project_memory_fts(project_memory_fts, rowid, project_brief, category, content, trigger_desc)
                 VALUES ('delete', old.rowid, old.project_brief, old.category, old.content, old.trigger_desc);
                 INSERT INTO project_memory_fts(rowid, project_brief, category, content, trigger_desc)
                 VALUES (new.rowid, new.project_brief, new.category, new.content, new.trigger_desc);
             END;

             CREATE TABLE IF NOT EXISTS global_memory (
                 id TEXT PRIMARY KEY,
                 category TEXT NOT NULL,
                 content TEXT NOT NULL,
                 importance TEXT NOT NULL DEFAULT 'normal',
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 mention_count INTEGER NOT NULL DEFAULT 1,
                 CHECK (importance IN ('high', 'normal', 'low'))
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_global_memory_unique ON global_memory (category, content);

             CREATE TABLE IF NOT EXISTS open_questions (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 project_brief TEXT NOT NULL,
                 skill_name TEXT,
                 source_agent TEXT NOT NULL DEFAULT 'leader',
                 question TEXT NOT NULL,
                 question_key TEXT NOT NULL,
                 raised_iteration INTEGER NOT NULL DEFAULT 0,
                 resolved INTEGER NOT NULL DEFAULT 0,
                 resolution TEXT,
                 created_at INTEGER NOT NULL,
                 resolved_at INTEGER
             );
             CREATE INDEX IF NOT EXISTS idx_open_questions_brief ON open_questions (project_brief, resolved);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_open_questions_unique_active
                 ON open_questions (project_brief, question_key, resolved);

             CREATE TABLE IF NOT EXISTS model_reliability (
                 id TEXT PRIMARY KEY,
                 project_brief TEXT NOT NULL,
                 model_id TEXT NOT NULL,
                 skill_name TEXT,
                 topic TEXT NOT NULL,
                 adopted INTEGER NOT NULL,
                 session_id TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_model_reliability_brief
                 ON model_reliability (project_brief, model_id, topic);
             CREATE INDEX IF NOT EXISTS idx_model_reliability_strength
                 ON model_reliability (project_brief, topic, adopted);

             CREATE TABLE IF NOT EXISTS pattern_memory (
                 id TEXT PRIMARY KEY,
                 project_brief TEXT NOT NULL,
                 pattern_condition TEXT NOT NULL,
                 pattern_action TEXT NOT NULL,
                 pattern_outcome TEXT NOT NULL,
                 confidence INTEGER NOT NULL DEFAULT 1,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_pattern_memory_brief ON pattern_memory (project_brief);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_pattern_memory_unique
                 ON pattern_memory (project_brief, pattern_condition, pattern_action);",
        )
        .map_err(database_error)?;

        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap_or(0);
        if version < MEMORY_SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", MEMORY_SCHEMA_VERSION)
                .map_err(database_error)?;
        }
        Ok(())
    }

    pub fn with_transaction<F, T>(&mut self, f: F) -> Result<T, AgentError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, AgentError>,
    {
        let tx = self.conn.transaction().map_err(database_error)?;
        let result = f(&tx)?;
        tx.commit().map_err(database_error)?;
        Ok(result)
    }

    pub fn add_session_fact(
        &mut self,
        session_id: &str,
        project_brief: &str,
        category: &str,
        content: &str,
        skill_name: Option<&str>,
        source_agent: &str,
        source_type: &str,
    ) -> Result<(), AgentError> {
        self.conn
            .execute(
                "INSERT INTO session_memory
                 (id, session_id, project_brief, category, content, skill_name,
                  source_agent, source_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    session_id,
                    project_brief,
                    category,
                    content,
                    skill_name,
                    source_agent,
                    source_type,
                    chrono::Utc::now().timestamp()
                ],
            )
            .map_err(database_error)?;
        Ok(())
    }

    pub fn get_session_facts(&self, session_id: &str) -> Result<Vec<MemoryEntry>, AgentError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, project_brief, category, content, skill_name,
                        source_agent, source_type, created_at
                 FROM session_memory
                 WHERE session_id = ?1 AND archived = 0
                 ORDER BY created_at ASC",
            )
            .map_err(database_error)?;
        let rows = stmt
            .query_map([session_id], memory_entry_from_row)
            .map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }

    pub fn archive_old_session_facts(
        &mut self,
        project_brief: &str,
        current_session_id: &str,
    ) -> Result<usize, AgentError> {
        self.conn
            .execute(
                "UPDATE session_memory SET archived = 1
                 WHERE project_brief = ?1 AND session_id != ?2 AND archived = 0",
                params![project_brief, current_session_id],
            )
            .map_err(database_error)
    }

    pub fn add_project_memory_with_source(
        &mut self,
        project_brief: &str,
        category: &str,
        content: &str,
        trigger_desc: Option<&str>,
        skill_name: Option<&str>,
        source_agent: &str,
        source_type: &str,
    ) -> Result<(), AgentError> {
        let now = chrono::Utc::now().timestamp();
        self.conn
            .execute(
                "INSERT INTO project_memory
                 (id, project_brief, category, content, trigger_desc, importance,
                  hard_pinned, skill_name, source_agent, source_type,
                  created_at, updated_at, last_confirmed_at, mention_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'normal', 0, ?6, ?7, ?8, ?9, ?9, ?9, 1)
                 ON CONFLICT(project_brief, category, content) DO UPDATE SET
                    mention_count = mention_count + 1,
                    updated_at = ?9,
                    last_confirmed_at = ?9,
                    trigger_desc = COALESCE(project_memory.trigger_desc, excluded.trigger_desc),
                    importance = CASE
                        WHEN project_memory.mention_count + 1 >= 3 THEN 'high'
                        ELSE project_memory.importance
                    END",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_brief,
                    category,
                    content,
                    trigger_desc,
                    skill_name,
                    source_agent,
                    source_type,
                    now
                ],
            )
            .map_err(database_error)?;
        Ok(())
    }

    pub fn add_project_memory(
        &mut self,
        project_brief: &str,
        category: &str,
        content: &str,
        trigger_desc: Option<&str>,
        skill_name: Option<&str>,
    ) -> Result<(), AgentError> {
        self.add_project_memory_with_source(
            project_brief,
            category,
            content,
            trigger_desc,
            skill_name,
            "leader",
            "llm",
        )
    }

    pub fn save_project_config(
        &mut self,
        project_brief: &str,
        content: &str,
    ) -> Result<(), AgentError> {
        let content = safe_prefix(content, 2_000);
        let project_brief = project_brief.to_string();
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM project_memory
                 WHERE project_brief = ?1 AND category = 'project_config'",
                [&project_brief],
            )
            .map_err(database_error)?;
            let now = chrono::Utc::now().timestamp();
            tx.execute(
                "INSERT INTO project_memory
                 (id, project_brief, category, content, trigger_desc, importance,
                  hard_pinned, skill_name, source_agent, source_type,
                  created_at, updated_at, last_confirmed_at, mention_count)
                 VALUES (?1, ?2, 'project_config', ?3, NULL, 'high', 1, NULL,
                         'user', 'confirmed', ?4, ?4, ?4, 1)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_brief,
                    content,
                    now
                ],
            )
            .map_err(database_error)?;
            Ok(())
        })
    }

    pub fn get_project_config(&self, project_brief: &str) -> Result<String, AgentError> {
        match self.conn.query_row(
            "SELECT content FROM project_memory
             WHERE project_brief = ?1 AND category = 'project_config'
             ORDER BY created_at DESC LIMIT 1",
            [project_brief],
            |row| row.get(0),
        ) {
            Ok(content) => Ok(content),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(String::new()),
            Err(e) => Err(database_error(e)),
        }
    }

    fn get_project_memory_peek(
        &self,
        project_brief: &str,
    ) -> Result<Vec<ProjectMemoryEntry>, AgentError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "{} WHERE project_brief = ?1 AND importance != 'superseded'
                 ORDER BY hard_pinned DESC,
                    CASE source_type
                        WHEN 'confirmed' THEN 6 WHEN 'observed' THEN 5
                        WHEN 'imported' THEN 4 WHEN 'user' THEN 3
                        WHEN 'llm' THEN 2 WHEN 'inferred' THEN 1 ELSE 0
                    END DESC,
                    CASE importance WHEN 'high' THEN 3 WHEN 'normal' THEN 2 ELSE 1 END DESC,
                    mention_count DESC,
                    MAX(last_confirmed_at, last_accessed_at) DESC,
                    created_at ASC",
                project_memory_select()
            ))
            .map_err(database_error)?;
        let rows = stmt
            .query_map([project_brief], project_memory_from_row)
            .map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }

    pub fn get_project_memory(
        &mut self,
        project_brief: &str,
    ) -> Result<Vec<ProjectMemoryEntry>, AgentError> {
        let rows = self.get_project_memory_peek(project_brief)?;
        if let Err(e) = self.bump_project_access(&rows) {
            eprintln!("[MEMORY] access stats update failed: {e}");
        }
        Ok(rows)
    }

    pub fn get_project_memory_checked(
        &self,
        project_brief: &str,
    ) -> MemoryReadOutcome<Vec<ProjectMemoryEntry>> {
        match self.get_project_memory_peek(project_brief) {
            Ok(rows) if rows.is_empty() => MemoryReadOutcome::Empty,
            Ok(rows) => MemoryReadOutcome::Found(rows),
            Err(e) => MemoryReadOutcome::Failed(e.to_string()),
        }
    }

    pub fn clear_project_memory(&mut self, project_brief: &str) -> Result<(), AgentError> {
        let project_brief = project_brief.to_string();
        self.with_transaction(|tx| {
            for table in [
                "project_memory",
                "session_memory",
                "open_questions",
                "model_reliability",
                "pattern_memory",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE project_brief = ?1"),
                    [&project_brief],
                )
                .map_err(database_error)?;
            }
            Ok(())
        })
    }

    pub fn search_project_memory(
        &mut self,
        project_brief: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ProjectMemoryEntry>, AgentError> {
        let sql = format!(
            "{} JOIN project_memory_fts fts ON pm.rowid = fts.rowid
             WHERE pm.project_brief = ?1
               AND project_memory_fts MATCH ?2
               AND pm.importance != 'superseded'
             ORDER BY rank
             LIMIT ?3",
            project_memory_select_with_alias()
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(_) => return Ok(Vec::new()),
        };
        let mapped = match stmt.query_map(
            params![project_brief, query, limit as i64],
            project_memory_from_row,
        ) {
            Ok(rows) => rows,
            Err(_) => return Ok(Vec::new()),
        };
        let rows = match mapped.collect::<Result<Vec<_>, _>>() {
            Ok(rows) => rows,
            Err(_) => return Ok(Vec::new()),
        };
        drop(stmt);
        if let Err(e) = self.bump_project_access(&rows) {
            eprintln!("[MEMORY] search access stats update failed: {e}");
        }
        Ok(rows)
    }

    pub fn repair_fts_index(&mut self) -> Result<(), AgentError> {
        self.conn
            .execute(
                "INSERT INTO project_memory_fts(project_memory_fts) VALUES('rebuild')",
                [],
            )
            .map_err(database_error)?;
        Ok(())
    }

    pub fn add_open_question(
        &mut self,
        session_id: &str,
        project_brief: &str,
        question: &str,
        raised_iteration: u32,
    ) -> Result<(), AgentError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO open_questions
                 (id, session_id, project_brief, source_agent, question, question_key,
                  raised_iteration, resolved, created_at)
                 VALUES (?1, ?2, ?3, 'leader', ?4, ?5, ?6, 0, ?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    session_id,
                    project_brief,
                    question,
                    normalize_question_key(question),
                    i64::from(raised_iteration),
                    chrono::Utc::now().timestamp()
                ],
            )
            .map_err(database_error)?;
        Ok(())
    }

    pub fn resolve_question(
        &mut self,
        session_id: &str,
        question_prefix: &str,
        resolution: &str,
    ) -> Result<(), AgentError> {
        self.conn
            .execute(
                "UPDATE open_questions
                 SET resolved = 1, resolution = ?1, resolved_at = ?2
                 WHERE session_id = ?3 AND question_key = ?4 AND resolved = 0",
                params![
                    resolution,
                    chrono::Utc::now().timestamp(),
                    session_id,
                    normalize_question_key(question_prefix)
                ],
            )
            .map_err(database_error)?;
        Ok(())
    }

    pub fn get_open_questions(&self, project_brief: &str) -> Result<Vec<OpenQuestion>, AgentError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, project_brief, skill_name, source_agent,
                        question, question_key, raised_iteration, resolved, resolution,
                        created_at, resolved_at
                 FROM open_questions
                 WHERE project_brief = ?1 AND resolved = 0
                 ORDER BY created_at ASC",
            )
            .map_err(database_error)?;
        let rows = stmt
            .query_map([project_brief], open_question_from_row)
            .map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }

    pub fn record_model_response(
        &mut self,
        project_brief: &str,
        model_id: &str,
        topic: &str,
        adopted: bool,
        session_id: &str,
    ) -> Result<(), AgentError> {
        self.conn
            .execute(
                "INSERT INTO model_reliability
                 (id, project_brief, model_id, topic, adopted, session_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_brief,
                    model_id,
                    topic,
                    adopted,
                    session_id,
                    chrono::Utc::now().timestamp()
                ],
            )
            .map_err(database_error)?;
        Ok(())
    }

    pub fn get_model_strengths(
        &self,
        project_brief: &str,
    ) -> Result<Vec<ModelStrength>, AgentError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT model_id, topic,
                        CAST(SUM(adopted) AS REAL) / COUNT(*) AS adoption_rate,
                        COUNT(*) AS total_count
                 FROM model_reliability
                 WHERE project_brief = ?1
                 GROUP BY model_id, topic
                 HAVING COUNT(*) >= 2
                 ORDER BY adoption_rate DESC, total_count DESC",
            )
            .map_err(database_error)?;
        let rows = stmt
            .query_map([project_brief], |row| {
                let adoption_rate: f64 = row.get(2)?;
                let label = if adoption_rate >= 0.7 {
                    "strong"
                } else if adoption_rate >= 0.4 {
                    "moderate"
                } else {
                    "weak"
                };
                Ok(ModelStrength {
                    model_id: row.get(0)?,
                    topic: row.get(1)?,
                    adoption_rate,
                    total_count: row.get(3)?,
                    label: label.to_string(),
                })
            })
            .map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }

    pub fn add_pattern(
        &mut self,
        project_brief: &str,
        pattern_condition: &str,
        pattern_action: &str,
        pattern_outcome: &str,
    ) -> Result<(), AgentError> {
        let now = chrono::Utc::now().timestamp();
        self.conn
            .execute(
                "INSERT INTO pattern_memory
                 (id, project_brief, pattern_condition, pattern_action, pattern_outcome,
                  confidence, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)
                 ON CONFLICT(project_brief, pattern_condition, pattern_action) DO UPDATE SET
                    confidence = confidence + 1,
                    pattern_outcome = excluded.pattern_outcome,
                    updated_at = ?6",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_brief,
                    pattern_condition,
                    pattern_action,
                    pattern_outcome,
                    now
                ],
            )
            .map_err(database_error)?;
        Ok(())
    }

    pub fn get_patterns(&self, project_brief: &str) -> Result<Vec<PatternEntry>, AgentError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, project_brief, pattern_condition, pattern_action,
                        pattern_outcome, confidence, created_at, updated_at
                 FROM pattern_memory WHERE project_brief = ?1
                 ORDER BY confidence DESC, updated_at DESC LIMIT 10",
            )
            .map_err(database_error)?;
        let rows = stmt
            .query_map([project_brief], |row| {
                Ok(PatternEntry {
                    id: row.get(0)?,
                    project_brief: row.get(1)?,
                    pattern_condition: row.get(2)?,
                    pattern_action: row.get(3)?,
                    pattern_outcome: row.get(4)?,
                    confidence: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }

    pub fn get_global_memory(&self) -> Result<Vec<GlobalMemoryEntry>, AgentError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, category, content, importance, created_at, updated_at, mention_count
                 FROM global_memory
                 ORDER BY CASE importance WHEN 'high' THEN 3 WHEN 'normal' THEN 2 ELSE 1 END DESC,
                          mention_count DESC, updated_at DESC, created_at ASC",
            )
            .map_err(database_error)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(GlobalMemoryEntry {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    content: row.get(2)?,
                    importance: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    mention_count: row.get(6)?,
                })
            })
            .map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }

    pub fn record_blueprint_finalized(
        &mut self,
        project_brief: &str,
        section_title: &str,
        models_consulted: &[String],
        iterations_since_last_section: u32,
    ) -> Result<(), AgentError> {
        let topic = detect_topic(section_title).to_string();
        let models_joined = models_consulted.join(", ");
        let title_lower = section_title.to_lowercase();
        let project_brief = project_brief.to_string();
        let section_title = section_title.to_string();

        self.with_transaction(|tx| {
            let now = chrono::Utc::now().timestamp();
            tx.execute(
                "INSERT INTO project_memory
                 (id, project_brief, category, content, importance,
                  hard_pinned, source_agent, source_type,
                  created_at, updated_at, last_confirmed_at, mention_count)
                 VALUES (?1, ?2, 'decision', ?3, 'normal', 0, 'leader', 'llm', ?4, ?4, ?4, 1)
                 ON CONFLICT(project_brief, category, content) DO UPDATE SET
                    mention_count = mention_count + 1,
                    updated_at = ?4,
                    last_confirmed_at = ?4",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_brief,
                    format!("Blueprint section finalised: {section_title}"),
                    now
                ],
            )
            .map_err(database_error)?;

            tx.execute(
                "INSERT INTO pattern_memory
                 (id, project_brief, pattern_condition, pattern_action, pattern_outcome,
                  confidence, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)
                 ON CONFLICT(project_brief, pattern_condition, pattern_action) DO UPDATE SET
                    confidence = confidence + 1,
                    pattern_outcome = excluded.pattern_outcome,
                    updated_at = ?6",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    project_brief,
                    format!("{topic} section"),
                    format!("consulted: {models_joined}"),
                    format!("finalised in {iterations_since_last_section} iterations"),
                    now
                ],
            )
            .map_err(database_error)?;

            for word in title_lower.split_whitespace().take(3) {
                if word.len() > 4 {
                    let key = normalize_question_key(word);
                    let _ = tx.execute(
                        "UPDATE open_questions
                         SET resolved = 1, resolution = ?1, resolved_at = ?2
                         WHERE project_brief = ?3 AND question_key = ?4 AND resolved = 0",
                        params![
                            format!("Resolved via blueprint section: {section_title}"),
                            now,
                            project_brief,
                            key
                        ],
                    );
                }
            }
            Ok(())
        })
    }

    pub fn write_session_completion_memory(
        &mut self,
        _session_id: &str,
        project_brief: &str,
        summary: &SessionSummaryData,
        open_questions: &[OpenQuestion],
        sections_finalized: u32,
        total_iterations: u32,
    ) -> Result<(), AgentError> {
        let project_brief = project_brief.to_string();
        self.with_transaction(|tx| {
            let now = chrono::Utc::now().timestamp();
            for (category, values) in [
                ("investigated", &summary.investigated),
                ("completed", &summary.completed),
                ("learned", &summary.learned),
                ("next_steps", &summary.next_steps),
            ] {
                if !values.is_empty() {
                    upsert_transaction_project_memory(
                        tx,
                        &project_brief,
                        category,
                        &values.join("; "),
                        now,
                    )?;
                }
            }

            for question in open_questions.iter().filter(|question| !question.resolved) {
                upsert_transaction_project_memory(
                    tx,
                    &project_brief,
                    "deferred",
                    &format!("Unresolved at session end: {}", question.question),
                    now,
                )?;
            }

            if sections_finalized >= 3 {
                let content = format!(
                    "Session complete: {sections_finalized} sections agreed in {total_iterations} iterations"
                );
                tx.execute(
                    "INSERT INTO global_memory
                     (id, category, content, importance, created_at, updated_at, mention_count)
                     VALUES (?1, 'completed', ?2, 'normal', ?3, ?3, 1)
                     ON CONFLICT(category, content) DO UPDATE SET
                        mention_count = mention_count + 1,
                        updated_at = ?3",
                    params![uuid::Uuid::new_v4().to_string(), content, now],
                )
                .map_err(database_error)?;
            }
            Ok(())
        })
    }

    pub fn decay_stale_importance(&mut self, project_brief: &str) -> Result<usize, AgentError> {
        let now = chrono::Utc::now().timestamp();
        let stale_before = now - (30 * 24 * 60 * 60);
        self.conn
            .execute(
                "UPDATE project_memory
                 SET importance = 'low', updated_at = ?1
                 WHERE project_brief = ?2
                   AND importance = 'normal'
                   AND hard_pinned = 0
                   AND MAX(last_confirmed_at, last_accessed_at) < ?3",
                params![now, project_brief, stale_before],
            )
            .map_err(database_error)
    }

    pub fn check_health(&self) -> MemoryHealth {
        let mut health = MemoryHealth {
            is_healthy: true,
            issues: Vec::new(),
            warnings: Vec::new(),
            table_counts: HashMap::new(),
            fts_needs_repair: false,
        };

        match self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        {
            Ok(result) if result == "ok" => {}
            Ok(result) => health
                .issues
                .push(format!("integrity_check returned: {result}")),
            Err(e) => health.issues.push(format!("integrity_check failed: {e}")),
        }

        for table in [
            "session_memory",
            "project_memory",
            "global_memory",
            "open_questions",
            "model_reliability",
            "pattern_memory",
        ] {
            match self
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                }) {
                Ok(count) => {
                    health.table_counts.insert(table.to_string(), count);
                }
                Err(e) => {
                    health.table_counts.insert(table.to_string(), -1);
                    health.issues.push(format!("failed to count {table}: {e}"));
                }
            }
        }

        let project_count = health.table_counts.get("project_memory").copied();
        let fts_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM project_memory_fts", [], |row| {
                row.get::<_, i64>(0)
            });
        match (project_count, fts_count) {
            (Some(project_count), Ok(fts_count)) if project_count != fts_count => {
                health.fts_needs_repair = true;
                health.warnings.push(format!(
                    "project memory/search index count mismatch ({project_count} vs {fts_count})"
                ));
            }
            (_, Err(e)) => {
                health.fts_needs_repair = true;
                health
                    .warnings
                    .push(format!("search index check failed: {e}"));
            }
            _ => {}
        }

        health.is_healthy = health.issues.is_empty();
        health
    }

    pub fn export_to(&self, destination_path: &str) -> Result<(), AgentError> {
        let mut dst = Connection::open(destination_path).map_err(database_error)?;
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut dst).map_err(database_error)?;
        backup
            .run_to_completion(5, Duration::from_millis(250), None)
            .map_err(database_error)
    }

    pub fn restore_from(&mut self, source_path: &str) -> Result<(), AgentError> {
        let src = Connection::open(source_path).map_err(database_error)?;
        let version: i64 = src
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| {
                AgentError::DatabaseError(format!("source file doesn't look like a memory.db: {e}"))
            })?;
        if version < MEMORY_SCHEMA_VERSION {
            return Err(AgentError::DatabaseError(
                "source file is missing the expected schema version stamp".to_string(),
            ));
        }
        let backup = rusqlite::backup::Backup::new(&src, &mut self.conn).map_err(database_error)?;
        backup
            .run_to_completion(5, Duration::from_millis(250), None)
            .map_err(database_error)
    }

    pub fn commit_pending_state(&mut self) -> Result<(), AgentError> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(database_error)
    }

    pub fn build_memory_context(
        &mut self,
        _session_id: &str,
        project_brief: &str,
        session_type: Option<&str>,
        recent_leader_text: Option<&str>,
    ) -> Result<String, AgentError> {
        let project_rows = self.get_project_memory(project_brief)?;
        let mut included_ids = HashSet::new();
        let mut included_content = HashSet::new();
        let mut context = String::new();

        let mut pinned: Vec<&ProjectMemoryEntry> = project_rows
            .iter()
            .filter(|entry| entry.hard_pinned)
            .collect();
        pinned.sort_by_key(|entry| {
            if entry.category == "project_config" {
                0
            } else {
                1
            }
        });
        if !pinned.is_empty() {
            context.push_str("=== PROJECT CONFIG — HARD PINNED ===\n");
            for entry in pinned {
                include_entry_signature(entry, &mut included_ids, &mut included_content);
                context.push_str(&format!("- {}\n", entry.content));
            }
            context.push('\n');
            if context.chars().count() > MEMORY_CONTEXT_BUDGET {
                eprintln!(
                    "[MEMORY] hard-pinned context exceeds the {} character soft cap",
                    MEMORY_CONTEXT_BUDGET
                );
            }
        }

        let high_lines = project_rows
            .iter()
            .filter(|entry| !entry.hard_pinned && entry.importance == "high")
            .filter_map(|entry| {
                if is_duplicate(entry, &included_ids, &included_content) {
                    return None;
                }
                include_entry_signature(entry, &mut included_ids, &mut included_content);
                Some(format!(
                    "- [{} / {}] {}",
                    entry.category, entry.source_type, entry.content
                ))
            })
            .collect::<Vec<_>>();
        append_limited_section(
            &mut context,
            "=== HIGH-CONFIDENCE PROJECT MEMORY ===",
            high_lines,
            MEMORY_CONTEXT_BUDGET,
        );

        if context.chars().count() < MEMORY_CONTEXT_BUDGET {
            let query = session_type
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| recent_leader_text.map(|text| detect_topic(text).to_string()));
            if let Some(query) = query {
                let recalled = self.search_project_memory(project_brief, &query, 5)?;
                let recalled_lines = recalled
                    .iter()
                    .filter_map(|entry| {
                        if is_duplicate(entry, &included_ids, &included_content) {
                            return None;
                        }
                        include_entry_signature(entry, &mut included_ids, &mut included_content);
                        Some(format!("- [{}] {}", entry.category, entry.content))
                    })
                    .collect::<Vec<_>>();
                append_limited_section(
                    &mut context,
                    "=== RELEVANT RECALLED MEMORY ===",
                    recalled_lines,
                    MEMORY_CONTEXT_BUDGET,
                );
            }
        }

        let question_lines = self
            .get_open_questions(project_brief)?
            .into_iter()
            .take(5)
            .map(|question| format!("- {}", question.question))
            .collect();
        append_limited_section(
            &mut context,
            "=== OPEN QUESTIONS ===",
            question_lines,
            MEMORY_CONTEXT_BUDGET,
        );

        let strength_lines = self
            .get_model_strengths(project_brief)?
            .into_iter()
            .filter(|strength| strength.label != "weak")
            .take(5)
            .map(|strength| {
                format!(
                    "- {} is {} on {} ({:.0}% adoption, {} observations)",
                    strength.model_id,
                    strength.label,
                    strength.topic,
                    strength.adoption_rate * 100.0,
                    strength.total_count
                )
            })
            .collect();
        append_limited_section(
            &mut context,
            "=== MODEL STRENGTH HINTS ===",
            strength_lines,
            MEMORY_CONTEXT_BUDGET,
        );

        Ok(context.trim_end().to_string())
    }

    fn bump_project_access(&self, rows: &[ProjectMemoryEntry]) -> Result<(), AgentError> {
        if rows.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; rows.len()].join(", ");
        let ids = rows
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        self.conn
            .execute(
                &format!(
                    "UPDATE project_memory
                     SET last_accessed_at = {}, access_count = access_count + 1
                     WHERE id IN ({placeholders})",
                    chrono::Utc::now().timestamp()
                ),
                params_from_iter(ids),
            )
            .map_err(database_error)?;
        Ok(())
    }
}

pub fn safe_prefix(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

pub fn normalize_question_key(question: &str) -> String {
    question
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric() || character.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(40)
        .collect()
}

pub fn detect_topic(prompt: &str) -> &'static str {
    let lower = prompt.to_lowercase();
    if lower.contains("database")
        || lower.contains("schema")
        || lower.contains("sql")
        || lower.contains("postgres")
        || lower.contains("mysql")
        || lower.contains("migration")
    {
        return "database";
    }
    if lower.contains("auth")
        || lower.contains("login")
        || lower.contains("jwt")
        || lower.contains("password")
        || lower.contains("token")
        || lower.contains("session")
    {
        return "auth";
    }
    if lower.contains("api")
        || lower.contains("endpoint")
        || lower.contains("rest")
        || lower.contains("graphql")
        || lower.contains("route")
        || lower.contains("request")
    {
        return "api";
    }
    if lower.contains("security")
        || lower.contains("vulnerability")
        || lower.contains("owasp")
        || lower.contains("encrypt")
        || lower.contains("attack")
        || lower.contains("xss")
    {
        return "security";
    }
    if lower.contains("architect")
        || lower.contains("design")
        || lower.contains("system")
        || lower.contains("scalab")
        || lower.contains("infrastructure")
    {
        return "architecture";
    }
    if lower.contains("test")
        || lower.contains("ci")
        || lower.contains("deploy")
        || lower.contains("pipeline")
        || lower.contains("quality")
    {
        return "devops";
    }
    "general"
}

fn database_error(error: rusqlite::Error) -> AgentError {
    AgentError::DatabaseError(error.to_string())
}

fn memory_entry_from_row(row: &Row<'_>) -> rusqlite::Result<MemoryEntry> {
    Ok(MemoryEntry {
        id: row.get(0)?,
        session_id: row.get(1)?,
        project_brief: row.get(2)?,
        category: row.get(3)?,
        content: row.get(4)?,
        skill_name: row.get(5)?,
        source_agent: row.get(6)?,
        source_type: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn project_memory_select() -> &'static str {
    "SELECT id, project_brief, category, content, trigger_desc, importance,
            superseded_by, hard_pinned, skill_name, source_agent, source_type,
            created_at, updated_at, last_confirmed_at, last_accessed_at,
            access_count, mention_count FROM project_memory"
}

fn project_memory_select_with_alias() -> &'static str {
    "SELECT pm.id, pm.project_brief, pm.category, pm.content, pm.trigger_desc,
            pm.importance, pm.superseded_by, pm.hard_pinned, pm.skill_name,
            pm.source_agent, pm.source_type, pm.created_at, pm.updated_at,
            pm.last_confirmed_at, pm.last_accessed_at, pm.access_count,
            pm.mention_count FROM project_memory pm"
}

fn project_memory_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectMemoryEntry> {
    Ok(ProjectMemoryEntry {
        id: row.get(0)?,
        project_brief: row.get(1)?,
        category: row.get(2)?,
        content: row.get(3)?,
        trigger_desc: row.get(4)?,
        importance: row.get(5)?,
        superseded_by: row.get(6)?,
        hard_pinned: row.get::<_, i64>(7)? != 0,
        skill_name: row.get(8)?,
        source_agent: row.get(9)?,
        source_type: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        last_confirmed_at: row.get(13)?,
        last_accessed_at: row.get(14)?,
        access_count: row.get(15)?,
        mention_count: row.get(16)?,
    })
}

fn open_question_from_row(row: &Row<'_>) -> rusqlite::Result<OpenQuestion> {
    Ok(OpenQuestion {
        id: row.get(0)?,
        session_id: row.get(1)?,
        project_brief: row.get(2)?,
        skill_name: row.get(3)?,
        source_agent: row.get(4)?,
        question: row.get(5)?,
        question_key: row.get(6)?,
        raised_iteration: row.get(7)?,
        resolved: row.get::<_, i64>(8)? != 0,
        resolution: row.get(9)?,
        created_at: row.get(10)?,
        resolved_at: row.get(11)?,
    })
}

fn upsert_transaction_project_memory(
    tx: &rusqlite::Transaction<'_>,
    project_brief: &str,
    category: &str,
    content: &str,
    now: i64,
) -> Result<(), AgentError> {
    tx.execute(
        "INSERT INTO project_memory
         (id, project_brief, category, content, importance, hard_pinned,
          source_agent, source_type, created_at, updated_at, last_confirmed_at, mention_count)
         VALUES (?1, ?2, ?3, ?4, 'normal', 0, 'leader', 'llm', ?5, ?5, ?5, 1)
         ON CONFLICT(project_brief, category, content) DO UPDATE SET
            mention_count = mention_count + 1,
            updated_at = ?5,
            last_confirmed_at = ?5",
        params![
            uuid::Uuid::new_v4().to_string(),
            project_brief,
            category,
            content,
            now
        ],
    )
    .map_err(database_error)?;
    Ok(())
}

fn normalized_content(content: &str) -> String {
    content
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn include_entry_signature(
    entry: &ProjectMemoryEntry,
    ids: &mut HashSet<String>,
    content: &mut HashSet<String>,
) {
    ids.insert(entry.id.clone());
    content.insert(normalized_content(&entry.content));
}

fn is_duplicate(
    entry: &ProjectMemoryEntry,
    ids: &HashSet<String>,
    content: &HashSet<String>,
) -> bool {
    ids.contains(&entry.id) || content.contains(&normalized_content(&entry.content))
}

fn append_limited_section(output: &mut String, header: &str, lines: Vec<String>, budget: usize) {
    if lines.is_empty() || output.chars().count() >= budget {
        return;
    }
    let prefix = format!("{header}\n");
    let mut section = String::new();
    for line in lines {
        let candidate = format!("{line}\n");
        if output.chars().count()
            + prefix.chars().count()
            + section.chars().count()
            + candidate.chars().count()
            > budget
        {
            break;
        }
        section.push_str(&candidate);
    }
    if !section.is_empty() {
        output.push_str(&prefix);
        output.push_str(&section);
        output.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_smoke() -> Result<(), AgentError> {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-phase1-memory-smoke-{}.db",
            uuid::Uuid::new_v4()
        ));
        let path_string = path.to_string_lossy().into_owned();
        let store = MemoryStore::new(&path_string)?;

        let mut statement = store
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'view')")
            .map_err(database_error)?;
        let tables = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(database_error)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(database_error)?;
        for required in [
            "session_memory",
            "project_memory",
            "project_memory_fts",
            "global_memory",
            "open_questions",
            "model_reliability",
            "pattern_memory",
        ] {
            assert!(tables.contains(required), "missing table: {required}");
        }

        let journal_mode: String = store
            .conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .map_err(database_error)?;
        let user_version: i64 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(database_error)?;
        assert_eq!(journal_mode, "wal");
        assert!(user_version >= MEMORY_SCHEMA_VERSION);
        eprintln!("memory smoke database: {}", path.display());
        Ok(())
    }
}
