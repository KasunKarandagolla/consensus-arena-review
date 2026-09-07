use serde::{Deserialize, Serialize};

pub const CHECKPOINT_VERSION: u32 = 1;

/// Next pipeline step that must be executed on resume. Persisted deterministically
/// so we do not repeat a completed route or lose a response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointNextStep {
    LeaderDecision,
    Route { target: String, prompt: String },
    LeaderReturn { from: String },
    BlueprintAck,
    Continue,
    AskUser,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    UserRequested,
    RateLimit,
    ProviderFailure,
    SystemRecovery,
}

impl Default for PauseReason {
    fn default() -> Self {
        PauseReason::UserRequested
    }
}

/// Versioned, secret-free checkpoint. Never contains api_key, bearer, cookies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionCheckpoint {
    pub checkpoint_version: u32,
    pub session_id: String,
    pub run_id: String,
    pub turn_number: u32,
    pub phase: String,
    pub leader_id: String,
    pub target_participant: Option<String>,
    pub next_step: CheckpointNextStep,
    pub pending_user_messages: Vec<String>,
    pub pause_requested: bool,
    pub paused: bool,
    pub pause_reason: PauseReason,
    pub created_at: String,
    // Persistent recovery: enough to reconstruct session without re-reading transcript Store's missing agent_ids
    #[serde(default)]
    pub agent_ids: Vec<String>,
    #[serde(default)]
    pub project_brief: String,
    #[serde(default)]
    pub session_type: String,
    #[serde(default)]
    pub hackathon_run_id: Option<String>,
    #[serde(default)]
    pub hackathon_task_brief: Option<String>,
    #[serde(default)]
    pub last_leader_response: Option<String>,
}

impl SessionCheckpoint {
    pub fn new(
        session_id: String,
        run_id: String,
        turn_number: u32,
        leader_id: String,
        next_step: CheckpointNextStep,
        pending_user_messages: Vec<String>,
        pause_reason: PauseReason,
    ) -> Self {
        Self {
            checkpoint_version: CHECKPOINT_VERSION,
            session_id,
            run_id,
            turn_number,
            phase: format!("{:?}", next_step),
            leader_id,
            target_participant: None,
            next_step,
            pending_user_messages,
            pause_requested: true,
            paused: true,
            pause_reason,
            created_at: chrono::Utc::now().to_rfc3339(),
            agent_ids: Vec::new(),
            project_brief: String::new(),
            session_type: String::new(),
            hackathon_run_id: None,
            hackathon_task_brief: None,
            last_leader_response: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.checkpoint_version != CHECKPOINT_VERSION {
            return Err(format!(
                "Unsupported checkpoint version {} (expected {})",
                self.checkpoint_version, CHECKPOINT_VERSION
            ));
        }
        if self.session_id.trim().is_empty() {
            return Err("checkpoint session_id cannot be empty".to_string());
        }
        if self.run_id.trim().is_empty() {
            return Err("checkpoint run_id cannot be empty".to_string());
        }
        if self.leader_id.trim().is_empty() {
            return Err("checkpoint leader_id cannot be empty".to_string());
        }
        // next_step is an enum, always valid if deserialized
        Ok(())
    }

    pub fn key(&self) -> String {
        format!("checkpoint:{}", self.session_id)
    }

    pub fn key_for(session_id: &str) -> String {
        format!("checkpoint:{}", session_id.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_round_trip() {
        let cp = SessionCheckpoint::new(
            "sess-123".to_string(),
            "run-456".to_string(),
            3,
            "claude".to_string(),
            CheckpointNextStep::LeaderDecision,
            vec!["hello".to_string()],
            PauseReason::UserRequested,
        );
        let json = serde_json::to_string(&cp).expect("serialize");
        assert!(json.contains("sess-123"));
        assert!(!json.contains("api_key"));
        let de: SessionCheckpoint = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cp, de);
        assert!(de.validate().is_ok());
    }

    #[test]
    fn checkpoint_rejects_unknown_version() {
        let mut cp = SessionCheckpoint::new(
            "sess".to_string(),
            "run".to_string(),
            1,
            "claude".to_string(),
            CheckpointNextStep::Continue,
            vec![],
            PauseReason::RateLimit,
        );
        cp.checkpoint_version = 99;
        assert!(cp.validate().is_err());
    }

    #[test]
    fn checkpoint_rejects_empty_session() {
        let cp = SessionCheckpoint::new(
            "".to_string(),
            "run".to_string(),
            1,
            "claude".to_string(),
            CheckpointNextStep::Complete,
            vec![],
            PauseReason::UserRequested,
        );
        assert!(cp.validate().is_err());
    }

    #[test]
    fn checkpoint_secret_free() {
        let cp = SessionCheckpoint::new(
            "sess".to_string(),
            "run".to_string(),
            1,
            "leader".to_string(),
            CheckpointNextStep::LeaderDecision,
            vec![],
            PauseReason::UserRequested,
        );
        let json = serde_json::to_string(&cp).expect("json");
        assert!(!json.to_lowercase().contains("api_key"));
        assert!(!json.contains("Bearer"));
        assert!(!json.contains("cookie"));
    }

    #[test]
    fn checkpoint_next_step_serialization() {
        let steps = vec![
            CheckpointNextStep::LeaderDecision,
            CheckpointNextStep::Route {
                target: "deepseek".to_string(),
                prompt: "hi".to_string(),
            },
            CheckpointNextStep::Complete,
        ];
        for step in steps {
            let cp = SessionCheckpoint::new(
                "s".to_string(),
                "r".to_string(),
                1,
                "claude".to_string(),
                step.clone(),
                vec![],
                PauseReason::UserRequested,
            );
            let json = serde_json::to_string(&cp).expect("ser");
            let de: SessionCheckpoint = serde_json::from_str(&json).expect("de");
            assert_eq!(de.next_step, step);
        }
    }
}
