use crate::session_runtime::SessionOwner;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId(String);

impl OperationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    /// Strict parser: only accepts 32 hex chars (simple) or 36 with dashes.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let parsed = Uuid::parse_str(raw).map_err(|_| "invalid operation id".to_string())?;
        Ok(Self(parsed.simple().to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserSurface {
    Leader,
    Participant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
        agent_id: &str,
        turn: u32,
        surface: BrowserSurface,
    ) -> Self {
        Self {
            operation_id: OperationId::new(),
            session_id: owner.session_id.clone(),
            run_generation: owner.run_generation,
            agent_id: agent_id.to_string(),
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
    fn operation_id_round_trip() {
        let id = OperationId::new();
        let s = id.as_str().to_string();
        let parsed = OperationId::parse(&s).expect("should parse");
        assert_eq!(parsed.as_str(), s);
    }

    #[test]
    fn operation_id_rejects_malformed() {
        assert!(OperationId::parse("not-a-uuid").is_err());
        assert!(OperationId::parse("").is_err());
        assert!(OperationId::parse("123").is_err());
        // simple format is 32 hex; invalid hex should fail
        assert!(OperationId::parse("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
    }

    #[test]
    fn id1_same_agent_turn_different_owner_generation() {
        let owner_a = SessionOwner {
            session_id: "sess-1".to_string(),
            run_generation: 1,
        };
        let owner_b = SessionOwner {
            session_id: "sess-1".to_string(),
            run_generation: 2,
        };
        let ctx_a = OperationContext::from_owner(&owner_a, "claude", 7, BrowserSurface::Leader);
        let ctx_b = OperationContext::from_owner(&owner_b, "claude", 7, BrowserSurface::Leader);
        assert_ne!(ctx_a.operation_id, ctx_b.operation_id);
        assert_ne!(ctx_a.run_generation, ctx_b.run_generation);
        assert_eq!(ctx_a.agent_id, ctx_b.agent_id);
        assert_eq!(ctx_a.turn, ctx_b.turn);
    }

    #[test]
    fn id2_same_owner_agent_turn_different_nonce() {
        let owner = SessionOwner {
            session_id: "sess-1".to_string(),
            run_generation: 1,
        };
        let ctx1 = OperationContext::from_owner(&owner, "claude", 5, BrowserSurface::Participant);
        let ctx2 = OperationContext::from_owner(&owner, "claude", 5, BrowserSurface::Participant);
        assert_ne!(ctx1.operation_id, ctx2.operation_id);
        assert_eq!(ctx1.session_id, ctx2.session_id);
        assert_eq!(ctx1.run_generation, ctx2.run_generation);
    }

    #[test]
    fn id3_malformed_operation_id() {
        assert!(OperationId::parse("invalid-op-id-!@#").is_err());
        assert!(OperationId::parse("g1234567890123456789012345678901").is_err());
    }
}
