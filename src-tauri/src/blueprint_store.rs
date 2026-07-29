use chrono::Utc;
use rusqlite::{Connection, Error as SqliteError};
use serde::{Deserialize, Serialize};

use crate::errors::AgentError;

// ── Section status ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionStatus {
    Draft,
    Agreed,
    Negotiation,
    Disputed,
}

impl SectionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            SectionStatus::Draft       => "draft",
            SectionStatus::Agreed      => "agreed",
            SectionStatus::Negotiation => "negotiation",
            SectionStatus::Disputed    => "disputed",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "agreed"      => SectionStatus::Agreed,
            "negotiation" => SectionStatus::Negotiation,
            "disputed"    => SectionStatus::Disputed,
            _             => SectionStatus::Draft,
        }
    }
}

// ── Blueprint section ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintSection {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub content: String,
    pub status: SectionStatus,
    pub iteration_finalised: Option<u32>,
}

// ── Store ─────────────────────────────────────────────────────────────────────

pub struct BlueprintStore {
    conn: Connection,
}

impl BlueprintStore {
    pub fn new(db_path: &str) -> Result<Self, AgentError> {
        let conn = Connection::open(db_path).map_err(|e| {
            AgentError::DatabaseError(format!("Failed to open blueprint database: {}", e))
        })?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS blueprint_sections (
                id                  TEXT PRIMARY KEY,
                session_id          TEXT NOT NULL,
                title               TEXT NOT NULL,
                content             TEXT NOT NULL,
                status              TEXT NOT NULL DEFAULT 'draft',
                iteration_finalised INTEGER,
                created_at          INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_bp_session
                ON blueprint_sections (session_id);",
        )
        .map_err(|e| {
            AgentError::DatabaseError(format!("Failed to initialise blueprint table: {}", e))
        })?;

        Ok(BlueprintStore { conn })
    }

    /// Insert or replace a blueprint section.
    /// Takes &self because rusqlite::Connection::execute uses interior mutability.
    pub fn upsert_section(&self, section: &BlueprintSection) -> Result<(), AgentError> {
        let now = Utc::now().timestamp();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO blueprint_sections
                 (id, session_id, title, content, status, iteration_finalised, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    &section.id,
                    &section.session_id,
                    &section.title,
                    &section.content,
                    section.status.as_str(),
                    section.iteration_finalised,
                    now,
                ],
            )
            .map_err(|e| {
                AgentError::DatabaseError(format!("Failed to upsert blueprint section: {}", e))
            })?;
        Ok(())
    }

    /// Return all sections for a session in insertion order.
    /// Used by recover_session (IMP-7) to re-emit blueprint-section-added events.
    pub fn get_sections(&self, session_id: &str) -> Result<Vec<BlueprintSection>, AgentError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, title, content, status, iteration_finalised
                 FROM blueprint_sections
                 WHERE session_id = ?1
                 ORDER BY created_at ASC",
            )
            .map_err(|e| {
                AgentError::DatabaseError(format!("Failed to prepare get_sections: {}", e))
            })?;

        let rows = stmt
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<u32>>(5)?,
                ))
            })
            .map_err(|e| {
                AgentError::DatabaseError(format!("Failed to query sections: {}", e))
            })?;

        let mut sections = Vec::new();
        for row in rows {
            match row {
                Ok((id, sid, title, content, status_str, iter_fin)) => {
                    sections.push(BlueprintSection {
                        id,
                        session_id: sid,
                        title,
                        content,
                        status: SectionStatus::from_str(&status_str),
                        iteration_finalised: iter_fin,
                    });
                }
                Err(e) => {
                    tracing::warn!("[BLUEPRINT] Row error in get_sections: {}", e);
                }
            }
        }

        Ok(sections)
    }

    /// Export all agreed sections as a Markdown document.
    pub fn export_markdown(&self, session_id: &str) -> Result<String, AgentError> {
        let sections = self.get_sections(session_id)?;
        let mut out = String::new();
        for s in &sections {
            out.push_str(&format!("## {}\n\n{}\n\n", s.title, s.content));
        }
        Ok(out)
    }

    /// Export all agreed sections as plain text.
    pub fn export_plaintext(&self, session_id: &str) -> Result<String, AgentError> {
        let sections = self.get_sections(session_id)?;
        let mut out = String::new();
        for s in &sections {
            out.push_str(&format!("{}\n\n{}\n\n", s.title, s.content));
        }
        Ok(out)
    }

    /// Task 3 (CRIT-4): backs the `delete_session` command's blueprint-side
    /// cascade. Unlike `TranscriptStore::delete_session`, a session with zero
    /// blueprint sections is completely normal (e.g. deleted right after
    /// setup, before any section was agreed) — so this does NOT error on
    /// zero rows affected; zero is a valid, expected outcome here.
    pub fn delete_session_sections(&self, session_id: &str) -> Result<(), AgentError> {
        self.conn
            .execute(
                "DELETE FROM blueprint_sections WHERE session_id = ?1",
                rusqlite::params![session_id],
            )
            .map_err(|e| {
                AgentError::DatabaseError(format!(
                    "Failed to delete blueprint sections for session '{}': {}",
                    session_id, e
                ))
            })?;
        Ok(())
    }
}
