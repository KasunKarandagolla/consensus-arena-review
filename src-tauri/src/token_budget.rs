use std::collections::HashMap;

pub struct TokenBudget {
    per_agent: HashMap<String, u32>,
}

impl TokenBudget {
    pub fn new() -> Self {
        TokenBudget {
            per_agent: HashMap::new(),
        }
    }

    pub fn record_tokens(&mut self, agent_id: &str, count: u32) {
        let entry = self.per_agent.entry(agent_id.to_string()).or_insert(0);
        *entry += count;
    }

    pub fn get_tokens(&self, agent_id: &str) -> u32 {
        self.per_agent.get(agent_id).copied().unwrap_or(0)
    }

    pub fn get_percentage(&self, agent_id: &str, limit: u32) -> f32 {
        if limit == 0 {
            return 0.0;
        }
        (self.get_tokens(agent_id) as f32 / limit as f32) * 100.0
    }

    pub fn should_compress(&self, agent_id: &str, limit: u32) -> bool {
        self.get_percentage(agent_id, limit) >= 70.0
    }

    pub fn should_migrate(&self, agent_id: &str, limit: u32) -> bool {
        self.get_percentage(agent_id, limit) >= 90.0
    }

    pub fn reset(&mut self, agent_id: &str) {
        self.per_agent.insert(agent_id.to_string(), 0);
    }

    /// HIGH-7 (Task 10): clear every agent's count at once. Called from
    /// `commands::start_session` so a new session never inherits token
    /// counts accumulated during a previous session's lifetime.
    ///
    /// Note: as of this fix, nothing in `response_router.rs` actually calls
    /// `record_tokens()` yet during the session loop, so in practice this
    /// reset has no visible effect until token recording itself is wired
    /// into the loop — that's a separate, currently-unscoped gap, not
    /// something this reset can fix on its own. This change makes the
    /// session-boundary behaviour correct for whenever that wiring lands.
    pub fn reset_all(&mut self) {
        self.per_agent.clear();
    }

    pub fn all_tokens(&self) -> &HashMap<String, u32> {
        &self.per_agent
    }
}
