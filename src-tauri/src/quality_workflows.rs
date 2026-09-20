//! Small Arena-owned quality workflow policies.
//!
//! These are policy/data types, not a second coordinator. Existing Delivery
//! and Verification code remains responsible for execution and authority.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A bounded record of an exploratory tool call. It is deliberately advisory;
/// it cannot satisfy a verifier or mutate ProductAuthority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolUseReceipt {
    pub receipt_id: String,
    pub session_id: String,
    pub profile: String,
    pub tool: String,
    pub input_digest: String,
    pub output_summary: String,
    pub advisory_only: bool,
}

impl ToolUseReceipt {
    pub fn new(
        session_id: &str,
        profile: &str,
        tool: &str,
        input: &str,
        output_summary: &str,
    ) -> Result<Self, String> {
        if session_id.trim().is_empty()
            || profile.trim().is_empty()
            || tool.trim().is_empty()
            || output_summary.trim().is_empty()
            || output_summary.len() > 8 * 1024
        {
            return Err("tool receipt fields are missing or oversized".to_string());
        }
        let digest = Sha256::digest(input.as_bytes());
        Ok(Self {
            receipt_id: format!(
                "tool-{:x}",
                Sha256::digest(format!("{session_id}:{tool}:{input}").as_bytes())
            ),
            session_id: session_id.to_string(),
            profile: profile.to_string(),
            tool: tool.to_string(),
            input_digest: format!("sha256:{digest:x}"),
            output_summary: output_summary.to_string(),
            advisory_only: true,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEvidenceKind {
    PlaywrightTest,
    PlaywrightMcpExploration,
    ChromeDevtoolsDiagnosis,
    NativeTauriWebdriver,
}

impl BrowserEvidenceKind {
    pub const fn is_deterministic_verification(self) -> bool {
        matches!(self, Self::PlaywrightTest | Self::NativeTauriWebdriver)
    }

    pub const fn authority_note(self) -> &'static str {
        match self {
            Self::PlaywrightTest => "frozen web-target command may become verifier evidence",
            Self::PlaywrightMcpExploration => {
                "exploratory browser output is advisory and must be converted to a frozen check"
            }
            Self::ChromeDevtoolsDiagnosis => {
                "Chromium diagnosis is advisory and does not verify the native Tauri shell"
            }
            Self::NativeTauriWebdriver => {
                "native-shell evidence requires the target-specific Tauri WebdriverIO service"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceProcedure {
    pub scenario: String,
    pub baseline_command: String,
    pub constraint: String,
    pub hypothesis: String,
    pub measurement_command: String,
    pub bounded_change: String,
    pub rerun_command: String,
    pub evidence_ref: Option<String>,
}

impl PerformanceProcedure {
    pub fn is_complete(&self) -> bool {
        !self.scenario.trim().is_empty()
            && !self.baseline_command.trim().is_empty()
            && !self.constraint.trim().is_empty()
            && !self.hypothesis.trim().is_empty()
            && !self.measurement_command.trim().is_empty()
            && !self.bounded_change.trim().is_empty()
            && !self.rerun_command.trim().is_empty()
            && self
                .evidence_ref
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1Disposition {
    RejectedNotRequired,
    Integrated,
}

pub const SENTRY_MCP_V1: V1Disposition = V1Disposition::RejectedNotRequired;
pub const AGENTSYS_SKILLERS_V1: V1Disposition = V1Disposition::RejectedNotRequired;
pub const FLOW_NEXT_V1: V1Disposition = V1Disposition::RejectedNotRequired;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exploratory_browser_output_cannot_be_verification() {
        assert!(!BrowserEvidenceKind::PlaywrightMcpExploration.is_deterministic_verification());
        assert!(!BrowserEvidenceKind::ChromeDevtoolsDiagnosis.is_deterministic_verification());
        assert!(BrowserEvidenceKind::PlaywrightTest.is_deterministic_verification());
    }

    #[test]
    fn performance_procedure_requires_actual_evidence() {
        let mut procedure = PerformanceProcedure {
            scenario: "frontend build".to_string(),
            baseline_command: "npm run build".to_string(),
            constraint: "4 GiB host".to_string(),
            hypothesis: "parallelism increases RSS".to_string(),
            measurement_command: "/usr/bin/time npm run build".to_string(),
            bounded_change: "one build job".to_string(),
            rerun_command: "npm run build".to_string(),
            evidence_ref: None,
        };
        assert!(!procedure.is_complete());
        procedure.evidence_ref = Some("evidence/build.txt".to_string());
        assert!(procedure.is_complete());
    }

    #[test]
    fn tool_receipt_is_bounded_and_advisory() {
        let receipt = ToolUseReceipt::new(
            "browser-session",
            "browser_qa",
            "playwright.inspect",
            "https://example.invalid",
            "one locator discovered",
        )
        .expect("receipt");
        assert!(receipt.advisory_only);
        assert!(receipt.input_digest.starts_with("sha256:"));
        assert!(ToolUseReceipt::new("", "browser_qa", "tool", "in", "out").is_err());
    }
}
