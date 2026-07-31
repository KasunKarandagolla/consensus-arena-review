use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AgentHealth {
    pub failure_ratio: f64,
    pub captcha_ratio: f64,
    pub score: f64,
}

impl AgentHealth {
    pub fn new(failure_ratio: f64, captcha_ratio: f64) -> Self {
        let score = failure_ratio * 0.7 + captcha_ratio * 0.3;
        AgentHealth {
            failure_ratio,
            captcha_ratio,
            score,
        }
    }
}

pub struct ResourceMonitor {
    last_heartbeat: Instant,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        ResourceMonitor {
            last_heartbeat: Instant::now(),
        }
    }

    pub fn check_memory(&self, current_mb: u64) -> MemoryAction {
        if current_mb >= 1200 {
            MemoryAction::Hibernate
        } else if current_mb >= 800 {
            MemoryAction::ForceReload
        } else if current_mb >= 600 {
            MemoryAction::AgentRehydrate
        } else {
            MemoryAction::Normal
        }
    }

    pub fn check_heartbeat(&self) -> bool {
        // 72-hour session heartbeat checker
        let elapsed = self.last_heartbeat.elapsed();
        elapsed < Duration::from_secs(72 * 3600)
    }

    pub fn reset_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }
}

pub enum MemoryAction {
    Normal,
    Hibernate,
    ForceReload,
    AgentRehydrate,
}
