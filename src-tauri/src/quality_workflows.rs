//! Small Arena-owned quality workflow policies.
//!
//! These are policy/data types, not a second coordinator. Existing Delivery
//! and Verification code remains responsible for execution and authority.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A bounded record of tools actually observed in one OpenCode execution.
/// Arguments and tool payloads are deliberately excluded so credentials or
/// untrusted prompt material cannot enter durable Product OS state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolUseReceipt {
    pub receipt_id: String,
    pub work_order_id: String,
    pub root_session_id: String,
    pub profile: String,
    pub tools: Vec<String>,
    pub tool_count: u32,
    pub status: String,
    pub started_at: i64,
    pub completed_at: i64,
    pub advisory_only: bool,
}

impl ToolUseReceipt {
    pub fn from_observed_tools(
        work_order_id: &str,
        root_session_id: &str,
        profile: &str,
        observed_tools: &[String],
        status: &str,
        started_at: i64,
        completed_at: i64,
    ) -> Result<Self, String> {
        if work_order_id.trim().is_empty()
            || root_session_id.trim().is_empty()
            || profile.trim().is_empty()
            || status.trim().is_empty()
            || completed_at < started_at
        {
            return Err("tool receipt fields are missing or invalid".to_string());
        }
        let mut tools = observed_tools
            .iter()
            .map(|tool| tool.trim())
            .filter(|tool| !tool.is_empty() && tool.len() <= 128)
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        tools.sort();
        tools.dedup();
        if tools.len() > 64 {
            return Err("tool receipt exceeded the bounded observed-tool set".to_string());
        }
        let tool_count = observed_tools.len().min(u32::MAX as usize) as u32;
        let identity = format!(
            "{work_order_id}:{root_session_id}:{profile}:{started_at}:{completed_at}:{}",
            tools.join(",")
        );
        Ok(Self {
            receipt_id: format!("tool-{:x}", Sha256::digest(identity.as_bytes())),
            work_order_id: work_order_id.to_string(),
            root_session_id: root_session_id.to_string(),
            profile: profile.to_string(),
            tools,
            tool_count,
            status: status.to_string(),
            started_at,
            completed_at,
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
        let receipt = ToolUseReceipt::from_observed_tools(
            "work-order",
            "browser-session",
            "browser_qa",
            &["playwright.inspect".to_string(), "playwright.inspect".to_string()],
            "complete",
            10,
            11,
        )
        .expect("receipt");
        assert!(receipt.advisory_only);
        assert_eq!(receipt.tools, vec!["playwright.inspect"]);
        assert_eq!(receipt.tool_count, 2);
        assert!(
            ToolUseReceipt::from_observed_tools(
                "",
                "browser-session",
                "browser_qa",
                &[],
                "complete",
                10,
                11
            )
            .is_err()
        );
    }
}
