pub struct TurnManager {
    agent_ids: Vec<String>,
    leader_id: String,
    current_index: usize,
    pub iteration: u32,
}

impl TurnManager {
    pub fn new(agent_ids: Vec<String>, leader_id: String) -> Self {
        TurnManager {
            agent_ids,
            leader_id,
            current_index: 0,
            iteration: 0,
        }
    }

    /// Returns agents in order: leader first, then non-leaders.
    /// Returns None when all agents have spoken in the current iteration.
    pub fn next_agent(&mut self) -> Option<String> {
        let ordered = self.ordered_agents();
        if self.current_index >= ordered.len() {
            return None;
        }
        let agent = ordered[self.current_index].clone();
        self.current_index += 1;
        Some(agent)
    }

    pub fn advance_iteration(&mut self) {
        self.current_index = 0;
        self.iteration += 1;
    }

    pub fn current_iteration(&self) -> u32 {
        self.iteration
    }

    pub fn is_leader(&self, agent_id: &str) -> bool {
        self.leader_id == agent_id
    }

    fn ordered_agents(&self) -> Vec<String> {
        let mut ordered = vec![self.leader_id.clone()];
        for id in &self.agent_ids {
            if id != &self.leader_id {
                ordered.push(id.clone());
            }
        }
        ordered
    }
}
