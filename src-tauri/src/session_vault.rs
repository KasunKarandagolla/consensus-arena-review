use crate::errors::AgentError;
use ring::aead::{self, BoundKey, Nonce, NonceSequence, SealingKey, OpeningKey, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::error::Unspecified;
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{params, Connection};
use std::num::NonZeroU32;

const SALT: &[u8] = b"consensus-arena-v1-salt-2024";
const ITERATIONS: u32 = 100_000;

struct OneNonce([u8; NONCE_LEN]);

impl NonceSequence for OneNonce {
    fn advance(&mut self) -> Result<Nonce, Unspecified> {
        Ok(Nonce::assume_unique_for_key(self.0))
    }
}

pub struct SessionVault {
    conn: Connection,
    key_bytes: [u8; 32],
}

impl SessionVault {
    pub fn new() -> Self {
        let conn = Connection::open_in_memory().expect("vault db failed");
        let key_bytes = Self::derive_key();
        let vault = Self { conn, key_bytes };
        vault.init_schema().expect("vault schema failed");
        vault
    }

    pub fn open(db_path: &str) -> Result<Self, AgentError> {
        let conn = Connection::open(db_path)?;
        let key_bytes = Self::derive_key();
        let vault = Self { conn, key_bytes };
        vault.init_schema()?;
        Ok(vault)
    }

    fn derive_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(ITERATIONS).unwrap(),
            SALT,
            b"consensus-arena-desktop-key",
            &mut key,
        );
        key
    }

    fn init_schema(&self) -> Result<(), AgentError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cookies (
                agent_id TEXT PRIMARY KEY,
                data BLOB NOT NULL,
                saved_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS conversation_urls (
                session_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                url TEXT NOT NULL,
                PRIMARY KEY (session_id, agent_id)
            );",
        )
        .map_err(AgentError::from)
    }

    pub fn save_conversation_url(
        &mut self,
        session_id: &str,
        agent_id: &str,
        url: &str,
    ) -> Result<(), AgentError> {
        self.conn.execute(
            "INSERT INTO conversation_urls (session_id, agent_id, url)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id, agent_id) DO UPDATE SET url = excluded.url",
            params![session_id, agent_id, url],
        )?;
        Ok(())
    }

    pub fn get_conversation_url(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Option<String>, AgentError> {
        let mut stmt = self
            .conn
            .prepare("SELECT url FROM conversation_urls WHERE session_id = ?1 AND agent_id = ?2")?;
        let mut rows = stmt.query(params![session_id, agent_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Task 3 (CRIT-4): backs the `delete_session` command's vault-side
    /// cascade. Deletes only this session's saved `conversation_urls` rows.
    /// Deliberately does NOT touch the `cookies` table — cookies are stored
    /// per-`agent_id` (one row per model, e.g. "claude", "chatgpt"), not
    /// per-session; they represent the user's ongoing login state with that
    /// model's website, not anything owned by this particular session, and
    /// deleting a session must never log the user out of a model account.
    pub fn delete_session_urls(&mut self, session_id: &str) -> Result<(), AgentError> {
        self.conn.execute(
            "DELETE FROM conversation_urls WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn save_cookies(&self, agent_id: &str, data: &[u8]) -> Result<(), AgentError> {
        let encrypted = self.encrypt(data)?;
        self.conn.execute(
            "INSERT INTO cookies (agent_id, data, saved_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(agent_id) DO UPDATE SET data = excluded.data, saved_at = excluded.saved_at",
            params![agent_id, encrypted, chrono::Utc::now().timestamp()],
        )?;
        Ok(())
    }

    pub fn load_cookies(&self, agent_id: &str) -> Result<Option<Vec<u8>>, AgentError> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM cookies WHERE agent_id = ?1")?;
        let mut rows = stmt.query(params![agent_id])?;
        if let Some(row) = rows.next()? {
            let blob: Vec<u8> = row.get(0)?;
            Ok(Some(self.decrypt(&blob)?))
        } else {
            Ok(None)
        }
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AgentError> {
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| AgentError::UnknownError("nonce generation failed".to_string()))?;

        let unbound = UnboundKey::new(&AES_256_GCM, &self.key_bytes)
            .map_err(|_| AgentError::UnknownError("key creation failed".to_string()))?;
        let mut sealing = SealingKey::new(unbound, OneNonce(nonce_bytes));

        let mut in_out = plaintext.to_vec();
        sealing
            .seal_in_place_append_tag(aead::Aad::empty(), &mut in_out)
            .map_err(|_| AgentError::UnknownError("encryption failed".to_string()))?;

        // Prepend nonce: [12 bytes nonce][ciphertext+tag]
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);
        Ok(result)
    }

    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, AgentError> {
        if data.len() < NONCE_LEN {
            return Err(AgentError::UnknownError("encrypted data too short".to_string()));
        }
        let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(nonce_bytes);

        let unbound = UnboundKey::new(&AES_256_GCM, &self.key_bytes)
            .map_err(|_| AgentError::UnknownError("key creation failed".to_string()))?;
        let mut opening = OpeningKey::new(unbound, OneNonce(nonce));

        let mut in_out = ciphertext.to_vec();
        let decrypted = opening
            .open_in_place(aead::Aad::empty(), &mut in_out)
            .map_err(|_| AgentError::UnknownError("decryption failed".to_string()))?;
        Ok(decrypted.to_vec())
    }
}
