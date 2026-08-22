use crate::errors::AgentError;

#[derive(Debug, Clone)]
pub enum ActionType {
    ReadFile,
    WriteFile,
    ExecuteCommand,
    NetworkRequest,
}

#[derive(Debug, Clone)]
pub struct PendingAction {
    pub action: ActionType,
    pub requested_by: String,
    pub reason: String,
}

pub struct AgenticManager {
    pub workspace_dir: String,
    pub pending_approvals: Vec<PendingAction>,
}

impl AgenticManager {
    pub fn new(workspace_dir: String) -> Self {
        AgenticManager {
            workspace_dir,
            pending_approvals: Vec::new(),
        }
    }

    pub fn request_action(&mut self, action: ActionType, requested_by: String, reason: String) -> Result<(), AgentError> {
        match action {
            ActionType::ReadFile => {
                // Auto-approve reads within workspace
                Ok(())
            }
            ActionType::WriteFile | ActionType::ExecuteCommand | ActionType::NetworkRequest => {
                // Require user approval
                self.pending_approvals.push(PendingAction {
                    action,
                    requested_by,
                    reason,
                });
                Ok(())
            }
        }
    }

    pub fn approve_action(&mut self, index: usize) -> Result<(), AgentError> {
        if index < self.pending_approvals.len() {
            self.pending_approvals.remove(index);
            Ok(())
        } else {
            Err(AgentError::UnknownError("Invalid approval index".to_string()))
        }
    }

    pub fn reject_action(&mut self, index: usize) -> Result<(), AgentError> {
        if index < self.pending_approvals.len() {
            self.pending_approvals.remove(index);
            Ok(())
        } else {
            Err(AgentError::UnknownError("Invalid rejection index".to_string()))
        }
    }
}
