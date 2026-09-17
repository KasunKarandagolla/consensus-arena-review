#[cfg(not(test))]
use crate::credentials::OsCredentialStore;
use crate::credentials::{CredentialError, CredentialStore, secure_storage_help};
use crate::errors::AgentError;
use chrono::Utc;
use rusqlite::{Connection, Error as SqliteError};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serializer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    credentials: Arc<dyn CredentialStore>,
    credential_storage_available: Arc<AtomicBool>,
    credential_migration_pending: Arc<AtomicBool>,
}

#[derive(Deserialize)]
pub struct AgentBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub system_prompt: String,
    pub leader_priming_prompt: String,
    pub participant_priming_prompt: String,
    #[serde(default)]
    pub credential_storage_available: bool,
    #[serde(default)]
    pub credential_migration_pending: bool,
}

impl serde::Serialize for AgentBrainConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AgentBrainConfig", 9)?;
        state.serialize_field("api_key", "")?;
        state.serialize_field("api_key_configured", &!self.api_key.is_empty())?;
        state.serialize_field("base_url", &self.base_url)?;
        state.serialize_field("model", &self.model)?;
        state.serialize_field("system_prompt", &self.system_prompt)?;
        state.serialize_field("leader_priming_prompt", &self.leader_priming_prompt)?;
        state.serialize_field(
            "participant_priming_prompt",
            &self.participant_priming_prompt,
        )?;
        state.serialize_field(
            "credential_storage_available",
            &self.credential_storage_available,
        )?;
        state.serialize_field(
            "credential_migration_pending",
            &self.credential_migration_pending,
        )?;
        state.end()
    }
}

/// D-039: Secondary (alternative) agent brain configuration.
#[derive(Deserialize)]
pub struct SecondaryBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub system_prompt: String,
    #[serde(default)]
    pub credential_storage_available: bool,
    #[serde(default)]
    pub credential_migration_pending: bool,
}

impl serde::Serialize for SecondaryBrainConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("SecondaryBrainConfig", 7)?;
        state.serialize_field("api_key", "")?;
        state.serialize_field("api_key_configured", &!self.api_key.is_empty())?;
        state.serialize_field("base_url", &self.base_url)?;
        state.serialize_field("model", &self.model)?;
        state.serialize_field("system_prompt", &self.system_prompt)?;
        state.serialize_field(
            "credential_storage_available",
            &self.credential_storage_available,
        )?;
        state.serialize_field(
            "credential_migration_pending",
            &self.credential_migration_pending,
        )?;
        state.end()
    }
}

/// Task 5 (HIGH-3): D-038 fallback brain configuration. No `system_prompt`
/// field — the fallback always reuses the primary brain's system prompt
/// (see `agent_brain.rs::with_fallback`, which takes only api_key/base_url/
/// model and never touches `system_prompt`).
#[derive(Deserialize)]
pub struct FallbackBrainConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub credential_storage_available: bool,
    #[serde(default)]
    pub credential_migration_pending: bool,
}

impl serde::Serialize for FallbackBrainConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("FallbackBrainConfig", 6)?;
        state.serialize_field("api_key", "")?;
        state.serialize_field("api_key_configured", &!self.api_key.is_empty())?;
        state.serialize_field("base_url", &self.base_url)?;
        state.serialize_field("model", &self.model)?;
        state.serialize_field(
            "credential_storage_available",
            &self.credential_storage_available,
        )?;
        state.serialize_field(
            "credential_migration_pending",
            &self.credential_migration_pending,
        )?;
        state.end()
    }
}

impl SettingsStore {
    pub fn new(db_path: &str) -> Result<Self, AgentError> {
        #[cfg(test)]
        {
            return Self::new_with_credential_store(
                db_path,
                Arc::new(crate::credentials::MemoryCredentialStore::default()),
            );
        }
        #[cfg(not(test))]
        Self::new_with_credential_store(db_path, Arc::new(OsCredentialStore))
    }

    pub fn new_with_credential_store(
        db_path: &str,
        credentials: Arc<dyn CredentialStore>,
    ) -> Result<Self, AgentError> {
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

        let mut store = SettingsStore {
            conn,
            credentials,
            credential_storage_available: Arc::new(AtomicBool::new(true)),
            credential_migration_pending: Arc::new(AtomicBool::new(false)),
        };
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

        store.migrate_legacy_credentials();
        Ok(store)
    }

    fn security_error() -> AgentError {
        AgentError::DatabaseError(secure_storage_help().to_string())
    }

    fn is_direct_credential(key: &str) -> bool {
        matches!(
            key,
            "brain_api_key" | "brain_fallback_api_key" | "brain2_api_key"
        )
    }

    fn get_plain(&self, key: &str) -> Result<Option<String>, AgentError> {
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

    fn set_plain(&mut self, key: &str, value: &str) -> Result<(), AgentError> {
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

    fn restore_secret_value(&mut self, account: &str, value: Option<&str>) {
        let result = match value {
            Some(value) => self.credentials.set(account, value),
            None => self.credentials.delete(account),
        };
        if result.is_err() {
            self.credential_storage_available
                .store(false, Ordering::Relaxed);
            self.credential_migration_pending
                .store(true, Ordering::Relaxed);
        }
    }

    fn save_secret_fields(
        &mut self,
        account: &str,
        secret: &str,
        clear_secret: bool,
        fields: &[(&str, &str)],
    ) -> Result<(), AgentError> {
        let previous = self.get(account)?;
        if clear_secret {
            self.clear_credential(account)?;
        } else if !secret.is_empty() {
            self.set_secret(account, secret)?;
        }
        let persisted = (|| {
            let tx = self.conn.transaction().map_err(|error| {
                AgentError::DatabaseError(format!("Failed to save credential metadata: {error}"))
            })?;
            for (key, value) in fields {
                tx.execute(
                    "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![key, value, Utc::now().timestamp()],
                )
                .map_err(|error| {
                    AgentError::DatabaseError(format!(
                        "Failed to save credential metadata: {error}"
                    ))
                })?;
            }
            tx.commit().map_err(|error| {
                AgentError::DatabaseError(format!("Failed to save credential metadata: {error}"))
            })
        })();
        if let Err(error) = persisted {
            self.restore_secret_value(account, previous.as_deref());
            return Err(error);
        }
        Ok(())
    }

    fn set_secret(&mut self, account: &str, secret: &str) -> Result<(), AgentError> {
        if secret.is_empty() {
            return Ok(());
        }
        if self.credential_migration_pending() {
            self.migrate_legacy_credentials();
            if self.credential_migration_pending() {
                return Err(Self::security_error());
            }
        }
        let previous = self.credentials.get(account).map_err(|_| {
            self.credential_storage_available
                .store(false, Ordering::Relaxed);
            self.credential_migration_pending
                .store(true, Ordering::Relaxed);
            Self::security_error()
        })?;
        if self.credentials.set(account, secret).is_err() {
            self.restore_secret_value(account, previous.as_deref());
            self.credential_storage_available
                .store(false, Ordering::Relaxed);
            self.credential_migration_pending
                .store(true, Ordering::Relaxed);
            return Err(Self::security_error());
        }
        match self.credentials.get(account) {
            Ok(Some(stored)) if stored == secret => {}
            _ => {
                self.restore_secret_value(account, previous.as_deref());
                self.credential_storage_available
                    .store(false, Ordering::Relaxed);
                self.credential_migration_pending
                    .store(true, Ordering::Relaxed);
                return Err(Self::security_error());
            }
        }
        self.credential_storage_available
            .store(true, Ordering::Relaxed);
        if self.remove_legacy_rows(&[account], None).is_err() {
            self.restore_secret_value(account, previous.as_deref());
            self.credential_migration_pending
                .store(true, Ordering::Relaxed);
            return Err(Self::security_error());
        }
        self.migrate_legacy_credentials();
        Ok(())
    }

    fn remove_legacy_rows(
        &mut self,
        keys: &[&str],
        sanitized_hackathon: Option<&str>,
    ) -> Result<(), AgentError> {
        let rows_to_remove = keys
            .iter()
            .map(|key| self.get_plain(key).map(|value| value.is_some()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .any(|exists| exists);
        let hackathon_changed = sanitized_hackathon.is_some();
        if !rows_to_remove && !hackathon_changed {
            return Ok(());
        }
        self.conn
            .execute_batch("PRAGMA secure_delete = ON;")
            .map_err(|_| Self::security_error())?;
        let tx = self
            .conn
            .transaction()
            .map_err(|_| Self::security_error())?;
        for key in keys {
            tx.execute("DELETE FROM settings WHERE key = ?1", [key])
                .map_err(|_| Self::security_error())?;
        }
        if let Some(config) = sanitized_hackathon {
            tx.execute(
                "UPDATE settings SET value = ?1, updated_at = ?2 WHERE key = 'hackathon_config'",
                rusqlite::params![config, Utc::now().timestamp()],
            )
            .map_err(|_| Self::security_error())?;
        }
        tx.commit().map_err(|_| Self::security_error())?;
        if rows_to_remove || hackathon_changed {
            // secure_delete is enabled before the delete transaction, so a
            // failed compaction cannot restore plaintext rows or lose the
            // already read-back-validated keyring copy.
            let _ = self.conn.execute_batch("VACUUM;");
        }
        Ok(())
    }

    fn restore_credentials(&mut self, previous: &[(String, Option<String>)]) {
        for (account, secret) in previous.iter().rev() {
            self.restore_secret_value(account, secret.as_deref());
        }
    }

    fn migrate_legacy_credentials(&mut self) {
        let mut secrets = Vec::<(String, String)>::new();
        for key in ["brain_api_key", "brain_fallback_api_key", "brain2_api_key"] {
            match self.get_plain(key) {
                Ok(Some(value)) if !value.is_empty() => {
                    secrets.push((key.to_string(), value));
                }
                Ok(_) => {}
                Err(_) => {
                    self.credential_migration_pending
                        .store(true, Ordering::Relaxed);
                    self.credential_storage_available
                        .store(false, Ordering::Relaxed);
                    return;
                }
            }
        }

        let mut sanitized_hackathon = None;
        match self.get_plain("hackathon_config") {
            Ok(Some(raw)) if !raw.trim().is_empty() => {
                let mut config =
                    match serde_json::from_str::<crate::hackathon::HackathonConfig>(&raw) {
                        Ok(config) => config,
                        Err(_) => {
                            self.credential_migration_pending
                                .store(true, Ordering::Relaxed);
                            self.credential_storage_available
                                .store(false, Ordering::Relaxed);
                            return;
                        }
                    };
                let mut had_legacy_key = false;
                for model in &mut config.models {
                    if !model.api_key.is_empty() {
                        had_legacy_key = true;
                        secrets.push((
                            format!("hackathon.model.{}", model.id),
                            model.api_key.clone(),
                        ));
                        model.api_key.clear();
                    }
                }
                if had_legacy_key {
                    sanitized_hackathon = serde_json::to_string(&config).ok();
                    if sanitized_hackathon.is_none() {
                        self.credential_migration_pending
                            .store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
            Ok(_) => {}
            Err(_) => {
                self.credential_migration_pending
                    .store(true, Ordering::Relaxed);
                self.credential_storage_available
                    .store(false, Ordering::Relaxed);
                return;
            }
        }

        let mut previous_credentials = Vec::<(String, Option<String>)>::new();
        for (account, secret) in &secrets {
            let previous = match self.credentials.get(account) {
                Ok(previous) => previous,
                Err(_) => {
                    self.restore_credentials(&previous_credentials);
                    self.credential_storage_available
                        .store(false, Ordering::Relaxed);
                    self.credential_migration_pending
                        .store(true, Ordering::Relaxed);
                    return;
                }
            };
            previous_credentials.push((account.clone(), previous));
            if self.credentials.set(account, secret).is_err()
                || !matches!(self.credentials.get(account), Ok(Some(value)) if value == *secret)
            {
                self.restore_credentials(&previous_credentials);
                self.credential_storage_available
                    .store(false, Ordering::Relaxed);
                self.credential_migration_pending
                    .store(true, Ordering::Relaxed);
                return;
            }
        }

        if !secrets.is_empty() {
            let secret_keys = ["brain_api_key", "brain_fallback_api_key", "brain2_api_key"];
            if self
                .remove_legacy_rows(&secret_keys, sanitized_hackathon.as_deref())
                .is_err()
            {
                self.restore_credentials(&previous_credentials);
                self.credential_migration_pending
                    .store(true, Ordering::Relaxed);
                return;
            }
        }

        if self
            .set_plain("secure_credentials_migrated_v1", "true")
            .is_err()
        {
            self.credential_migration_pending
                .store(true, Ordering::Relaxed);
            return;
        }
        match self.credentials.get("brain_api_key") {
            Ok(_) => {
                self.credential_storage_available
                    .store(true, Ordering::Relaxed);
                self.credential_migration_pending
                    .store(false, Ordering::Relaxed);
            }
            Err(CredentialError::Unavailable) => {
                self.credential_storage_available
                    .store(false, Ordering::Relaxed);
                self.credential_migration_pending
                    .store(!secrets.is_empty(), Ordering::Relaxed);
            }
        }
    }

    pub fn credential_storage_available(&self) -> bool {
        self.credential_storage_available.load(Ordering::Relaxed)
    }

    pub fn credential_migration_pending(&self) -> bool {
        self.credential_migration_pending.load(Ordering::Relaxed)
    }

    pub fn clear_credential(&mut self, account: &str) -> Result<(), AgentError> {
        let previous = self.credentials.get(account).map_err(|_| {
            self.credential_storage_available
                .store(false, Ordering::Relaxed);
            Self::security_error()
        })?;
        if self.credentials.delete(account).is_err() {
            self.restore_secret_value(account, previous.as_deref());
            self.credential_storage_available
                .store(false, Ordering::Relaxed);
            return Err(Self::security_error());
        }
        let legacy_key = if Self::is_direct_credential(account) {
            Some(account)
        } else {
            None
        };
        if let Some(key) = legacy_key {
            if self.remove_legacy_rows(&[key], None).is_err() {
                self.restore_secret_value(account, previous.as_deref());
                self.credential_migration_pending
                    .store(true, Ordering::Relaxed);
                return Err(Self::security_error());
            }
        }
        self.credential_storage_available
            .store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, AgentError> {
        if Self::is_direct_credential(key) {
            match self.credentials.get(key) {
                Ok(Some(secret)) => return Ok(Some(secret)),
                Ok(None) => match self.get_plain(key)? {
                    Some(_) => {
                        self.credential_migration_pending
                            .store(true, Ordering::Relaxed);
                        return Err(Self::security_error());
                    }
                    None => return Ok(None),
                },
                Err(CredentialError::Unavailable) => {
                    self.credential_storage_available
                        .store(false, Ordering::Relaxed);
                    if let Some(legacy) = self.get_plain(key)? {
                        self.credential_migration_pending
                            .store(true, Ordering::Relaxed);
                        return Ok(Some(legacy));
                    }
                    self.credential_migration_pending
                        .store(true, Ordering::Relaxed);
                    return Err(Self::security_error());
                }
            }
        }
        self.get_plain(key)
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), AgentError> {
        if Self::is_direct_credential(key) {
            return self.set_secret(key, value);
        }
        self.set_plain(key, value)
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
            credential_storage_available: self.credential_storage_available(),
            credential_migration_pending: self.credential_migration_pending(),
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
        self.save_secret_fields(
            "brain_api_key",
            &config.api_key,
            false,
            &[
                ("brain_base_url", &config.base_url),
                ("brain_model", &config.model),
                ("brain_system_prompt", &config.system_prompt),
                ("prompt_leader_priming", &config.leader_priming_prompt),
                (
                    "prompt_participant_priming",
                    &config.participant_priming_prompt,
                ),
            ],
        )
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
            credential_storage_available: self.credential_storage_available(),
            credential_migration_pending: self.credential_migration_pending(),
        })
    }

    pub fn save_fallback_brain_config(
        &mut self,
        config: &FallbackBrainConfig,
    ) -> Result<(), AgentError> {
        self.save_secret_fields(
            "brain_fallback_api_key",
            &config.api_key,
            config.api_key.trim().is_empty()
                && config.base_url.trim().is_empty()
                && config.model.trim().is_empty(),
            &[
                ("brain_fallback_base_url", &config.base_url),
                ("brain_fallback_model", &config.model),
            ],
        )
    }

    // ── D-039: Secondary brain ────────────────────────────────────────────────

    pub fn get_secondary_brain_config(&self) -> Result<SecondaryBrainConfig, AgentError> {
        Ok(SecondaryBrainConfig {
            api_key: self.get("brain2_api_key")?.unwrap_or_default(),
            base_url: self.get("brain2_base_url")?.unwrap_or_default(),
            model: self.get("brain2_model")?.unwrap_or_default(),
            system_prompt: self.get("brain2_system_prompt")?.unwrap_or_default(),
            credential_storage_available: self.credential_storage_available(),
            credential_migration_pending: self.credential_migration_pending(),
        })
    }

    pub fn save_secondary_brain_config(
        &mut self,
        config: &SecondaryBrainConfig,
    ) -> Result<(), AgentError> {
        self.save_secret_fields(
            "brain2_api_key",
            &config.api_key,
            false,
            &[
                ("brain2_base_url", &config.base_url),
                ("brain2_model", &config.model),
                ("brain2_system_prompt", &config.system_prompt),
            ],
        )
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
        let mut config = match self.get("hackathon_config")? {
            Some(raw) if !raw.trim().is_empty() => match serde_json::from_str(&raw) {
                Ok(config) => config,
                Err(_) => {
                    self.credential_storage_available
                        .store(false, Ordering::Relaxed);
                    self.credential_migration_pending
                        .store(true, Ordering::Relaxed);
                    return Err(Self::security_error());
                }
            },
            _ => crate::hackathon::HackathonConfig::default(),
        };
        for model in &mut config.models {
            let account = format!("hackathon.model.{}", model.id);
            match self.credentials.get(&account) {
                Ok(Some(secret)) => model.api_key = secret,
                Ok(None) if model.api_key.trim().is_empty() => {}
                Ok(None) => {
                    self.credential_migration_pending
                        .store(true, Ordering::Relaxed);
                    return Err(Self::security_error());
                }
                Err(_) => {
                    self.credential_storage_available
                        .store(false, Ordering::Relaxed);
                    self.credential_migration_pending
                        .store(true, Ordering::Relaxed);
                    return Err(Self::security_error());
                }
            }
        }
        Ok(config)
    }

    pub fn save_hackathon_config(
        &mut self,
        config: &crate::hackathon::HackathonConfig,
    ) -> Result<(), AgentError> {
        if self.credential_migration_pending() {
            self.migrate_legacy_credentials();
            if self.credential_migration_pending() {
                return Err(Self::security_error());
            }
        }
        let existing_raw = self.get_plain("hackathon_config")?;
        let existing = self.get_hackathon_config()?;
        let existing_secrets = existing
            .models
            .iter()
            .map(|model| (model.id.clone(), model.api_key.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let mut persisted = config.clone();
        for model in &mut persisted.models {
            if model.api_key.trim().is_empty() {
                model.api_key = existing_secrets.get(&model.id).cloned().unwrap_or_default();
            }
        }
        let retained_ids = persisted
            .models
            .iter()
            .map(|model| model.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let mut changed = Vec::<(String, Option<String>)>::new();
        let mut removed = Vec::<(String, String)>::new();
        for model in &mut persisted.models {
            if model.api_key.is_empty() {
                continue;
            }
            let account = format!("hackathon.model.{}", model.id);
            let previous = self.credentials.get(&account).map_err(|_| {
                self.credential_storage_available
                    .store(false, Ordering::Relaxed);
                self.credential_migration_pending
                    .store(true, Ordering::Relaxed);
                Self::security_error()
            })?;
            changed.push((account.clone(), previous.clone()));
            if self.credentials.set(&account, &model.api_key).is_err()
                || !matches!(self.credentials.get(&account), Ok(Some(value)) if value == model.api_key)
            {
                self.restore_credentials(&changed);
                self.credential_storage_available
                    .store(false, Ordering::Relaxed);
                self.credential_migration_pending
                    .store(true, Ordering::Relaxed);
                return Err(Self::security_error());
            }
            model.api_key.clear();
        }
        let serialized = serde_json::to_string(&persisted).map_err(|e| {
            AgentError::DatabaseError(format!("Failed to serialize hackathon config: {}", e))
        });
        let serialized = match serialized {
            Ok(serialized) => serialized,
            Err(error) => {
                self.restore_credentials(&changed);
                return Err(error);
            }
        };
        if let Err(error) = self.set("hackathon_config", &serialized) {
            self.restore_credentials(&changed);
            return Err(error);
        }
        for model in &existing.models {
            if !retained_ids.contains(&model.id) {
                let account = format!("hackathon.model.{}", model.id);
                if !model.api_key.is_empty() {
                    removed.push((account.clone(), model.api_key.clone()));
                }
                if self.credentials.delete(&account).is_err() {
                    let removed_credentials = removed
                        .iter()
                        .map(|(account, secret)| (account.clone(), Some(secret.clone())))
                        .collect::<Vec<_>>();
                    self.restore_credentials(&removed_credentials);
                    self.restore_credentials(&changed);
                    let metadata_restored = match existing_raw.as_deref() {
                        Some(raw) => self.set_plain("hackathon_config", raw).is_ok(),
                        None => self
                            .conn
                            .execute("DELETE FROM settings WHERE key = 'hackathon_config'", [])
                            .is_ok(),
                    };
                    if !metadata_restored {
                        self.credential_storage_available
                            .store(false, Ordering::Relaxed);
                        self.credential_migration_pending
                            .store(true, Ordering::Relaxed);
                    }
                    self.credential_migration_pending
                        .store(true, Ordering::Relaxed);
                    return Err(Self::security_error());
                }
            }
        }
        self.migrate_legacy_credentials();
        Ok(())
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
    use crate::credentials::{CredentialError, CredentialStore, MemoryCredentialStore};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

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
        let store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            std::sync::Arc::new(crate::credentials::MemoryCredentialStore::default()),
        )
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
        let reopened = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            std::sync::Arc::new(crate::credentials::MemoryCredentialStore::default()),
        )
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

    #[test]
    fn legacy_agent_key_moves_to_credential_store_and_never_serializes_back() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-credential-migration-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let legacy_secret = "credential-migration-sentinel-do-not-log";
        let fallback_secret = "fallback-migration-sentinel-do-not-log";
        let secondary_secret = "secondary-migration-sentinel-do-not-log";
        let hackathon_secret = "hackathon-migration-sentinel-do-not-log";
        let legacy_hackathon = crate::hackathon::HackathonConfig {
            groups: Vec::new(),
            models: vec![crate::hackathon::HackathonModelConfig {
                id: "legacy-model-one".to_string(),
                model_name: "legacy model".to_string(),
                base_url: "https://api.example.test/v1".to_string(),
                api_key: hackathon_secret.to_string(),
                group_id: "legacy-group".to_string(),
            }],
            max_questions_per_teammate: Some(3),
            enabled: false,
        };
        {
            let conn = rusqlite::Connection::open(&path).expect("open test database");
            conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);").expect("create settings table");
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('brain_api_key', ?1, 1)",
                [legacy_secret],
            )
            .expect("seed legacy key");
            conn.execute("INSERT INTO settings (key, value, updated_at) VALUES ('brain_fallback_api_key', ?1, 1)", [fallback_secret]).expect("seed fallback key");
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('brain2_api_key', ?1, 1)",
                [secondary_secret],
            )
            .expect("seed secondary key");
            conn.execute("INSERT INTO settings (key, value, updated_at) VALUES ('brain_base_url', 'https://api.example.test/v1', 1)", []).expect("seed base URL");
            conn.execute("INSERT INTO settings (key, value, updated_at) VALUES ('brain_model', 'test-model', 1)", []).expect("seed model");
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('hackathon_config', ?1, 1)",
                [serde_json::to_string(&legacy_hackathon).expect("serialize legacy hackathon")],
            )
            .expect("seed legacy Hackathon key");
        }
        let credentials = Arc::new(MemoryCredentialStore::default());
        let store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            credentials.clone(),
        )
        .expect("migrate settings");
        assert_eq!(
            credentials
                .get("brain_api_key")
                .expect("keyring read")
                .as_deref(),
            Some(legacy_secret)
        );
        for (account, secret) in [
            ("brain_fallback_api_key", fallback_secret),
            ("brain2_api_key", secondary_secret),
            ("hackathon.model.legacy-model-one", hackathon_secret),
        ] {
            assert!(matches!(credentials.get(account), Ok(Some(value)) if value == secret));
        }
        let config = store
            .get_agent_brain_config()
            .expect("read migrated config");
        let serialized = serde_json::to_string(&config).expect("serialize safe config");
        assert!(!serialized.contains(legacy_secret));
        assert!(serialized.contains("\"api_key\":\"\""));
        assert!(serialized.contains("\"api_key_configured\":true"));
        let fallback_json = serde_json::to_string(
            &store
                .get_fallback_brain_config()
                .expect("read migrated fallback config"),
        )
        .expect("serialize safe fallback config");
        let secondary_json = serde_json::to_string(
            &store
                .get_secondary_brain_config()
                .expect("read migrated secondary config"),
        )
        .expect("serialize safe secondary config");
        for (json, secret) in [
            (&fallback_json, fallback_secret),
            (&secondary_json, secondary_secret),
        ] {
            assert!(!json.contains(secret));
            assert!(json.contains("\"api_key\":\"\""));
            assert!(json.contains("\"api_key_configured\":true"));
        }
        let rows_with_secret: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE instr(value, ?1) > 0",
                [legacy_secret],
                |row| row.get(0),
            )
            .expect("scan settings rows");
        assert_eq!(rows_with_secret, 0);
        for secret in [fallback_secret, secondary_secret, hackathon_secret] {
            let count: i64 = store
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM settings WHERE instr(value, ?1) > 0",
                    [secret],
                    |row| row.get(0),
                )
                .expect("scan migrated settings rows");
            assert_eq!(count, 0);
        }
        let raw_database = std::fs::read(&path).expect("read migrated SQLite file");
        for secret in [
            legacy_secret,
            fallback_secret,
            secondary_secret,
            hackathon_secret,
        ] {
            assert!(
                !raw_database
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes()),
                "migrated plaintext secret remains in the SQLite file"
            );
        }
        let loaded_hackathon = store
            .get_hackathon_config()
            .expect("read migrated Hackathon config");
        assert_eq!(loaded_hackathon.models[0].api_key, hackathon_secret);
        let serialized_hackathon = store
            .get_plain("hackathon_config")
            .expect("read safe Hackathon storage")
            .expect("Hackathon config exists");
        assert!(!serialized_hackathon.contains(hackathon_secret));
        assert!(!store.credential_migration_pending());
    }

    struct UnavailableCredentialStore;

    impl CredentialStore for UnavailableCredentialStore {
        fn get(&self, _account: &str) -> Result<Option<String>, CredentialError> {
            Err(CredentialError::Unavailable)
        }
        fn set(&self, _account: &str, _secret: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Unavailable)
        }
        fn delete(&self, _account: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Unavailable)
        }
    }

    #[derive(Default)]
    struct ToggleCredentialStore {
        available: AtomicBool,
        entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
    }

    impl CredentialStore for ToggleCredentialStore {
        fn get(&self, account: &str) -> Result<Option<String>, CredentialError> {
            if !self.available.load(Ordering::Relaxed) {
                return Err(CredentialError::Unavailable);
            }
            self.entries
                .lock()
                .map(|entries| entries.get(account).cloned())
                .map_err(|_| CredentialError::Unavailable)
        }

        fn set(&self, account: &str, secret: &str) -> Result<(), CredentialError> {
            if !self.available.load(Ordering::Relaxed) {
                return Err(CredentialError::Unavailable);
            }
            self.entries
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), CredentialError> {
            if !self.available.load(Ordering::Relaxed) {
                return Err(CredentialError::Unavailable);
            }
            self.entries
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .remove(account);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailAfterOneCredentialWrite {
        entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
        writes: std::sync::atomic::AtomicUsize,
    }

    impl CredentialStore for FailAfterOneCredentialWrite {
        fn get(&self, account: &str) -> Result<Option<String>, CredentialError> {
            self.entries
                .lock()
                .map(|entries| entries.get(account).cloned())
                .map_err(|_| CredentialError::Unavailable)
        }

        fn set(&self, account: &str, secret: &str) -> Result<(), CredentialError> {
            if self.writes.fetch_add(1, Ordering::Relaxed) == 1 {
                return Err(CredentialError::Unavailable);
            }
            self.entries
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), CredentialError> {
            self.entries
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .remove(account);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailCredentialDelete {
        entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
    }

    impl CredentialStore for FailCredentialDelete {
        fn get(&self, account: &str) -> Result<Option<String>, CredentialError> {
            self.entries
                .lock()
                .map(|entries| entries.get(account).cloned())
                .map_err(|_| CredentialError::Unavailable)
        }

        fn set(&self, account: &str, secret: &str) -> Result<(), CredentialError> {
            self.entries
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete(&self, _account: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Unavailable)
        }
    }

    #[test]
    fn partial_legacy_migration_failure_restores_vault_and_keeps_sqlite_values() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-credential-partial-failure-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let first_secret = "partial-migration-first-sentinel-do-not-log";
        let second_secret = "partial-migration-second-sentinel-do-not-log";
        {
            let conn = rusqlite::Connection::open(&path).expect("open test database");
            conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);").expect("create settings table");
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('brain_api_key', ?1, 1)",
                [first_secret],
            )
            .expect("seed first legacy key");
            conn.execute("INSERT INTO settings (key, value, updated_at) VALUES ('brain_fallback_api_key', ?1, 1)", [second_secret]).expect("seed second legacy key");
        }
        let credentials = Arc::new(FailAfterOneCredentialWrite::default());
        let store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            credentials.clone(),
        )
        .expect("open store after failed partial migration");

        assert!(store.credential_migration_pending());
        assert!(!store.credential_storage_available());
        assert_eq!(
            credentials
                .get("brain_api_key")
                .expect("read restored vault"),
            None
        );
        for (key, secret) in [
            ("brain_api_key", first_secret),
            ("brain_fallback_api_key", second_secret),
        ] {
            assert_eq!(
                store.get_plain(key).expect("legacy key remains").as_deref(),
                Some(secret)
            );
        }
    }

    #[test]
    fn retry_after_keyring_recovery_migrates_every_legacy_credential_family() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-credential-retry-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let legacy_values = [
            "retry-primary-sentinel",
            "retry-fallback-sentinel",
            "retry-secondary-sentinel",
            "retry-hackathon-sentinel",
        ];
        let mut hackathon = crate::hackathon::HackathonConfig::default();
        hackathon
            .models
            .push(crate::hackathon::HackathonModelConfig {
                id: "retry-model".to_string(),
                model_name: "model".to_string(),
                base_url: "https://api.example.test/v1".to_string(),
                api_key: legacy_values[3].to_string(),
                group_id: "group".to_string(),
            });
        {
            let conn = rusqlite::Connection::open(&path).expect("open migration database");
            conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);")
                .expect("create settings table");
            for (key, value) in [
                ("brain_api_key", legacy_values[0]),
                ("brain_fallback_api_key", legacy_values[1]),
                ("brain2_api_key", legacy_values[2]),
            ] {
                conn.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, 1)",
                    rusqlite::params![key, value],
                )
                .expect("seed legacy credential");
            }
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('hackathon_config', ?1, 1)",
                [serde_json::to_string(&hackathon).expect("serialize legacy Hackathon config")],
            )
            .expect("seed legacy Hackathon settings");
        }
        let credentials = Arc::new(ToggleCredentialStore::default());
        let mut store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            credentials.clone(),
        )
        .expect("open store with locked keyring");
        assert!(store.credential_migration_pending());
        credentials.available.store(true, Ordering::Relaxed);

        store
            .set("brain_api_key", "replacement-primary-sentinel")
            .expect("retry migration and update primary credential");

        for (account, value) in [
            ("brain_api_key", "replacement-primary-sentinel"),
            ("brain_fallback_api_key", legacy_values[1]),
            ("brain2_api_key", legacy_values[2]),
            ("hackathon.model.retry-model", legacy_values[3]),
        ] {
            assert!(matches!(credentials.get(account), Ok(Some(stored)) if stored == value));
        }
        assert!(!store.credential_migration_pending());
        let raw_database = std::fs::read(&path).expect("read migrated database bytes");
        for value in legacy_values
            .iter()
            .chain([&"replacement-primary-sentinel"])
        {
            assert!(
                !raw_database
                    .windows(value.len())
                    .any(|window| window == value.as_bytes())
            );
        }
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn credential_metadata_write_failure_restores_previous_vault_entry() {
        let (mut store, path) = temp_store();
        store
            .set("brain2_api_key", "existing-secondary-sentinel")
            .expect("seed old credential");
        store
            .conn
            .execute_batch(
                "CREATE TRIGGER reject_secondary_model BEFORE INSERT ON settings
             WHEN NEW.key = 'brain2_model' BEGIN SELECT RAISE(ABORT, 'synthetic'); END;",
            )
            .expect("install deterministic metadata failure");
        let result = store.save_secondary_brain_config(&super::SecondaryBrainConfig {
            api_key: "replacement-secondary-sentinel".to_string(),
            base_url: "https://new.example.test/v1".to_string(),
            model: "new-model".to_string(),
            system_prompt: String::new(),
            credential_storage_available: true,
            credential_migration_pending: false,
        });
        assert!(result.is_err());
        assert_eq!(
            store
                .get("brain2_api_key")
                .expect("read restored credential")
                .as_deref(),
            Some("existing-secondary-sentinel")
        );
        assert_eq!(
            store
                .get_plain("brain2_base_url")
                .expect("base url rollback"),
            None
        );
        assert_eq!(
            store.get_plain("brain2_model").expect("model rollback"),
            None
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn credential_disconnect_cleanup_failure_restores_vault_entry() {
        let (mut store, path) = temp_store();
        store
            .set("brain_api_key", "disconnect-preserved-sentinel")
            .expect("seed credential");
        store
            .set_plain("brain_api_key", "disconnect-preserved-sentinel")
            .expect("seed legacy plaintext row for cleanup rollback");
        store
            .conn
            .execute_batch(
                "CREATE TRIGGER reject_credential_delete BEFORE DELETE ON settings
             WHEN OLD.key = 'brain_api_key' BEGIN SELECT RAISE(ABORT, 'synthetic'); END;",
            )
            .expect("install deterministic cleanup failure");
        assert!(store.clear_credential("brain_api_key").is_err());
        assert_eq!(
            store
                .get("brain_api_key")
                .expect("credential restored")
                .as_deref(),
            Some("disconnect-preserved-sentinel")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn hackathon_credential_rollback_failure_marks_storage_unavailable() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-hackathon-rollback-failure-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let credentials = Arc::new(FailCredentialDelete::default());
        let mut store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            credentials.clone(),
        )
        .expect("open settings store");
        store
            .conn
            .execute_batch(
                "CREATE TRIGGER reject_hackathon_config_insert BEFORE INSERT ON settings
                 WHEN NEW.key = 'hackathon_config' BEGIN SELECT RAISE(ABORT, 'synthetic'); END;",
            )
            .expect("install metadata failure trigger");
        let secret = "rollback-failure-synthetic-secret";
        let config = crate::hackathon::HackathonConfig {
            groups: Vec::new(),
            models: vec![crate::hackathon::HackathonModelConfig {
                id: "rollback-model".to_string(),
                model_name: "model".to_string(),
                base_url: "https://api.example.test/v1".to_string(),
                api_key: secret.to_string(),
                group_id: "group".to_string(),
            }],
            max_questions_per_teammate: None,
            enabled: false,
        };

        assert!(store.save_hackathon_config(&config).is_err());
        assert!(!store.credential_storage_available());
        assert!(store.credential_migration_pending());
        assert_eq!(
            credentials
                .get("hackathon.model.rollback-model")
                .expect("inspect retained secure entry")
                .as_deref(),
            Some(secret)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn malformed_hackathon_config_marks_secret_enumeration_unavailable() {
        let (mut store, path) = temp_store();
        store
            .set_plain("hackathon_config", "{")
            .expect("seed malformed config");
        assert!(store.get_hackathon_config().is_err());
        assert!(!store.credential_storage_available());
        assert!(store.credential_migration_pending());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn locked_hackathon_credential_marks_storage_unavailable() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-locked-hackathon-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let credentials = Arc::new(MemoryCredentialStore::default());
        let mut store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            credentials,
        )
        .expect("open settings store");
        let mut config = crate::hackathon::HackathonConfig::default();
        config.models.push(crate::hackathon::HackathonModelConfig {
            id: "locked-model".to_string(),
            model_name: "model".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "locked-hackathon-sentinel".to_string(),
            group_id: "group".to_string(),
        });
        store
            .save_hackathon_config(&config)
            .expect("save secure Hackathon key");
        drop(store);

        let locked = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            Arc::new(UnavailableCredentialStore),
        )
        .expect("reopen with unavailable keyring");
        assert!(locked.get_hackathon_config().is_err());
        assert!(!locked.credential_storage_available());
        assert!(locked.credential_migration_pending());
        drop(locked);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_vault_entry_never_falls_back_to_stale_plaintext_credentials() {
        let (mut store, path) = temp_store();
        let credentials = store.credentials.clone();
        store
            .set("brain_api_key", "stale-direct-secret")
            .expect("save direct key securely");
        credentials
            .delete("brain_api_key")
            .expect("simulate removed vault entry");
        store
            .set_plain("brain_api_key", "stale-direct-secret")
            .expect("simulate restored legacy row");
        assert!(store.get("brain_api_key").is_err());
        assert!(store.credential_migration_pending());

        let mut config = crate::hackathon::HackathonConfig::default();
        config.models.push(crate::hackathon::HackathonModelConfig {
            id: "stale-model".to_string(),
            model_name: "model".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "stale-hackathon-secret".to_string(),
            group_id: "group".to_string(),
        });
        store
            .set_plain(
                "hackathon_config",
                &serde_json::to_string(&config).expect("serialize stale settings"),
            )
            .expect("simulate restored legacy Hackathon row");
        assert!(store.get_hackathon_config().is_err());
        assert!(store.credential_migration_pending());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn new_hackathon_secret_with_locked_store_marks_security_status() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-hackathon-locked-save-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            Arc::new(UnavailableCredentialStore),
        )
        .expect("open settings store");
        let mut config = crate::hackathon::HackathonConfig::default();
        config.models.push(crate::hackathon::HackathonModelConfig {
            id: "new-locked-model".to_string(),
            model_name: "model".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "locked-new-model-secret".to_string(),
            group_id: "group".to_string(),
        });
        assert!(store.save_hackathon_config(&config).is_err());
        assert!(!store.credential_storage_available());
        assert!(store.credential_migration_pending());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_legacy_migration_keeps_original_value_and_surfaces_pending_status() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-credential-failure-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let legacy_secret = "credential-failure-sentinel-do-not-log";
        {
            let conn = rusqlite::Connection::open(&path).expect("open test database");
            conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);").expect("create settings table");
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('brain_api_key', ?1, 1)",
                [legacy_secret],
            )
            .expect("seed legacy key");
        }
        let store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            Arc::new(UnavailableCredentialStore),
        )
        .expect("open store while keyring is unavailable");
        assert!(store.credential_migration_pending());
        assert!(!store.credential_storage_available());
        assert_eq!(
            store
                .get("brain_api_key")
                .expect("legacy fallback remains")
                .as_deref(),
            Some(legacy_secret)
        );
        let persisted: String = store
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'brain_api_key'",
                [],
                |row| row.get(0),
            )
            .expect("legacy row remains intact");
        assert_eq!(persisted, legacy_secret);
    }

    #[test]
    fn hackathon_keys_are_secure_persisted_preserved_and_removed_with_model() {
        let path = std::env::temp_dir().join(format!(
            "consensus-arena-hackathon-key-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let credentials = Arc::new(MemoryCredentialStore::default());
        let mut store = SettingsStore::new_with_credential_store(
            path.to_str().expect("temp path is utf8"),
            credentials.clone(),
        )
        .expect("open test settings");
        let sentinel = "hackathon-key-sentinel-do-not-log";
        let config = crate::hackathon::HackathonConfig {
            groups: Vec::new(),
            models: vec![crate::hackathon::HackathonModelConfig {
                id: "model-one".to_string(),
                model_name: "model".to_string(),
                base_url: "https://api.example.test/v1".to_string(),
                api_key: sentinel.to_string(),
                group_id: "group-one".to_string(),
            }],
            max_questions_per_teammate: Some(3),
            enabled: false,
        };
        store
            .save_hackathon_config(&config)
            .expect("save hackathon key");
        assert_eq!(
            credentials
                .get("hackathon.model.model-one")
                .expect("secure key lookup")
                .as_deref(),
            Some(sentinel)
        );
        let raw: String = store
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'hackathon_config'",
                [],
                |row| row.get(0),
            )
            .expect("read saved config");
        assert!(!raw.contains(sentinel));
        let loaded = store.get_hackathon_config().expect("load secure key");
        assert_eq!(loaded.models[0].api_key, sentinel);

        let mut without_key = loaded.clone();
        without_key.models[0].api_key.clear();
        store
            .save_hackathon_config(&without_key)
            .expect("blank round-trip keeps existing key");
        assert_eq!(
            store
                .get_hackathon_config()
                .expect("key still available")
                .models[0]
                .api_key,
            sentinel
        );

        let removed = crate::hackathon::HackathonConfig::default();
        store.save_hackathon_config(&removed).expect("remove model");
        assert_eq!(
            credentials
                .get("hackathon.model.model-one")
                .expect("removed secure key lookup"),
            None
        );
    }

    #[test]
    fn clearing_agent_brain_key_removes_secure_value_and_database_row() {
        let (mut store, _path) = temp_store();
        let secret = "clear-key-sentinel-do-not-log";
        store.set("brain_api_key", secret).expect("save credential");
        assert_eq!(
            store
                .get_agent_brain_config()
                .expect("read configured brain")
                .api_key,
            secret
        );
        store
            .clear_credential("brain_api_key")
            .expect("clear secure key");
        assert_eq!(store.get("brain_api_key").expect("read cleared key"), None);
        assert!(
            store
                .get_agent_brain_config()
                .expect("read cleared brain")
                .api_key
                .is_empty()
        );
    }
}
