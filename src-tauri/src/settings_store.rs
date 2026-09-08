use crate::errors::AgentError;
use chrono::Utc;
use rusqlite::{Connection, Error as SqliteError};

// ── Embedded prompt defaults — single authoritative source is the
// markdown files at the repository root. SettingsStore falls back to
// these when the DB key is missing/empty so a fresh install already
// runs the hardened beta prompts without manual seeding.
const DEFAULT_LEADER_PRIMING_RAW: &str = include_str!("../../leader_priming.md");
const DEFAULT_PARTICIPANT_PRIMING_RAW: &str = include_str!("../../participant_priming.md");
const DEFAULT_AGENT_SYSTEM_RAW: &str = include_str!("../../agent_system.md");

fn extract_prompt_body(raw: &str) -> String {
    // Files are `header\n---\nbody`. Return body only for model injection.
    if let Some(idx) = raw.find("\n---\n") {
        raw[idx + 5..].trim_start().to_string()
    } else if let Some(idx) = raw.find("---") {
        raw[idx + 3..].trim_start().to_string()
    } else {
        raw.trim().to_string()
    }
}

pub fn default_leader_priming() -> String {
    extract_prompt_body(DEFAULT_LEADER_PRIMING_RAW)
}

pub fn default_participant_priming() -> String {
    extract_prompt_body(DEFAULT_PARTICIPANT_PRIMING_RAW)
}

pub fn default_agent_system() -> String {
    extract_prompt_body(DEFAULT_AGENT_SYSTEM_RAW)
}

fn prompt_hash_for_log(s: &str) -> String {
    // Dev-safe: length + first 16 hex of SHA256, no prompt content leaked as full text.
    let digest = ring::digest::digest(&ring::digest::SHA256, s.as_bytes());
    let hex: String = digest
        .as_ref()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("len={} sha256={}...", s.len(), &hex[..16.min(hex.len())])
}

pub fn canonical_prompt_hashes() -> (String, String, String) {
    (
        prompt_hash_for_log(&default_leader_priming()),
        prompt_hash_for_log(&default_participant_priming()),
        prompt_hash_for_log(&default_agent_system()),
    )
}

fn is_canonical_leader_content(s: &str) -> bool {
    // Substantive markers that must appear in canonical leader (see §10)
    const MARKERS: &[&str] = &[
        "You are the leader of an expert AI panel assembled",
        "Runtime state is authoritative",
        "Route — consult one participant",
        "RouteCompare",
        "Ask User",
        "Hackathon Mode",
        "Phase 1",
        "Phase 2",
        "Independent judgment",
        "Handling disagreement",
        "Quality bar",
        "Global completion",
    ];
    MARKERS.iter().all(|m| s.contains(m))
}

fn is_canonical_participant_content(s: &str) -> bool {
    const MARKERS: &[&str] = &[
        "You are a reviewing member of an expert AI panel designing",
        "Runtime context is authoritative",
        "What you will be shown",
        "What is expected from your review",
        "Do independent research before objecting",
        "Reviewing Hackathon results",
        "Do not accept anything you don't actually believe",
        "The leader is not exempt",
        "If your objection is dismissed",
        "Ambiguity vs. low-value questions",
        "When product-vision questions come up",
        "Style",
    ];
    MARKERS.iter().all(|m| s.contains(m))
}

fn is_canonical_agent_content(s: &str) -> bool {
    const MARKERS: &[&str] = &[
        "You are the orchestration agent for an autonomous multi-model expert panel",
        "Roster is authoritative",
        "hackathon",
        "RouteCompare",
        "AskUser",
        "Continue",
        "Complete",
        "Route",
        "Blueprint",
    ];
    MARKERS.iter().all(|m| s.contains(m))
}

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

        let mut store = SettingsStore { conn };
        // Seed hardened beta prompts on first open so a fresh install already
        // has the finalized templates without requiring a manual Settings save.
        let seeds = [
            ("brain_system_prompt", default_agent_system()),
            ("prompt_leader_priming", default_leader_priming()),
            ("prompt_participant_priming", default_participant_priming()),
        ];
        for (key, value) in &seeds {
            if value.trim().is_empty() {
                continue;
            }
            match store.get(key) {
                Ok(Some(existing)) if !existing.trim().is_empty() => {
                    tracing::debug!(
                        "[PROMPT] seed skip {} existing {} canonical {}",
                        key,
                        prompt_hash_for_log(&existing),
                        prompt_hash_for_log(value)
                    );
                }
                Ok(_) => {
                    tracing::info!(
                        "[PROMPT] seed {} canonical {}",
                        key,
                        prompt_hash_for_log(value)
                    );
                    let _ = store.set(key, value);
                }
                Err(_) => {}
            }
        }
        let (canon_leader_hash, canon_part_hash, canon_system_hash) = canonical_prompt_hashes();
        tracing::info!(
            "[PROMPT] canonical hashes leader={} participant={} system={}",
            canon_leader_hash,
            canon_part_hash,
            canon_system_hash
        );

        // Migration: existing installs that already had the old factory defaults
        // (header present but without hardened markers, OR legacy short prompt with uppercase placeholders/hardcoded 7-model list)
        // must be upgraded to the hardened canonical prompts. User-custom prompts (no header and not matching legacy markers, or genuinely different content) are preserved.
        // This distinguishes:
        //   fresh install  -> seeded above (no existing key)
        //   old factory    -> header present, lacks hardened marker -> migrate
        //   legacy short   -> contains "small expert panel inside Consensus Arena" / "{{AGENTS}}" / hardcoded 7 list -> migrate (version 3 differential fix)
        //   custom user    -> no header and not legacy markers, or header plus custom content with marker? preserve
        // Versioned so future hardenings can bump PROMPT_HARDENING_VERSION.
        const PROMPT_HARDENING_VERSION: u32 = 4;
        const PROMPT_VERSION_KEY: &str = "prompt_hardening_version";
        let current_version = store
            .get(PROMPT_VERSION_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        if current_version < PROMPT_HARDENING_VERSION {
            fn is_legacy_short_leader(s: &str) -> bool {
                // Differential fix: the actual runtime injected short prompt (observed 2026-09-07) contains these.
                // Canonical uses lowercase {{project_brief}}, {{participant_count}} etc. Legacy uses uppercase {{PROJECT_BRIEF}}, {{AGENTS}} and hardcoded 7.
                s.contains("small expert panel inside Consensus Arena")
                    || s.contains("{{AGENTS}}")
                    || s.contains("{{PROJECT_BRIEF}}")
                    || s.contains("{{SESSION_TYPE}}")
                    || s.contains("Ask ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, or Kimi")
                    || s.contains("Available participants:")
                        && s.contains("ChatGPT")
                        && s.contains("Kimi")
                    || s.contains("For this test run:")
                    || s.contains("Finalized blueprint section:")
            }
            fn is_old_leader_factory(s: &str) -> bool {
                is_legacy_short_leader(s)
                    || (s.contains("leader_priming") && !s.contains("Runtime state is authoritative"))
                    || (s.contains("Runtime state is authoritative") && !s.contains("{{project_brief}}"))
                    // Very short prompts without header that are clearly not canonical (canonical is >8000 chars, contains Phase 1/2)
                    || (s.len() < 3000
                        && s.contains("You are the leader")
                        && !s.contains("You are the leader of an expert AI panel assembled"))
            }
            fn is_legacy_short_participant(s: &str) -> bool {
                // Old participant fallback (session_runner / context_manager short) and legacy short with uppercase placeholders
                s.contains("You are participating in a structured expert panel discussion.")
                    || s.contains("You may also be asked to review a Hackathon Mode result")
                        && s.len() < 3000 // canonical 9052, legacy short far shorter
                    || s.contains("{{AGENTS}}")
                    || s.contains("{{PROJECT_BRIEF}}")
                    || s.contains("For this test run:")
                    || s.contains("Available participants:")
                        && s.contains("ChatGPT")
                        && s.contains("Kimi")
                    || (s.len() < 3000
                        && s.contains("You are a reviewing member")
                        && !s.contains("You are a reviewing member of an expert AI panel designing"))
                    || (s.contains("Agent ID:") && s.contains("Project Brief:") && s.len() < 2000)
            }
            fn is_old_participant_factory(s: &str) -> bool {
                is_legacy_short_participant(s)
                    || (s.contains("participant_priming")
                        && !s.contains("Runtime context is authoritative"))
                    || (s.contains("Runtime context is authoritative")
                        && !s.contains("{{project_brief}}"))
            }
            fn is_legacy_short_agent(s: &str) -> bool {
                // Canonical agent_system is >8000 chars and contains hackathon + Roster authoritative + 12 classification rules.
                // Legacy short system prompt observed historically was <2000 chars and lacked those sections.
                let len = s.trim().len();
                len < 4000
                    && s.contains("orchestration agent")
                    && (!s.contains("hackathon") || !s.contains("Roster is authoritative"))
                    || s.contains("{{AGENTS}}")
                    || s.contains("You are the leader of a small expert panel")
            }
            fn is_old_agent_factory(s: &str) -> bool {
                is_legacy_short_agent(s)
                    || (s.contains("agent_system")
                        && (!s.contains("hackathon") || !s.contains("Roster is authoritative")))
            }
            let checks: &[(&str, fn(&str) -> bool, String)] = &[
                (
                    "prompt_leader_priming",
                    is_old_leader_factory,
                    default_leader_priming(),
                ),
                (
                    "prompt_participant_priming",
                    is_old_participant_factory,
                    default_participant_priming(),
                ),
                (
                    "brain_system_prompt",
                    is_old_agent_factory,
                    default_agent_system(),
                ),
            ];
            for (key, is_old, new_val) in checks {
                if let Ok(Some(stored)) = store.get(key) {
                    let is_old_flag = is_old(&stored);
                    let stored_hash = prompt_hash_for_log(&stored);
                    let canonical_hash = prompt_hash_for_log(new_val);
                    let is_canonical = is_canonical_leader_content(&stored)
                        || stored.contains("You are a reviewing member of an expert AI panel")
                        || stored.contains("You are the orchestration agent for an autonomous");
                    tracing::info!(
                        "[PROMPT] migration check key={} is_old={} is_canonical_markers={} stored={} canonical={}",
                        key,
                        is_old_flag,
                        is_canonical,
                        stored_hash,
                        canonical_hash
                    );
                    if is_old_flag {
                        tracing::warn!(
                            "[PROMPT] migrating {} legacy→canonical stored={} canonical={}",
                            key,
                            stored_hash,
                            canonical_hash
                        );
                        let _ = store.set(key, new_val);
                    }
                }
            }
            tracing::info!(
                "[PROMPT] migration version {}→{} complete canonical leader={} participant={} system={}",
                current_version,
                PROMPT_HARDENING_VERSION,
                prompt_hash_for_log(&default_leader_priming()),
                prompt_hash_for_log(&default_participant_priming()),
                prompt_hash_for_log(&default_agent_system())
            );
            let _ = store.set(PROMPT_VERSION_KEY, &PROMPT_HARDENING_VERSION.to_string());
        } else {
            tracing::debug!(
                "[PROMPT] migration skipped current_version={} target={}",
                current_version,
                PROMPT_HARDENING_VERSION
            );
        }

        Ok(store)
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, AgentError> {
        let result =
            self.conn
                .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                    row.get::<_, String>(0)
                });

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
        let mut system_prompt = self.get("brain_system_prompt")?.unwrap_or_default();
        if system_prompt.trim().is_empty() {
            system_prompt = default_agent_system();
        }
        let mut leader_priming_prompt = self.get("prompt_leader_priming")?.unwrap_or_default();
        if leader_priming_prompt.trim().is_empty() {
            leader_priming_prompt = default_leader_priming();
        }
        let mut participant_priming_prompt =
            self.get("prompt_participant_priming")?.unwrap_or_default();
        if participant_priming_prompt.trim().is_empty() {
            participant_priming_prompt = default_participant_priming();
        }

        Ok(AgentBrainConfig {
            api_key,
            base_url,
            model,
            system_prompt,
            leader_priming_prompt,
            participant_priming_prompt,
        })
    }

    /// Raw accessor for a single prompt template with embedded fallback.
    /// Used by `get_prompt_template` command so the Settings UI shows the
    /// hardened defaults on first open without requiring a manual save.
    pub fn get_prompt_template_with_default(&self, key: &str) -> Result<String, AgentError> {
        let raw = self.get(key)?.unwrap_or_default();
        if !raw.trim().is_empty() {
            return Ok(raw);
        }
        Ok(match key {
            "brain_system_prompt" => default_agent_system(),
            "prompt_leader_priming" => default_leader_priming(),
            "prompt_participant_priming" => default_participant_priming(),
            _ => String::new(),
        })
    }

    pub fn save_agent_brain_config(&mut self, config: &AgentBrainConfig) -> Result<(), AgentError> {
        self.set("brain_api_key", &config.api_key)?;
        self.set("brain_base_url", &config.base_url)?;
        self.set("brain_model", &config.model)?;
        self.set("brain_system_prompt", &config.system_prompt)?;
        self.set("prompt_leader_priming", &config.leader_priming_prompt)?;
        self.set(
            "prompt_participant_priming",
            &config.participant_priming_prompt,
        )?;
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
            AgentError::DatabaseError(format!("Failed to serialize custom participants: {}", e))
        })?;
        self.set("custom_participants", &serialized)
    }

    // ── Maintenance mode (Diagnostics gate) ─────────────────────────────────

    /// Returns true only when the user has explicitly enabled Maintenance mode.
    /// Missing key or any value other than "true" is treated as OFF (default).
    pub fn get_maintenance_mode(&self) -> Result<bool, AgentError> {
        match self.get("maintenance_mode")? {
            Some(value) => Ok(value == "true"),
            None => Ok(false),
        }
    }

    pub fn set_maintenance_mode(&mut self, enabled: bool) -> Result<(), AgentError> {
        self.set("maintenance_mode", if enabled { "true" } else { "false" })
    }

    // ── Hackathon Mode ──────────────────────────────────────────────────────

    /// Load hackathon config from settings.db key `hackathon_config`.
    /// Missing or empty value returns default config.
    pub fn get_hackathon_config(&self) -> Result<crate::hackathon::HackathonConfig, AgentError> {
        match self.get("hackathon_config")? {
            Some(raw) if !raw.trim().is_empty() => serde_json::from_str(&raw).map_err(|e| {
                AgentError::DatabaseError(format!("Failed to parse hackathon config: {}", e))
            }),
            _ => Ok(crate::hackathon::HackathonConfig::default()),
        }
    }

    pub fn save_hackathon_config(
        &mut self,
        config: &crate::hackathon::HackathonConfig,
    ) -> Result<(), AgentError> {
        let serialized = serde_json::to_string(config).map_err(|e| {
            AgentError::DatabaseError(format!("Failed to serialize hackathon config: {}", e))
        })?;
        self.set("hackathon_config", &serialized)
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
