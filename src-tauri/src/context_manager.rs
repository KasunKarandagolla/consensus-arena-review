use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    Architecture,
    Mvp,
    Api,
    Security,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusSignal {
    Agrees,
    Disagrees(String),
    Improves(String),
    NoSignal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub agent_id: String,
    pub role: String,
    pub iteration: u32,
    pub response: String,
    pub consensus_signal: ConsensusSignal,
    pub timestamp: i64,
}

pub struct ContextManager {
    pub history: Vec<TurnRecord>,
    pub project_brief: String,
    pub requirements_charter: Option<String>,
    pub session_type: SessionType,
    pub pending_user_input: Option<String>,
}

impl ContextManager {
    pub fn new(project_brief: String, session_type: SessionType) -> Self {
        Self {
            history: Vec::new(),
            project_brief,
            requirements_charter: None,
            session_type,
            pending_user_input: None,
        }
    }

    pub fn build_prompt_for_agent(&self, agent_id: &str, role: &str, current_question: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str(&format!(
            "You are participating in a structured expert panel discussion.\nYour role is {}. Agent ID: {}.\n\n",
            role, agent_id
        ));
        prompt.push_str(&format!("Project Brief:\n{}\n\n", self.project_brief));
        if let Some(charter) = &self.requirements_charter {
            prompt.push_str(&format!("Requirements Charter:\n{}\n\n", charter));
        }
        if !self.history.is_empty() {
            prompt.push_str("=== Expert Panel Discussion So Far ===\n");
            for record in self.get_history_for_prompt() {
                prompt.push_str(&format!("{} ({}): {}\n\n", record.agent_id, record.role, record.response));
            }
            prompt.push_str("=== End of Discussion ===\n\n");
        }
        prompt.push_str(&format!("Current question: {}\n\n", current_question));
        prompt.push_str(
            "Instructions:\n\
             - If you agree and have nothing to improve, respond with CONSENSUS on its own line.\n\
             - If you have a concern, include DISAGREES: [your reason].\n\
             - If you have a concrete improvement, include IMPROVES: [your change].\n\
             - Be concise. Only respond when you have something new to add.\n"
        );
        prompt
    }

    fn get_history_for_prompt(&self) -> Vec<&TurnRecord> {
        let total_chars: usize = self.history.iter().map(|r| r.response.len()).sum();
        if total_chars <= 60000 {
            return self.history.iter().collect();
        }
        let max_iter = self.history.iter().map(|r| r.iteration).max().unwrap_or(0);
        let cutoff = if max_iter >= 2 { max_iter - 1 } else { 0 };
        self.history.iter().filter(|r| r.iteration >= cutoff).collect()
    }

    pub fn add_turn(&mut self, record: TurnRecord) {
        self.history.push(record);
    }

    pub fn set_requirements_charter(&mut self, charter: String) {
        self.requirements_charter = Some(charter);
    }

    pub fn set_pending_user_input(&mut self, input: String) {
        self.pending_user_input = Some(input);
    }

    pub fn take_pending_user_input(&mut self) -> Option<String> {
        self.pending_user_input.take()
    }

    pub fn detect_consensus_signal(&self, response: &str) -> ConsensusSignal {
        let upper = response.to_uppercase();
        if upper.contains("CONSENSUS") {
            return ConsensusSignal::Agrees;
        }
        if let Some(pos) = upper.find("IMPROVES:") {
            let reason = response[pos + 9..].trim().lines().next().unwrap_or("").to_string();
            return ConsensusSignal::Improves(reason);
        }
        if let Some(pos) = upper.find("DISAGREES:") {
            let reason = response[pos + 10..].trim().lines().next().unwrap_or("").to_string();
            return ConsensusSignal::Disagrees(reason);
        }
        ConsensusSignal::NoSignal
    }

    pub fn is_consensus_reached(&self, agent_ids: &[String]) -> bool {
        if agent_ids.is_empty() || self.history.is_empty() {
            return false;
        }
        let max_iter = self.history.iter().map(|r| r.iteration).max().unwrap_or(0);
        let last_iter: Vec<&TurnRecord> = self.history.iter().filter(|r| r.iteration == max_iter).collect();
        for agent_id in agent_ids {
            match last_iter.iter().find(|r| &r.agent_id == agent_id) {
                Some(r) => match &r.consensus_signal {
                    ConsensusSignal::Agrees => {}
                    _ => return false,
                },
                None => return false,
            }
        }
        true
    }

    pub fn is_improvement_velocity_low(&self) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let max_iter = self.history.iter().map(|r| r.iteration).max().unwrap_or(0);
        if max_iter < 2 {
            return false;
        }
        let recent: Vec<&TurnRecord> = self.history.iter().filter(|r| r.iteration >= max_iter - 1).collect();
        !recent.iter().any(|r| matches!(r.consensus_signal, ConsensusSignal::Improves(_)))
    }
}
