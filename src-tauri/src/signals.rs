#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignalState {
    Unknown,
    Asserted,
    Negated,
}

#[derive(Debug, Clone)]
pub struct TurnSignals {
    pub dom_toggle: SignalState,
    pub content_stability: SignalState,
    pub sse_stream: SignalState,
    pub network_idle: SignalState,
}

impl TurnSignals {
    pub fn new() -> Self {
        TurnSignals {
            dom_toggle: SignalState::Unknown,
            content_stability: SignalState::Unknown,
            sse_stream: SignalState::Unknown,
            network_idle: SignalState::Unknown,
        }
    }

    pub fn weighted_sum(&self) -> f64 {
        let mut sum = 0.0;
        if self.dom_toggle == SignalState::Asserted {
            sum += 1.5;
        }
        if self.content_stability == SignalState::Asserted {
            sum += 1.0;
        }
        if self.sse_stream == SignalState::Asserted {
            sum += 1.0;
        }
        if self.network_idle == SignalState::Asserted {
            sum += 0.5;
        }
        sum
    }

    pub fn is_complete(&self) -> bool {
        self.weighted_sum() >= 3.0
    }
}

#[derive(Debug, Clone)]
pub struct CompletionReport {
    pub total_weight: f64,
    pub threshold: f64,
    pub complete: bool,
}

impl CompletionReport {
    pub fn from_signals(signals: &TurnSignals) -> Self {
        let total_weight = signals.weighted_sum();
        let threshold = 3.0;
        CompletionReport {
            total_weight,
            threshold,
            complete: total_weight >= threshold,
        }
    }
}
