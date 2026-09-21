//! Arena-owned specialist routing, capability, and continuity contracts.
//!
//! This module deliberately contains policy and bounded adapters, not another
//! coordinator. Product OS admits work, SessionRuntime owns live execution,
//! and Delivery remains the implementation/verifier authority.

use crate::dsh_worker;
use crate::product_os::ProductWorkOrderRole;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

pub const POLICY_SCHEMA_VERSION: u32 = 1;
pub const MAX_CONTEXT_BYTES: usize = 32 * 1024;
pub const MAX_CHILDREN_PER_PARENT: usize = 12;
pub const MAX_DELEGATION_DEPTH: u8 = 3;
pub const CONTINUITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleFamily {
    ResearchLead,
    ProductDirector,
    ArchitectureLead,
    ImplementationEngineer,
    QaReviewLead,
}

impl RoleFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResearchLead => "research_lead",
            Self::ProductDirector => "product_director",
            Self::ArchitectureLead => "architecture_lead",
            Self::ImplementationEngineer => "implementation_engineer",
            Self::QaReviewLead => "qa_review_lead",
        }
    }

    pub fn from_product_role(role: &ProductWorkOrderRole) -> Self {
        match role {
            ProductWorkOrderRole::ResearchLead
            | ProductWorkOrderRole::Researcher
            | ProductWorkOrderRole::FactVerifier => Self::ResearchLead,
            ProductWorkOrderRole::ProductDirector => Self::ProductDirector,
            ProductWorkOrderRole::ArchitectA
            | ProductWorkOrderRole::ArchitectB
            | ProductWorkOrderRole::ChiefEngineer
            | ProductWorkOrderRole::ReuseReviewer
            | ProductWorkOrderRole::ConstraintsReviewer
            | ProductWorkOrderRole::RedTeamReviewer
            | ProductWorkOrderRole::DissentReviewer
            | ProductWorkOrderRole::FeasibilityReviewer => Self::ArchitectureLead,
            ProductWorkOrderRole::BrowserQa => Self::QaReviewLead,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelHealthStatus {
    Unknown,
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelHealthSnapshot {
    pub model_id: String,
    pub provider: String,
    pub source: String,
    pub free: bool,
    pub last_probe_at: Option<i64>,
    pub last_success: bool,
    pub latency_ms: Option<u64>,
    pub consecutive_failures: u32,
    pub status: ModelHealthStatus,
    pub error_classification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelPolicyConfig {
    pub role_family: RoleFamily,
    pub preferred_model: Option<String>,
    pub fallback_models: Vec<String>,
    pub custom_model_id: Option<String>,
    pub enabled: bool,
}

impl ModelPolicyConfig {
    pub fn for_role(role_family: RoleFamily) -> Self {
        Self {
            role_family,
            preferred_model: None,
            fallback_models: Vec::new(),
            custom_model_id: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedModelPolicy {
    pub schema_version: u32,
    pub root_role_family: RoleFamily,
    pub preferred_model: Option<String>,
    pub fallback_models: Vec<String>,
    pub resolved_model: Option<String>,
    pub policy_revision: u64,
    pub inheritance_root_work_order_id: String,
    pub resolved_at: i64,
    pub health_snapshot: Vec<ModelHealthSnapshot>,
    pub blocked_reason: Option<String>,
}

impl ResolvedModelPolicy {
    pub fn blocked(&self) -> bool {
        self.resolved_model.is_none()
    }

    pub fn inherit(&self, _parent_work_order_id: &str) -> Self {
        // Descendants inherit the original first-level policy snapshot. The
        // immediate parent changes at every delegation level, but the policy
        // root must remain stable so restart/recovery and audit can always
        // identify the owner-configured first-level role that supplied it.
        self.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCatalogEntry {
    pub model_id: String,
    pub provider: String,
    pub display_name: String,
    pub source: String,
    pub free: bool,
    pub health: ModelHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelCatalog {
    pub catalog_revision: u64,
    pub refreshed_at: Option<i64>,
    pub models: Vec<ModelCatalogEntry>,
}

impl ModelCatalog {
    pub fn verified_selectable(&self) -> Vec<ModelCatalogEntry> {
        self.models
            .iter()
            .filter(|entry| entry.free && entry.health.status == ModelHealthStatus::Healthy)
            .cloned()
            .collect()
    }

    pub fn find(&self, model_id: &str) -> Option<&ModelCatalogEntry> {
        self.models.iter().find(|entry| entry.model_id == model_id)
    }

    pub fn record_probe(
        &mut self,
        model_id: &str,
        provider: &str,
        source: &str,
        free: bool,
        success: bool,
        latency_ms: Option<u64>,
        error_classification: Option<String>,
        now: i64,
    ) {
        let failures = self
            .find(model_id)
            .map(|entry| entry.health.consecutive_failures)
            .unwrap_or(0);
        let consecutive_failures = if success {
            0
        } else {
            failures.saturating_add(1)
        };
        let status = if success {
            ModelHealthStatus::Healthy
        } else if matches!(
            error_classification.as_deref(),
            Some("timeout" | "rate_limit")
        ) {
            ModelHealthStatus::Degraded
        } else {
            ModelHealthStatus::Unavailable
        };
        let health = ModelHealthSnapshot {
            model_id: model_id.to_string(),
            provider: provider.to_string(),
            source: source.to_string(),
            free,
            last_probe_at: Some(now),
            last_success: success,
            latency_ms,
            consecutive_failures,
            status,
            error_classification,
        };
        let entry = ModelCatalogEntry {
            model_id: model_id.to_string(),
            provider: provider.to_string(),
            display_name: model_id.to_string(),
            source: source.to_string(),
            free,
            health,
        };
        self.models.retain(|existing| existing.model_id != model_id);
        self.models.push(entry);
        self.models
            .sort_by(|left, right| left.model_id.cmp(&right.model_id));
        self.catalog_revision = self.catalog_revision.saturating_add(1);
        self.refreshed_at = Some(now);
    }

    pub fn resolve(
        &self,
        config: &ModelPolicyConfig,
        root_work_order_id: &str,
        policy_revision: u64,
        now: i64,
    ) -> ResolvedModelPolicy {
        let mut candidates = Vec::new();
        if let Some(preferred) = config.preferred_model.as_deref() {
            candidates.push(preferred.to_string());
        }
        candidates.extend(config.fallback_models.iter().cloned());
        if let Some(custom) = config.custom_model_id.as_deref() {
            candidates.push(custom.to_string());
        }
        let resolved = if config.enabled {
            candidates.iter().find_map(|candidate| {
                self.find(candidate).and_then(|entry| {
                    (entry.free && entry.health.status == ModelHealthStatus::Healthy)
                        .then_some(entry.model_id.clone())
                })
            })
        } else {
            None
        };
        let health_snapshot = candidates
            .iter()
            .filter_map(|candidate| self.find(candidate).map(|entry| entry.health.clone()))
            .collect::<Vec<_>>();
        ResolvedModelPolicy {
            schema_version: POLICY_SCHEMA_VERSION,
            root_role_family: config.role_family,
            preferred_model: config.preferred_model.clone(),
            fallback_models: config.fallback_models.clone(),
            resolved_model: resolved.clone(),
            policy_revision,
            inheritance_root_work_order_id: root_work_order_id.to_string(),
            resolved_at: now,
            health_snapshot,
            blocked_reason: resolved.is_none().then(|| {
                "configured preferred and fallback models are not healthy verified free models"
                    .to_string()
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecialistSettings {
    pub schema_version: u32,
    pub policy_revision: u64,
    pub policies: BTreeMap<RoleFamily, ModelPolicyConfig>,
    pub catalog: ModelCatalog,
    #[serde(default)]
    pub custom_models: Vec<CustomApiModelConfig>,
}

impl Default for SpecialistSettings {
    fn default() -> Self {
        let roles = [
            RoleFamily::ResearchLead,
            RoleFamily::ProductDirector,
            RoleFamily::ArchitectureLead,
            RoleFamily::ImplementationEngineer,
            RoleFamily::QaReviewLead,
        ];
        Self {
            schema_version: POLICY_SCHEMA_VERSION,
            policy_revision: 0,
            policies: roles
                .into_iter()
                .map(|role| (role, ModelPolicyConfig::for_role(role)))
                .collect(),
            catalog: ModelCatalog::default(),
            custom_models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomApiModelConfig {
    pub custom_model_id: String,
    pub display_name: String,
    pub base_url: String,
    pub model_name: String,
    pub tested_at: Option<i64>,
    pub test_passed: bool,
    pub api_key_configured: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomApiModelInput {
    pub display_name: String,
    pub base_url: String,
    #[serde(default, skip_serializing)]
    pub api_key: String,
    pub model_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomModelTestResult {
    pub passed: bool,
    pub status: String,
    pub latency_ms: Option<u64>,
    pub error_classification: Option<String>,
}

pub fn validate_custom_input(input: &CustomApiModelInput) -> Result<(), String> {
    if input.display_name.trim().is_empty() || input.display_name.len() > 120 {
        return Err("custom model display name is empty or oversized".to_string());
    }
    let parsed = reqwest::Url::parse(input.base_url.trim())
        .map_err(|_| "custom model base URL is invalid".to_string())?;
    if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
        return Err("custom model base URL must be an absolute HTTP(S) URL".to_string());
    }
    crate::opencode_adapter::validate_model_identifier(input.model_name.trim())?;
    if input.api_key.trim().is_empty() {
        return Err("custom model API key is required for a bounded test".to_string());
    }
    Ok(())
}

pub fn custom_model_id(input: &CustomApiModelInput) -> String {
    use sha2::{Digest, Sha256};
    let digest =
        Sha256::digest(format!("{}:{}", input.base_url.trim(), input.model_name.trim()).as_bytes());
    format!(
        "custom:{}",
        digest
            .iter()
            .take(12)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub async fn test_custom_model(
    input: &CustomApiModelInput,
) -> Result<CustomModelTestResult, String> {
    validate_custom_input(input)?;
    let endpoint = if input
        .base_url
        .trim_end_matches('/')
        .ends_with("/chat/completions")
    {
        input.base_url.trim_end_matches('/').to_string()
    } else {
        format!("{}/chat/completions", input.base_url.trim_end_matches('/'))
    };
    let started = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("custom model client unavailable: {error}"))?;
    let response = client
        .post(endpoint)
        .bearer_auth(&input.api_key)
        .json(&serde_json::json!({
            "model": input.model_name,
            "messages": [{"role": "user", "content": "Reply with exactly ARENA_CUSTOM_PROBE_OK"}],
            "max_tokens": 8,
            "temperature": 0
        }))
        .send()
        .await;
    let latency_ms = Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            let classification = if error.is_timeout() {
                "timeout"
            } else {
                "network"
            };
            return Ok(CustomModelTestResult {
                passed: false,
                status: "unavailable".to_string(),
                latency_ms,
                error_classification: Some(classification.to_string()),
            });
        }
    };
    if !response.status().is_success() {
        let classification = if response.status().as_u16() == 429 {
            "rate_limit"
        } else if response.status().is_client_error() {
            "auth_or_request"
        } else {
            "provider_error"
        };
        return Ok(CustomModelTestResult {
            passed: false,
            status: "unavailable".to_string(),
            latency_ms,
            error_classification: Some(classification.to_string()),
        });
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "custom model returned malformed JSON".to_string())?;
    let content = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let passed = content.trim() == "ARENA_CUSTOM_PROBE_OK";
    Ok(CustomModelTestResult {
        passed,
        status: if passed { "healthy" } else { "degraded" }.to_string(),
        latency_ms,
        error_classification: (!passed).then(|| "probe_marker_mismatch".to_string()),
    })
}

impl SpecialistSettings {
    pub fn policy_for(&self, role: RoleFamily) -> ModelPolicyConfig {
        self.policies
            .get(&role)
            .cloned()
            .unwrap_or_else(|| ModelPolicyConfig::for_role(role))
    }

    pub fn update_policy(&mut self, policy: ModelPolicyConfig) -> Result<(), String> {
        validate_policy(&policy)?;
        self.policy_revision = self.policy_revision.saturating_add(1);
        self.policies.insert(policy.role_family, policy);
        Ok(())
    }
}

pub fn validate_policy(policy: &ModelPolicyConfig) -> Result<(), String> {
    if policy.fallback_models.len() > 8 {
        return Err("specialist model fallback list is too long".to_string());
    }
    let mut ids = BTreeSet::new();
    for model in policy
        .preferred_model
        .iter()
        .chain(policy.fallback_models.iter())
        .chain(policy.custom_model_id.iter())
    {
        let id = crate::opencode_adapter::validate_model_identifier(model)?;
        if !ids.insert(id) {
            return Err("specialist model policy contains duplicate candidates".to_string());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionContextBundle {
    pub project_id: String,
    pub run_id: String,
    pub authority_revision: u64,
    pub founder_intent: String,
    pub owner_directives: Vec<String>,
    pub objective: String,
    pub root_work_order_id: String,
    pub parent_work_order_id: Option<String>,
    pub ancestor_summaries: Vec<String>,
    pub adopted_decisions: Vec<String>,
    pub evidence_references: Vec<String>,
    pub unresolved_blockers: Vec<String>,
    pub repository_identity: String,
    pub specialist_template_id: String,
    pub skill_ids: Vec<String>,
    pub inherited_model_policy: ResolvedModelPolicy,
    pub previous_attempt_summary: Option<String>,
    pub output_contract: String,
}

impl ExecutionContextBundle {
    pub fn validate(&self) -> Result<(), String> {
        let encoded = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        if encoded.len() > MAX_CONTEXT_BYTES {
            return Err("execution context bundle exceeds Arena bound".to_string());
        }
        for value in [
            &self.project_id,
            &self.run_id,
            &self.objective,
            &self.root_work_order_id,
            &self.repository_identity,
            &self.specialist_template_id,
            &self.output_contract,
        ] {
            if value.trim().is_empty() {
                return Err(
                    "execution context bundle has an empty identity or contract".to_string()
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuityStatus {
    Running,
    ReconciliationRequired,
    Completed,
    Blocked,
}

/// Durable result fence for a work order. A runtime session can be reused only
/// when its identity and epoch still match; otherwise the caller reconstructs
/// a fresh worker from the persisted context bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionContinuityState {
    pub schema_version: u32,
    pub work_order_id: String,
    pub root_work_order_id: String,
    pub execution_epoch: u64,
    pub authority_revision: u64,
    pub runtime_session_id: Option<String>,
    pub status: ContinuityStatus,
}

impl ExecutionContinuityState {
    pub fn recover_after_restart(&mut self) {
        if self.status == ContinuityStatus::Running {
            self.status = ContinuityStatus::ReconciliationRequired;
        }
    }

    pub fn fence_owner_guidance(&mut self, authority_revision: u64) {
        self.execution_epoch = self.execution_epoch.saturating_add(1);
        self.authority_revision = authority_revision;
        self.status = ContinuityStatus::ReconciliationRequired;
    }

    pub fn runtime_session_reusable(
        &self,
        runtime_session_id: Option<&str>,
        work_order_id: &str,
        execution_epoch: u64,
        authority_revision: u64,
    ) -> bool {
        self.status == ContinuityStatus::Running
            && self.runtime_session_id.as_deref() == runtime_session_id
            && self.runtime_session_id.is_some()
            && self.work_order_id == work_order_id
            && self.execution_epoch == execution_epoch
            && self.authority_revision == authority_revision
    }

    pub fn accept_result(
        &mut self,
        execution_epoch: u64,
        authority_revision: u64,
    ) -> Result<(), String> {
        if self.status != ContinuityStatus::Running
            || self.execution_epoch != execution_epoch
            || self.authority_revision != authority_revision
        {
            return Err("continuity result is stale or requires reconciliation".to_string());
        }
        self.status = ContinuityStatus::Completed;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecialistCapabilityDefinition {
    pub specialist_id: String,
    pub role_family: RoleFamily,
    pub capabilities: Vec<String>,
    pub trigger_hints: Vec<String>,
    pub allowed_child_capability_classes: Vec<String>,
    pub default_skill_ids: Vec<String>,
    pub tool_profile: String,
    pub maximum_delegation_depth: u8,
    pub authority_restrictions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildWorkProposal {
    pub capability_class: String,
    pub objective: String,
    pub specialist_id: String,
    pub model_hint: Option<String>,
}

pub fn curated_specialists() -> Vec<SpecialistCapabilityDefinition> {
    vec![
        SpecialistCapabilityDefinition {
            specialist_id: "ecc.code-explorer".to_string(),
            role_family: RoleFamily::ArchitectureLead,
            capabilities: vec!["repo_scan".to_string(), "dependency_mapping".to_string()],
            trigger_hints: vec!["unknown repository area".to_string()],
            allowed_child_capability_classes: vec!["bounded_research".to_string()],
            default_skill_ids: vec!["ecc.repo-scan".to_string()],
            tool_profile: "read_only_repository".to_string(),
            maximum_delegation_depth: 2,
            authority_restrictions: vec!["cannot mutate ProductAuthority".to_string()],
        },
        SpecialistCapabilityDefinition {
            specialist_id: "ecc.spec-miner".to_string(),
            role_family: RoleFamily::ProductDirector,
            capabilities: vec!["specification_mining".to_string()],
            trigger_hints: vec!["requirements are ambiguous".to_string()],
            allowed_child_capability_classes: Vec::new(),
            default_skill_ids: vec!["ecc.product-lens".to_string()],
            tool_profile: "bounded_semantic_review".to_string(),
            maximum_delegation_depth: 2,
            authority_restrictions: vec!["proposals require Arena admission".to_string()],
        },
        SpecialistCapabilityDefinition {
            specialist_id: "ecc.code-reviewer".to_string(),
            role_family: RoleFamily::QaReviewLead,
            capabilities: vec!["semantic_review".to_string()],
            trigger_hints: vec!["candidate needs independent review".to_string()],
            allowed_child_capability_classes: vec!["verification_evidence".to_string()],
            default_skill_ids: vec!["ecc.verification-loop".to_string()],
            tool_profile: "candidate_review".to_string(),
            maximum_delegation_depth: 2,
            authority_restrictions: vec!["cannot mark Verified or Apply".to_string()],
        },
    ]
}

pub fn curated_ecc_resource(resource_kind: &str, resource_id: &str) -> Option<Value> {
    let catalog =
        serde_json::from_str::<Value>(include_str!("../resources/specialists/ecc/catalog.json"))
            .ok()?;
    let values = catalog.get(resource_kind)?.as_array()?;
    values
        .iter()
        .find(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == resource_id)
        })
        .cloned()
}

pub fn admit_child_proposal(
    parent: &ResolvedModelPolicy,
    parent_depth: u8,
    current_project_revision: u64,
    admitted_project_revision: u64,
    existing_child_count: usize,
    definition: &SpecialistCapabilityDefinition,
    proposal: &ChildWorkProposal,
) -> Result<ResolvedModelPolicy, String> {
    if current_project_revision != admitted_project_revision {
        return Err("specialist proposal belongs to a stale Product OS revision".to_string());
    }
    if parent_depth >= MAX_DELEGATION_DEPTH || parent_depth >= definition.maximum_delegation_depth {
        return Err("specialist delegation depth is exhausted".to_string());
    }
    if existing_child_count >= MAX_CHILDREN_PER_PARENT {
        return Err("specialist child budget is exhausted".to_string());
    }
    if !definition
        .allowed_child_capability_classes
        .iter()
        .any(|class| class == &proposal.capability_class)
    {
        return Err("parent specialist is not authorized for this child capability".to_string());
    }
    if proposal.objective.trim().is_empty() || proposal.objective.len() > 4_000 {
        return Err("specialist child objective is empty or oversized".to_string());
    }
    // The proposal's model hint is intentionally ignored. The inherited root
    // policy is the only model-routing authority.
    Ok(parent.clone())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentReachDoctorChannel {
    pub channel: String,
    pub active_backend: Option<String>,
    pub health_status: String,
    pub observed_at: Option<i64>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentReachDoctorReport {
    pub version: Option<String>,
    pub channels: Vec<AgentReachDoctorChannel>,
}

pub fn parse_agent_reach_doctor(
    raw: &str,
    observed_at: i64,
) -> Result<AgentReachDoctorReport, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("Agent Reach doctor JSON is malformed: {error}"))?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    // Current Agent Reach prints check_all() directly for doctor --json, so
    // the top-level object itself is the channel map. Older/fixture wrappers
    // may still place that map under "channels"; accept both shapes.
    let registry = value.get("channels").unwrap_or(&value);
    let mut channels = Vec::new();
    match registry {
        Value::Array(entries) => {
            for entry in entries {
                if let Some(channel) = parse_agent_reach_channel(entry, None, observed_at) {
                    channels.push(channel);
                }
            }
        }
        Value::Object(entries) => {
            for (name, entry) in entries {
                if name == "version" || name == "channels" {
                    continue;
                }
                if let Some(channel) = parse_agent_reach_channel(entry, Some(name), observed_at) {
                    channels.push(channel);
                }
            }
        }
        _ => return Err("Agent Reach doctor channel registry has an invalid shape".to_string()),
    }
    if channels.is_empty() {
        return Err("Agent Reach doctor channel registry is empty".to_string());
    }
    Ok(AgentReachDoctorReport { version, channels })
}

fn parse_agent_reach_channel(
    entry: &Value,
    fallback_name: Option<&str>,
    observed_at: i64,
) -> Option<AgentReachDoctorChannel> {
    // In current upstream doctor JSON the map key is the stable channel ID and
    // "name" is a human-readable description. Prefer the key when present.
    let channel = entry
        .get("channel")
        .and_then(Value::as_str)
        .or(fallback_name)
        .or_else(|| entry.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_ascii_lowercase();
    let healthy = entry
        .get("healthy")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            entry
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| matches!(status, "ok" | "healthy" | "ready" | "available"))
        });
    let active_backend = entry
        .get("active_backend")
        .or_else(|| entry.get("backend"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let reason = entry
        .get("reason")
        .or_else(|| entry.get("error"))
        .or_else(|| entry.get("message"))
        .and_then(Value::as_str)
        .map(|value| value.chars().take(400).collect());
    Some(AgentReachDoctorChannel {
        channel,
        active_backend,
        health_status: if healthy { "healthy" } else { "unavailable" }.to_string(),
        observed_at: Some(observed_at),
        unavailable_reason: if healthy { None } else { reason },
    })
}

pub async fn probe_opencode_model(
    model_id: &str,
    cwd: &Path,
) -> Result<ModelHealthSnapshot, String> {
    let model_id = crate::opencode_adapter::validate_model_identifier(model_id)?;
    let started = std::time::Instant::now();
    let output = dsh_worker::run_contained_command(
        &crate::opencode_adapter::executable(),
        &[
            "run".into(),
            "--model".into(),
            model_id.clone().into(),
            "--format".into(),
            "json".into(),
            "Reply with exactly ARENA_MODEL_PROBE_OK".into(),
        ],
        cwd,
        Duration::from_secs(45),
    )
    .await?;
    let combined = format!("{}\n{}", output.stdout, output.stderr);
    let success = output.exit_code == Some(0)
        && !output.timed_out
        && combined.contains("ARENA_MODEL_PROBE_OK");
    let error_classification = if output.timed_out {
        Some("timeout".to_string())
    } else if output.exit_code == Some(0) && !success {
        Some("empty_or_malformed_response".to_string())
    } else if combined.to_ascii_lowercase().contains("rate") {
        Some("rate_limit".to_string())
    } else if !success {
        Some("execution_failure".to_string())
    } else {
        None
    };
    Ok(ModelHealthSnapshot {
        model_id: model_id.clone(),
        provider: model_id.split('/').next().unwrap_or("unknown").to_string(),
        source: "opencode_models_probe".to_string(),
        free: model_id.contains("free"),
        last_probe_at: Some(chrono::Utc::now().timestamp()),
        last_success: success,
        latency_ms: Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
        consecutive_failures: u32::from(!success),
        status: if success {
            ModelHealthStatus::Healthy
        } else {
            ModelHealthStatus::Unavailable
        },
        error_classification,
    })
}

fn candidate_model_id(value: &str) -> Option<String> {
    let value = value.trim_matches(|character: char| {
        !character.is_ascii_alphanumeric()
            && character != '/'
            && character != '-'
            && character != '_'
            && character != '.'
            && character != ':'
    });
    (value.contains('/') && crate::opencode_adapter::validate_model_identifier(value).is_ok())
        .then(|| value.to_string())
}

pub fn parse_opencode_model_catalog(raw: &str) -> Vec<(String, String, bool)> {
    let mut result = BTreeMap::<String, (String, bool)>::new();
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        collect_json_models(&value, &mut result, false);
    }
    for line in raw.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(model) = line.split_whitespace().find_map(candidate_model_id) {
            // Fail closed. "Zen"/"OpenCode" identifies a provider, not a
            // pricing tier; paid and free models can coexist there. Text-mode
            // discovery is considered free only when the emitted model line
            // explicitly says so. Structured metadata remains preferred.
            let free = lower.contains("free");
            result
                .entry(model)
                .or_insert(("opencode".to_string(), free));
        }
    }
    result
        .into_iter()
        .map(|(model, (provider, free))| (model, provider, free))
        .collect()
}

fn collect_json_models(
    value: &Value,
    output: &mut BTreeMap<String, (String, bool)>,
    inherited_free: bool,
) {
    match value {
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_json_models(value, output, inherited_free)),
        Value::Object(map) => {
            let free = map
                .get("free")
                .and_then(Value::as_bool)
                .unwrap_or(inherited_free)
                || map
                    .get("tier")
                    .and_then(Value::as_str)
                    .is_some_and(|tier| tier == "free");
            let provider = map
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("opencode")
                .to_string();
            for key in ["id", "model", "model_id"] {
                if let Some(model) = map
                    .get(key)
                    .and_then(Value::as_str)
                    .and_then(candidate_model_id)
                {
                    output.insert(model, (provider.clone(), free));
                }
            }
            map.values()
                .for_each(|value| collect_json_models(value, output, free));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_catalog() -> ModelCatalog {
        let mut catalog = ModelCatalog::default();
        catalog.record_probe(
            "zen/model-a-free",
            "zen",
            "fixture",
            true,
            true,
            Some(20),
            None,
            1,
        );
        catalog.record_probe(
            "zen/model-b-free",
            "zen",
            "fixture",
            true,
            true,
            Some(25),
            None,
            1,
        );
        catalog
    }

    #[test]
    fn direct_child_inherits_root_policy() {
        let catalog = healthy_catalog();
        let config = ModelPolicyConfig {
            role_family: RoleFamily::ResearchLead,
            preferred_model: Some("zen/model-a-free".to_string()),
            fallback_models: vec!["zen/model-b-free".to_string()],
            custom_model_id: None,
            enabled: true,
        };
        let root = catalog.resolve(&config, "root", 1, 10);
        assert_eq!(root.resolved_model.as_deref(), Some("zen/model-a-free"));
        assert_eq!(root.inherit("child").resolved_model, root.resolved_model);
    }

    #[test]
    fn grandchild_cannot_replace_inherited_model_with_hint() {
        let catalog = healthy_catalog();
        let config = ModelPolicyConfig {
            role_family: RoleFamily::ResearchLead,
            preferred_model: Some("zen/model-a-free".to_string()),
            fallback_models: Vec::new(),
            custom_model_id: None,
            enabled: true,
        };
        let root = catalog.resolve(&config, "root", 1, 10);
        let definition = curated_specialists().remove(0);
        let proposal = ChildWorkProposal {
            capability_class: "bounded_research".to_string(),
            objective: "inspect dependencies".to_string(),
            specialist_id: definition.specialist_id.clone(),
            model_hint: Some("invented/paid-model".to_string()),
        };
        let child = admit_child_proposal(&root, 1, 2, 2, 0, &definition, &proposal)
            .expect("bounded child admission");
        assert_eq!(child.resolved_model.as_deref(), Some("zen/model-a-free"));
    }

    #[test]
    fn unhealthy_preferred_uses_only_verified_free_fallback() {
        let mut catalog = healthy_catalog();
        catalog.record_probe(
            "zen/model-a-free",
            "zen",
            "fixture",
            true,
            false,
            None,
            Some("timeout".to_string()),
            2,
        );
        let config = ModelPolicyConfig {
            role_family: RoleFamily::ResearchLead,
            preferred_model: Some("zen/model-a-free".to_string()),
            fallback_models: vec!["zen/model-b-free".to_string()],
            custom_model_id: None,
            enabled: true,
        };
        assert_eq!(
            catalog
                .resolve(&config, "root", 1, 2)
                .resolved_model
                .as_deref(),
            Some("zen/model-b-free")
        );
    }

    #[test]
    fn all_unhealthy_models_block_without_silent_paid_fallback() {
        let mut catalog = healthy_catalog();
        catalog.record_probe(
            "zen/model-a-free",
            "zen",
            "fixture",
            true,
            false,
            None,
            Some("auth".to_string()),
            2,
        );
        catalog.record_probe(
            "zen/model-b-free",
            "zen",
            "fixture",
            true,
            false,
            None,
            Some("rate_limit".to_string()),
            2,
        );
        let config = ModelPolicyConfig {
            role_family: RoleFamily::ResearchLead,
            preferred_model: Some("zen/model-a-free".to_string()),
            fallback_models: vec!["zen/model-b-free".to_string(), "paid/model".to_string()],
            custom_model_id: None,
            enabled: true,
        };
        assert!(catalog.resolve(&config, "root", 1, 2).blocked());
    }

    #[test]
    fn policy_revision_change_does_not_mutate_existing_snapshot() {
        let catalog = healthy_catalog();
        let first = catalog.resolve(
            &ModelPolicyConfig {
                role_family: RoleFamily::ResearchLead,
                preferred_model: Some("zen/model-a-free".to_string()),
                fallback_models: Vec::new(),
                custom_model_id: None,
                enabled: true,
            },
            "root",
            4,
            10,
        );
        let second = catalog.resolve(
            &ModelPolicyConfig {
                role_family: RoleFamily::ResearchLead,
                preferred_model: Some("zen/model-b-free".to_string()),
                fallback_models: Vec::new(),
                custom_model_id: None,
                enabled: true,
            },
            "new-root",
            5,
            11,
        );
        assert_eq!(first.resolved_model.as_deref(), Some("zen/model-a-free"));
        assert_eq!(second.resolved_model.as_deref(), Some("zen/model-b-free"));
        assert_eq!(first.policy_revision, 4);
    }

    #[test]
    fn failed_probe_is_not_selectable() {
        let mut catalog = ModelCatalog::default();
        catalog.record_probe(
            "zen/failed-free",
            "zen",
            "fixture",
            true,
            false,
            None,
            Some("timeout".to_string()),
            1,
        );
        assert!(catalog.verified_selectable().is_empty());
    }

    #[test]
    fn doctor_json_preserves_backend_and_unavailable_reason() {
        let report = parse_agent_reach_doctor(
            r#"{"version":"0.4.0","channels":[{"name":"github","backend":"gh","healthy":true},{"channel":"reddit","status":"unavailable","reason":"login required"}]}"#,
            10,
        )
        .expect("doctor report");
        assert_eq!(report.channels[0].active_backend.as_deref(), Some("gh"));
        assert_eq!(
            report.channels[1].unavailable_reason.as_deref(),
            Some("login required")
        );
    }

    #[test]
    fn doctor_json_accepts_upstream_style_channel_map() {
        let report = parse_agent_reach_doctor(
            r#"{"version":"0.4.0","channels":{"web":{"status":"available","backend":"exa"},"x":{"status":"unavailable","error":"not configured"}}}"#,
            11,
        )
        .expect("doctor channel map");
        assert_eq!(report.channels.len(), 2);
        assert!(report.channels.iter().any(|channel| {
            channel.channel == "web" && channel.active_backend.as_deref() == Some("exa")
        }));
    }

    #[test]
    fn doctor_json_accepts_current_upstream_direct_map_and_ok_status() {
        let report = parse_agent_reach_doctor(
            r#"{"github":{"status":"ok","name":"GitHub repositories","message":"gh available","tier":0,"backends":["gh"],"active_backend":"gh"},"reddit":{"status":"warn","name":"Reddit","message":"login required","tier":1,"backends":["rdt"],"active_backend":null}}"#,
            12,
        )
        .expect("current upstream doctor channel map");
        let github = report
            .channels
            .iter()
            .find(|channel| channel.channel == "github")
            .expect("github channel id must come from the map key");
        assert_eq!(github.health_status, "healthy");
        assert_eq!(github.active_backend.as_deref(), Some("gh"));
        let reddit = report
            .channels
            .iter()
            .find(|channel| channel.channel == "reddit")
            .expect("reddit channel");
        assert_eq!(reddit.health_status, "unavailable");
        assert_eq!(reddit.unavailable_reason.as_deref(), Some("login required"));
    }

    #[test]
    fn inherited_policy_keeps_first_level_root_identity() {
        let catalog = healthy_catalog();
        let root = catalog.resolve(
            &ModelPolicyConfig {
                role_family: RoleFamily::ResearchLead,
                preferred_model: Some("zen/model-a-free".to_string()),
                fallback_models: Vec::new(),
                custom_model_id: None,
                enabled: true,
            },
            "root-work-order",
            1,
            10,
        );
        let child = root.inherit("child-work-order");
        let grandchild = child.inherit("grandchild-parent");
        assert_eq!(
            grandchild.inheritance_root_work_order_id,
            "root-work-order"
        );
    }

    #[test]
    fn context_bundle_is_bounded_and_policy_has_no_secret_field() {
        let policy = healthy_catalog().resolve(
            &ModelPolicyConfig::for_role(RoleFamily::ResearchLead),
            "root",
            1,
            1,
        );
        let bundle = ExecutionContextBundle {
            project_id: "p".to_string(),
            run_id: "r".to_string(),
            authority_revision: 1,
            founder_intent: "intent".to_string(),
            owner_directives: Vec::new(),
            objective: "objective".to_string(),
            root_work_order_id: "root".to_string(),
            parent_work_order_id: None,
            ancestor_summaries: Vec::new(),
            adopted_decisions: Vec::new(),
            evidence_references: Vec::new(),
            unresolved_blockers: Vec::new(),
            repository_identity: "repo".to_string(),
            specialist_template_id: "ecc.code-explorer".to_string(),
            skill_ids: Vec::new(),
            inherited_model_policy: policy,
            previous_attempt_summary: None,
            output_contract: "bounded result".to_string(),
        };
        bundle.validate().expect("bounded context");
        let encoded = serde_json::to_string(&bundle).expect("encode");
        assert!(!encoded.contains("api_key"));
    }

    #[test]
    fn continuity_reconciles_restart_and_rejects_stale_result() {
        let mut state = ExecutionContinuityState {
            schema_version: CONTINUITY_SCHEMA_VERSION,
            work_order_id: "child".to_string(),
            root_work_order_id: "root".to_string(),
            execution_epoch: 4,
            authority_revision: 8,
            runtime_session_id: Some("runtime-4".to_string()),
            status: ContinuityStatus::Running,
        };
        assert!(state.runtime_session_reusable(Some("runtime-4"), "child", 4, 8));
        state.recover_after_restart();
        assert_eq!(state.status, ContinuityStatus::ReconciliationRequired);
        assert!(!state.runtime_session_reusable(Some("runtime-4"), "child", 4, 8));
        assert!(state.accept_result(4, 8).is_err());
    }

    #[test]
    fn owner_guidance_fences_old_epoch_and_requires_replan() {
        let mut state = ExecutionContinuityState {
            schema_version: CONTINUITY_SCHEMA_VERSION,
            work_order_id: "child".to_string(),
            root_work_order_id: "root".to_string(),
            execution_epoch: 1,
            authority_revision: 2,
            runtime_session_id: None,
            status: ContinuityStatus::Running,
        };
        state.fence_owner_guidance(3);
        assert_eq!(state.execution_epoch, 2);
        assert_eq!(state.status, ContinuityStatus::ReconciliationRequired);
        assert!(state.accept_result(1, 2).is_err());
    }

    #[test]
    fn ecc_resource_loading_is_allowlisted_and_versioned() {
        assert!(curated_ecc_resource("agents", "code-explorer").is_some());
        assert!(curated_ecc_resource("skills", "deep-research").is_some());
        assert!(curated_ecc_resource("agents", "arbitrary-shell-runner").is_none());
        let catalog: Value =
            serde_json::from_str(include_str!("../resources/specialists/ecc/catalog.json"))
                .expect("catalog");
        assert_eq!(
            catalog.get("upstream_commit").and_then(Value::as_str),
            Some("934195f955cf0da847d59fcd6f68856bce112d8b")
        );
    }
}
