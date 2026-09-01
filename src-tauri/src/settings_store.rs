use rusqlite::{Connection, Error as SqliteError};
use chrono::Utc;
use crate::errors::AgentError;

pub struct SettingsStore {
    conn: Connection,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AgentBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub system_prompt: String,
    pub leader_priming_prompt: String,
    pub participant_priming_prompt: String,
}

/// D-039: Secondary (alternative) agent brain configuration.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SecondaryBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub system_prompt: String,
}

/// Task 5 (HIGH-3): D-038 fallback brain configuration. No `system_prompt`
/// field — the fallback always reuses the primary brain's system prompt
/// (see `agent_brain.rs::with_fallback`, which takes only api_key/base_url/
/// model and never touches `system_prompt`).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct FallbackBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl SettingsStore {
    pub fn new(db_path: &str) -> Result<Self, AgentError> {
        let conn = Connection::open(db_path).map_err(|e| {
            AgentError::DatabaseError(format!("Failed to open settings database: {}", e))
        })?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                key         TEXT PRIMARY KEY,
                value       TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );",
        )
        .map_err(|e| {
            AgentError::DatabaseError(format!("Failed to create settings table: {}", e))
        })?;

        Ok(SettingsStore { conn })
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, AgentError> {
        let result = self.conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(SqliteError::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AgentError::DatabaseError(format!(
                "Failed to get setting '{}': {}",
                key, e
            ))),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), AgentError> {
        let now = Utc::now().timestamp();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![key, value, now],
            )
            .map_err(|e| {
                AgentError::DatabaseError(format!("Failed to set setting '{}': {}", key, e))
            })?;
        Ok(())
    }

    // ── Primary agent brain ───────────────────────────────────────────────────

    pub fn get_agent_brain_config(&self) -> Result<AgentBrainConfig, AgentError> {
        let api_key = self.get("brain_api_key")?.unwrap_or_default();
        let base_url = self.get("brain_base_url")?.unwrap_or_default();
        let model = self.get("brain_model")?.unwrap_or_default();
        let system_prompt = self.get("brain_system_prompt")?.unwrap_or_default();
        let leader_priming_prompt = self.get("prompt_leader_priming")?.unwrap_or_default();
        let participant_priming_prompt = self.get("prompt_participant_priming")?.unwrap_or_default();

        Ok(AgentBrainConfig {
            api_key,
            base_url,
            model,
            system_prompt,
            leader_priming_prompt,
            participant_priming_prompt,
        })
    }

    pub fn save_agent_brain_config(&mut self, config: &AgentBrainConfig) -> Result<(), AgentError> {
        self.set("brain_api_key", &config.api_key)?;
        self.set("brain_base_url", &config.base_url)?;
        self.set("brain_model", &config.model)?;
        self.set("brain_system_prompt", &config.system_prompt)?;
        self.set("prompt_leader_priming", &config.leader_priming_prompt)?;
        self.set("prompt_participant_priming", &config.participant_priming_prompt)?;
        Ok(())
    }

    // ── D-038 / Task 5 (HIGH-3): Fallback brain ──────────────────────────────

    pub fn get_fallback_api_key(&self) -> Result<Option<String>, AgentError> {
        self.get("brain_fallback_api_key")
    }

    pub fn get_fallback_base_url(&self) -> Result<Option<String>, AgentError> {
        self.get("brain_fallback_base_url")
    }

    pub fn get_fallback_model(&self) -> Result<Option<String>, AgentError> {
        self.get("brain_fallback_model")
    }

    /// Task 5: previously these three keys had getters but no struct-level
    /// read/write pair and no command ever called a setter for them — the
    /// fallback feature's storage layer existed but was completely
    /// unreachable from the frontend. This mirrors the existing
    /// get/save_secondary_brain_config pattern exactly.
    pub fn get_fallback_brain_config(&self) -> Result<FallbackBrainConfig, AgentError> {
        Ok(FallbackBrainConfig {
            api_key: self.get_fallback_api_key()?.unwrap_or_default(),
            base_url: self.get_fallback_base_url()?.unwrap_or_default(),
            model: self.get_fallback_model()?.unwrap_or_default(),
        })
    }

    pub fn save_fallback_brain_config(
        &mut self,
        config: &FallbackBrainConfig,
    ) -> Result<(), AgentError> {
        self.set("brain_fallback_api_key", &config.api_key)?;
        self.set("brain_fallback_base_url", &config.base_url)?;
        self.set("brain_fallback_model", &config.model)?;
        Ok(())
    }

    // ── D-039: Secondary brain ────────────────────────────────────────────────

    pub fn get_secondary_brain_config(&self) -> Result<SecondaryBrainConfig, AgentError> {
        Ok(SecondaryBrainConfig {
            api_key: self.get("brain2_api_key")?.unwrap_or_default(),
            base_url: self.get("brain2_base_url")?.unwrap_or_default(),
            model: self.get("brain2_model")?.unwrap_or_default(),
            system_prompt: self.get("brain2_system_prompt")?.unwrap_or_default(),
        })
    }

    pub fn save_secondary_brain_config(
        &mut self,
        config: &SecondaryBrainConfig,
    ) -> Result<(), AgentError> {
        self.set("brain2_api_key", &config.api_key)?;
        self.set("brain2_base_url", &config.base_url)?;
        self.set("brain2_model", &config.model)?;
        self.set("brain2_system_prompt", &config.system_prompt)?;
        Ok(())
    }

    // ── P1: persisted custom participants ─────────────────────────────────────

    /// The `settings` table is a generic key→value store, so a persisted
    /// participant list needs no schema migration: it is stored as a single
    /// JSON array under the `custom_participants` key. Built-in participants
    /// (the static `AGENTS` registry in browser_backend) are never persisted
    /// here.
    pub fn get_custom_participants(&self) -> Result<Vec<CustomParticipant>, AgentError> {
        match self.get("custom_participants")? {
            Some(raw) => {
                if raw.trim().is_empty() {
                    return Ok(Vec::new());
                }
                serde_json::from_str(&raw).map_err(|e| {
                    AgentError::DatabaseError(format!(
                        "Failed to parse persisted custom participants: {}",
                        e
                    ))
                })
            }
            None => Ok(Vec::new()),
        }
    }

    /// Overwrite the entire persisted custom-participant list. Replaces the
    /// stored JSON array. Storing an empty list clears it.
    pub fn save_custom_participants(
        &mut self,
        participants: &[CustomParticipant],
    ) -> Result<(), AgentError> {
        let serialized = serde_json::to_string(participants).map_err(|e| {
            AgentError::DatabaseError(format!(
                "Failed to serialize custom participants: {}",
                e
            ))
        })?;
        self.set("custom_participants", &serialized)
    }
}

/// P1: a user-defined participant backed by a URL. Deliberately carries only
/// the fields the generic browser driver needs (id, display name, base URL).
/// Browser interaction (input/send/response) remains entirely generic; no
/// per-model strategy is stored here. The `agent_id` must not collide with a
/// built-in participant.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CustomParticipant {
    pub agent_id: String,
    pub display_name: String,
    pub base_url: String,
}

#[cfg(test)]
mod tests {
    use super::{CustomParticipant, SettingsStore};

    /// Unique temp DB path per call so parallel tests never open/remove the
    /// same SQLite file concurrently (a shared path caused flaky "readonly
    /// database"/"disk I/O error" failures when the suite ran in isolation).
    fn temp_store() -> (SettingsStore, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let unique = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-settings-p1-{}-{}.db",
            std::process::id(),
            unique
        ));
        let _ = std::fs::remove_file(&path);
        let store = SettingsStore::new(path.to_str().expect("temp path is utf8"))
            .expect("settings store opens");
        (store, path)
    }

    // P1: an empty / unset custom list reads back as an empty vec — no schema
    // migration required and no error from a missing key.
    #[test]
    fn unset_custom_participants_reads_empty() {
        let (store, _path) = temp_store();
        let list = store.get_custom_participants().expect("reads empty list");
        assert!(list.is_empty());
    }

    // P1: round-trip — persisted custom participants survive a fresh store open
    // (i.e. an app restart) reading the same database file.
    #[test]
    fn custom_participants_round_trip_reopen() {
        let (store, path) = temp_store();
        let mut store = store;
        let participants = vec![CustomParticipant {
            agent_id: "acme".to_string(),
            display_name: "Acme Bot".to_string(),
            base_url: "https://acme.example.com".to_string(),
        }];
        store
            .save_custom_participants(&participants)
            .expect("saves list");

        // Reopen the same file — simulates an app restart.
        let reopened = SettingsStore::new(path.to_str().expect("temp path is utf8"))
            .expect("reopens settings store");
        let loaded = reopened
            .get_custom_participants()
            .expect("reads saved list");
        assert_eq!(loaded, participants);
    }

    // P1: saving an empty list clears persisted custom participants.
    #[test]
    fn empty_save_clears_custom_participants() {
        let (mut store, _path) = temp_store();
        let participants = vec![CustomParticipant {
            agent_id: "acme".to_string(),
            display_name: "Acme Bot".to_string(),
            base_url: "https://acme.example.com".to_string(),
        }];
        store
            .save_custom_participants(&participants)
            .expect("saves list");
        store
            .save_custom_participants(&[])
            .expect("saves empty list");
        let loaded = store.get_custom_participants().expect("reads cleared list");
        assert!(loaded.is_empty());
    }
}
