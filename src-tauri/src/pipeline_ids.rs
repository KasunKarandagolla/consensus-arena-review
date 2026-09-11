use std::fmt;

use crate::session_runtime::SessionOwner;

/// On which WebView surface an operation executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowserSurface {
    Leader,
    Participant,
}

impl fmt::Display for BrowserSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowserSurface::Leader => write!(f, "leader"),
            BrowserSurface::Participant => write!(f, "participant"),
        }
    }
}

/// One-shot UUIDv4 operation identity. Generated internally, never reused
/// intentionally in production. Parsing is strict UUID v4.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationId(String);

impl OperationId {
    /// Generate a fresh one-shot UUID v4.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Strict parse: canonical hyphenated UUID (8-4-4-4-12). Normalizes to canonical
    /// lower-case hyphenated form. Rejects non-hyphenated or malformed.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw.is_empty() {
            return Err("empty operation id".to_string());
        }
        if raw.len() != 36 {
            return Err(format!("invalid operation id length: {}", raw.len()));
        }
        // Check hyphen positions
        for &pos in &[8, 13, 18, 23] {
            if raw.as_bytes().get(pos) != Some(&b'-') {
                return Err("invalid operation id hyphen position".to_string());
            }
        }
        // Check hex digits
        for (i, &b) in raw.as_bytes().iter().enumerate() {
            if [8, 13, 18, 23].contains(&i) {
                continue;
            }
            if !(b.is_ascii_hexdigit()) {
                return Err("invalid operation id hex".to_string());
            }
        }
        let parsed =
            uuid::Uuid::parse_str(raw).map_err(|e| format!("invalid operation id: {e}"))?;
        // Ensure we store canonical string (lowercase hyphenated)
        Ok(Self(parsed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for OperationId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<OperationId> for String {
    fn from(id: OperationId) -> Self {
        id.0
    }
}

/// Immutable context for a single active irreversible browser operation.
/// Includes current SessionRuntime owner identity (session_id + run_generation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationContext {
    pub operation_id: OperationId,
    pub session_id: String,
    pub run_generation: u64,
    pub agent_id: String,
    pub turn: u32,
    pub surface: BrowserSurface,
}

impl OperationContext {
    pub fn from_owner(
        owner: &SessionOwner,
        agent_id: impl Into<String>,
        turn: u32,
        surface: BrowserSurface,
    ) -> Self {
        Self {
            operation_id: OperationId::new(),
            session_id: owner.session_id.clone(),
            run_generation: owner.run_generation,
            agent_id: agent_id.into(),
            turn,
            surface,
        }
    }

    /// Construct with explicit operation_id (used for tests / parsing).
    pub fn with_id(
        operation_id: OperationId,
        owner: &SessionOwner,
        agent_id: impl Into<String>,
        turn: u32,
        surface: BrowserSurface,
    ) -> Self {
        Self {
            operation_id,
            session_id: owner.session_id.clone(),
            run_generation: owner.run_generation,
            agent_id: agent_id.into(),
            turn,
            surface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_runtime::SessionOwner;

    #[test]
    fn operation_id_round_trip_strict_parse() {
        let id = OperationId::new();
        let parsed = OperationId::parse(id.as_str()).expect("parse should succeed");
        assert_eq!(id, parsed);
        assert_eq!(id.as_str(), parsed.as_str());
    }

    #[test]
    fn operation_id_malformed_rejected() {
        assert!(OperationId::parse("").is_err());
        assert!(OperationId::parse("not-a-uuid").is_err());
        assert!(OperationId::parse("123e4567-e89b-12d3-a456-42661417400").is_err()); // short
        assert!(OperationId::parse("123e4567-e89b-12d3-a456-42661417400z").is_err());
        assert!(OperationId::parse("xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx").is_err());
        // Missing hyphens should be rejected (uuid crate requires hyphens)
        assert!(OperationId::parse("550e8400e29b41d4a716446655440000").is_err());
    }

    #[test]
    fn t2_01_same_agent_turn_different_run_generation_different_operation_id() {
        let owner1 = SessionOwner {
            session_id: "sess".to_string(),
            run_generation: 1,
        };
        let owner2 = SessionOwner {
            session_id: "sess".to_string(),
            run_generation: 2,
        };
        let ctx1 = OperationContext::from_owner(&owner1, "claude", 1, BrowserSurface::Leader);
        let ctx2 = OperationContext::from_owner(&owner2, "claude", 1, BrowserSurface::Leader);
        // OperationIds are one-shot regardless, but contexts must differ in generation and id
        assert_ne!(ctx1.operation_id, ctx2.operation_id);
        assert_ne!(ctx1.run_generation, ctx2.run_generation);
        assert_eq!(ctx1.agent_id, ctx2.agent_id);
        assert_eq!(ctx1.turn, ctx2.turn);
    }

    #[test]
    fn t2_02_same_owner_agent_turn_twice_different_operation_id() {
        let owner = SessionOwner {
            session_id: "sess".to_string(),
            run_generation: 5,
        };
        let ctx1 = OperationContext::from_owner(&owner, "claude", 7, BrowserSurface::Participant);
        let ctx2 = OperationContext::from_owner(&owner, "claude", 7, BrowserSurface::Participant);
        assert_ne!(ctx1.operation_id, ctx2.operation_id);
        // Same logical identity otherwise
        assert_eq!(ctx1.session_id, ctx2.session_id);
        assert_eq!(ctx1.agent_id, ctx2.agent_id);
        assert_eq!(ctx1.turn, ctx2.turn);
        assert_eq!(ctx1.run_generation, ctx2.run_generation);
    }

    #[test]
    fn t2_03_malformed_ids_rejected() {
        // Already covered partially, but explicit T2-03 style
        for bad in [
            "",
            " ",
            "550e8400-e29b-41d4-a716-44665544000", // missing char
            "550e8400-e29b-41d4-a716-44665544000g", // invalid hex
            "550e8400e29b41d4a716446655440000",    // no hyphens
            "not-uuid-at-all-xxxx",
        ] {
            assert!(
                OperationId::parse(bad).is_err(),
                "should reject malformed id: {bad}"
            );
        }
    }

    #[test]
    fn operation_id_canonical_normalization() {
        // Uppercase input should parse and normalize to lowercase canonical
        let upper = "550E8400-E29B-41D4-A716-446655440000";
        let parsed = OperationId::parse(upper).expect("uppercase uuid should parse");
        assert_eq!(parsed.as_str(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn operation_context_carries_owner() {
        let owner = SessionOwner {
            session_id: "my-session".to_string(),
            run_generation: 42,
        };
        let ctx = OperationContext::from_owner(&owner, "gemini", 3, BrowserSurface::Participant);
        assert_eq!(ctx.session_id, "my-session");
        assert_eq!(ctx.run_generation, 42);
        assert_eq!(ctx.agent_id, "gemini");
        assert_eq!(ctx.turn, 3);
        assert_eq!(ctx.surface, BrowserSurface::Participant);
    }
}
