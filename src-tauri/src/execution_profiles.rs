//! Arena-owned OpenCode execution policy.
//!
//! Profiles are selected by Arena from an authoritative role. They are not a
//! worker-facing plugin or permission-escalation API. In particular, none of
//! the profiles carries Product OS, verification, or Apply authority.

use crate::product_os::ProductWorkOrderRole;
use serde::{Deserialize, Serialize};

pub const CONTEXT7_URL: &str = "https://mcp.context7.com/mcp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionProfile {
    SemanticNoTools,
    WebResearch,
    Implementation,
    DebugRepair,
    CandidateReview,
    BrowserQa,
    DocsMaintenance,
    PerformanceInvestigation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePolicy {
    Light,
    Standard,
    HeavyLsp,
    MpcBounded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSpec {
    pub agent: &'static str,
    pub allowed_tools: &'static [&'static str],
    pub selected_skills: &'static [&'static str],
    pub lsp: bool,
    pub context7: bool,
    pub timeout_seconds: u64,
    pub max_prompt_bytes: usize,
    pub max_result_bytes: usize,
    pub resource_policy: ResourcePolicy,
}

impl ExecutionProfile {
    pub fn spec(self) -> ProfileSpec {
        match self {
            Self::SemanticNoTools => ProfileSpec {
                agent: "plan",
                allowed_tools: &[],
                selected_skills: &[],
                lsp: false,
                context7: false,
                timeout_seconds: 300,
                max_prompt_bytes: 48 * 1024,
                max_result_bytes: 64 * 1024,
                resource_policy: ResourcePolicy::Light,
            },
            Self::WebResearch => ProfileSpec {
                agent: "plan",
                allowed_tools: &["websearch"],
                selected_skills: &[],
                lsp: false,
                context7: false,
                timeout_seconds: 300,
                max_prompt_bytes: 48 * 1024,
                max_result_bytes: 64 * 1024,
                resource_policy: ResourcePolicy::Light,
            },
            Self::Implementation => ProfileSpec {
                agent: "build",
                allowed_tools: &[
                    "read", "edit", "glob", "grep", "list", "bash", "lsp", "skill",
                ],
                selected_skills: &["test-driven-development", "verification-before-completion"],
                lsp: true,
                context7: false,
                timeout_seconds: 1_800,
                max_prompt_bytes: 48 * 1024,
                max_result_bytes: 64 * 1024,
                resource_policy: ResourcePolicy::HeavyLsp,
            },
            Self::DebugRepair => ProfileSpec {
                agent: "build",
                allowed_tools: &[
                    "read", "edit", "glob", "grep", "list", "bash", "lsp", "skill",
                ],
                selected_skills: &["systematic-debugging", "verification-before-completion"],
                lsp: true,
                context7: false,
                timeout_seconds: 1_800,
                max_prompt_bytes: 48 * 1024,
                max_result_bytes: 64 * 1024,
                resource_policy: ResourcePolicy::HeavyLsp,
            },
            Self::CandidateReview => ProfileSpec {
                agent: "plan",
                allowed_tools: &["read", "glob", "grep", "list", "lsp", "skill"],
                selected_skills: &["requesting-code-review", "receiving-code-review"],
                lsp: true,
                context7: false,
                timeout_seconds: 600,
                max_prompt_bytes: 48 * 1024,
                max_result_bytes: 64 * 1024,
                resource_policy: ResourcePolicy::HeavyLsp,
            },
            Self::BrowserQa => ProfileSpec {
                agent: "build",
                allowed_tools: &["read", "glob", "grep", "list", "bash"],
                selected_skills: &[],
                lsp: false,
                context7: false,
                timeout_seconds: 900,
                max_prompt_bytes: 48 * 1024,
                max_result_bytes: 64 * 1024,
                resource_policy: ResourcePolicy::Standard,
            },
            Self::DocsMaintenance => ProfileSpec {
                agent: "build",
                allowed_tools: &[
                    "read",
                    "edit",
                    "glob",
                    "grep",
                    "list",
                    "skill",
                    "mcp_context7",
                ],
                selected_skills: &[],
                lsp: false,
                context7: true,
                timeout_seconds: 600,
                max_prompt_bytes: 16 * 1024,
                max_result_bytes: 32 * 1024,
                resource_policy: ResourcePolicy::MpcBounded,
            },
            Self::PerformanceInvestigation => ProfileSpec {
                agent: "plan",
                allowed_tools: &["read", "glob", "grep", "list", "bash", "mcp_context7"],
                selected_skills: &[],
                lsp: false,
                context7: true,
                timeout_seconds: 900,
                max_prompt_bytes: 16 * 1024,
                max_result_bytes: 32 * 1024,
                resource_policy: ResourcePolicy::MpcBounded,
            },
        }
    }

    pub fn for_product_role(role: &ProductWorkOrderRole) -> Self {
        match role {
            ProductWorkOrderRole::Researcher | ProductWorkOrderRole::FactVerifier => {
                Self::WebResearch
            }
            ProductWorkOrderRole::ProductDirector
            | ProductWorkOrderRole::ArchitectA
            | ProductWorkOrderRole::ArchitectB
            | ProductWorkOrderRole::ReuseReviewer
            | ProductWorkOrderRole::ConstraintsReviewer
            | ProductWorkOrderRole::RedTeamReviewer
            | ProductWorkOrderRole::DissentReviewer
            | ProductWorkOrderRole::FeasibilityReviewer => Self::SemanticNoTools,
        }
    }

    pub fn authority_free_config(self) -> serde_json::Value {
        let spec = self.spec();
        let mut permissions = serde_json::Map::new();
        for tool in spec.allowed_tools {
            permissions.insert(
                (*tool).to_string(),
                serde_json::Value::String("allow".to_string()),
            );
        }
        for tool in [
            "question",
            "external_directory",
            "apply",
            "verify",
            "acceptance",
        ] {
            permissions.insert(
                tool.to_string(),
                serde_json::Value::String("deny".to_string()),
            );
        }
        permissions.insert("skill".to_string(), if spec.selected_skills.is_empty() {
            serde_json::Value::String("deny".to_string())
        } else {
            serde_json::json!({
                "*": "deny",
                "test-driven-development": if spec.selected_skills.contains(&"test-driven-development") { "allow" } else { "deny" },
                "verification-before-completion": if spec.selected_skills.contains(&"verification-before-completion") { "allow" } else { "deny" },
                "systematic-debugging": if spec.selected_skills.contains(&"systematic-debugging") { "allow" } else { "deny" },
                "requesting-code-review": if spec.selected_skills.contains(&"requesting-code-review") { "allow" } else { "deny" },
                "receiving-code-review": if spec.selected_skills.contains(&"receiving-code-review") { "allow" } else { "deny" }
            })
        });
        let mut config = serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "permission": permissions,
            "agent": {
                spec.agent: {
                    "mode": "primary",
                    "permission": permissions,
                    "prompt": format!("Arena execution profile {:?}. Arena owns ProductAuthority, acceptance, verification, and Apply; this profile never grants those authorities.", self)
                }
            }
        });
        if spec.lsp {
            config["lsp"] = serde_json::json!({});
        } else {
            config["lsp"] = serde_json::Value::Bool(false);
        }
        if spec.context7 {
            config["mcp"] = serde_json::json!({
                "context7": {"type": "remote", "url": CONTEXT7_URL, "enabled": true}
            });
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_code_roles_enable_lsp() {
        assert!(!ExecutionProfile::SemanticNoTools.spec().lsp);
        assert!(!ExecutionProfile::WebResearch.spec().lsp);
        assert!(ExecutionProfile::Implementation.spec().lsp);
        assert!(ExecutionProfile::DebugRepair.spec().lsp);
        assert!(ExecutionProfile::CandidateReview.spec().lsp);
    }

    #[test]
    fn selected_skills_are_pinned_by_profile_policy() {
        assert_eq!(
            ExecutionProfile::Implementation.spec().selected_skills,
            &["test-driven-development", "verification-before-completion"]
        );
        assert_eq!(
            ExecutionProfile::DebugRepair.spec().selected_skills,
            &["systematic-debugging", "verification-before-completion"]
        );
    }

    #[test]
    fn config_cannot_grant_arena_authority_or_arbitrary_skills() {
        let config = ExecutionProfile::Implementation.authority_free_config();
        assert_eq!(config["permission"]["apply"], "deny");
        assert_eq!(config["permission"]["acceptance"], "deny");
        assert_eq!(config["permission"]["skill"]["*"], "deny");
        assert_eq!(config["mcp"], serde_json::Value::Null);
    }
}
