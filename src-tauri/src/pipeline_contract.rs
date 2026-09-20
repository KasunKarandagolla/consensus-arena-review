//! Small, Arena-owned controller contracts used by the Product OS coordinator.
//!
//! This module is intentionally data-and-predicate oriented.  It is not a
//! workflow engine and it never grants a worker authority over product
//! decisions, verification, or Apply.

use crate::evidence_gates::{DecisionOutcome, GateId, GateStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductRoute {
    NewProduct,
    ExistingFeature,
    Incident,
}

impl Default for ProductRoute {
    fn default() -> Self {
        Self::NewProduct
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    Discover,
    Decide,
    Deliver,
    Release,
    ReproduceDiagnose,
    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmittedStage {
    pub stage: PipelineStage,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePlan {
    pub route: ProductRoute,
    pub stages: Vec<PipelineStage>,
    pub omitted_stages: Vec<OmittedStage>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchitecturePlanningMode {
    EstablishedPattern,
    CompetingProposals,
}

/// Cheap, deterministic planning predicate. It is deliberately conservative:
/// only explicit existing-repository, localized extension language may bypass
/// proposal competition.
pub fn architecture_planning_mode(route: ProductRoute, intent: &str) -> ArchitecturePlanningMode {
    let lower = intent.to_ascii_lowercase();
    if route == ProductRoute::ExistingFeature
        && [
            "csv export",
            "straightforward",
            "established",
            "localized",
            "existing utility",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        ArchitecturePlanningMode::EstablishedPattern
    } else {
        ArchitecturePlanningMode::CompetingProposals
    }
}

pub fn select_route(intent: &str) -> ProductRoute {
    select_route_with_context(intent, false)
}

pub fn select_route_with_context(
    intent: &str,
    repository_has_product_context: bool,
) -> ProductRoute {
    let lower = intent.to_ascii_lowercase();
    let explicit_existing_context = [
        "existing app",
        "existing repo",
        "existing repository",
        "existing project",
        "existing feature",
        "current product",
        "current app",
        "current service",
        "this repo",
        "this repository",
        "this codebase",
        "this project",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let explicit_greenfield_context = [
        "new product",
        "new app",
        "new application",
        "new service",
        "greenfield",
        "from scratch",
        "build a new",
        "create a new",
        "start a new",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    // Repository contents are a useful disambiguator for terse requests such
    // as "add export" or "fix the crash", but a starter/scaffold repository
    // must not override an explicit founder statement that this is greenfield.
    let existing_context = explicit_existing_context
        || (repository_has_product_context && !explicit_greenfield_context);
    let incident_language = [
        "incident",
        "outage",
        "regression",
        "crash",
        "broken current app",
        "broken current service",
        "production failure",
        "service down",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || (existing_context
            && [
                "fix bug",
                "bug in",
                "error in",
                "fails when",
                "not working",
                "broken ",
            ]
            .iter()
            .any(|marker| lower.contains(marker)));
    if explicit_greenfield_context && !explicit_existing_context {
        ProductRoute::NewProduct
    } else if incident_language {
        ProductRoute::Incident
    } else if existing_context
        && [
            "existing feature",
            "add ",
            "modify",
            "extend",
            "implement",
            "change ",
            "update ",
            "improve ",
            "enable ",
            "support ",
            "remove ",
            "replace ",
            "fix ",
            "refactor",
            "want ",
            "need ",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        ProductRoute::ExistingFeature
    } else {
        ProductRoute::NewProduct
    }
}

pub fn route_plan(route: ProductRoute) -> RoutePlan {
    match route {
        ProductRoute::NewProduct => RoutePlan {
            route,
            stages: vec![
                PipelineStage::Discover,
                PipelineStage::Decide,
                PipelineStage::Deliver,
                PipelineStage::Release,
            ],
            omitted_stages: Vec::new(),
        },
        ProductRoute::ExistingFeature => RoutePlan {
            route,
            stages: vec![
                PipelineStage::Decide,
                PipelineStage::Deliver,
                PipelineStage::Release,
            ],
            omitted_stages: vec![OmittedStage {
                stage: PipelineStage::Discover,
                reason:
                    "existing repository feature request; broad market discovery is not relevant"
                        .to_string(),
            }],
        },
        ProductRoute::Incident => RoutePlan {
            route,
            stages: vec![
                PipelineStage::ReproduceDiagnose,
                PipelineStage::Decide,
                PipelineStage::Deliver,
                PipelineStage::Release,
            ],
            omitted_stages: vec![OmittedStage {
                stage: PipelineStage::Discover,
                reason: "incident handling begins with reproduction and diagnosis".to_string(),
            }],
        },
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnerDecisionKind {
    AuthorizeValidationExperiment,
    AuthorizeNarrowBuild,
    AuthorizeBuild,
    ContinueEvaluation,
    StopRun,
    PivotRun,
    ApproveApply,
    ApproveRelease,
}

pub fn owner_decision_for_option(
    outcome: Option<DecisionOutcome>,
    option: &str,
) -> Result<OwnerDecisionKind, String> {
    let option = option.trim();
    let decision = match option {
        "authorize_validation_experiment"
            if outcome == Some(DecisionOutcome::ValidationExperiment) =>
        {
            OwnerDecisionKind::AuthorizeValidationExperiment
        }
        "authorize_narrow_build" | "narrow_build"
            if outcome == Some(DecisionOutcome::NarrowBuild) =>
        {
            OwnerDecisionKind::AuthorizeNarrowBuild
        }
        "authorize_build" => OwnerDecisionKind::AuthorizeBuild,
        "continue_evaluation" => OwnerDecisionKind::ContinueEvaluation,
        "stop" => OwnerDecisionKind::StopRun,
        "pivot" => OwnerDecisionKind::PivotRun,
        "approve_apply" => OwnerDecisionKind::ApproveApply,
        "approve_release" => OwnerDecisionKind::ApproveRelease,
        _ => return Err("owner decision option is not valid for the current question".to_string()),
    };
    Ok(decision)
}

pub fn owner_decision_matches_pending(
    pending: OwnerDecisionKind,
    decision: OwnerDecisionKind,
) -> bool {
    match pending {
        OwnerDecisionKind::AuthorizeValidationExperiment => matches!(
            decision,
            OwnerDecisionKind::AuthorizeValidationExperiment
                | OwnerDecisionKind::StopRun
                | OwnerDecisionKind::PivotRun
        ),
        OwnerDecisionKind::AuthorizeNarrowBuild => matches!(
            decision,
            OwnerDecisionKind::AuthorizeNarrowBuild
                | OwnerDecisionKind::StopRun
                | OwnerDecisionKind::PivotRun
        ),
        OwnerDecisionKind::AuthorizeBuild => matches!(
            decision,
            OwnerDecisionKind::AuthorizeBuild
                | OwnerDecisionKind::StopRun
                | OwnerDecisionKind::PivotRun
        ),
        OwnerDecisionKind::StopRun | OwnerDecisionKind::PivotRun => matches!(
            decision,
            OwnerDecisionKind::ContinueEvaluation
                | OwnerDecisionKind::StopRun
                | OwnerDecisionKind::PivotRun
        ),
        OwnerDecisionKind::ApproveApply => matches!(
            decision,
            OwnerDecisionKind::ApproveApply | OwnerDecisionKind::StopRun
        ),
        OwnerDecisionKind::ApproveRelease => matches!(
            decision,
            OwnerDecisionKind::ApproveRelease | OwnerDecisionKind::StopRun
        ),
        OwnerDecisionKind::ContinueEvaluation => matches!(
            decision,
            OwnerDecisionKind::ContinueEvaluation
                | OwnerDecisionKind::StopRun
                | OwnerDecisionKind::PivotRun
        ),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateRemediationOutcome {
    Satisfied,
    NeedsResearch,
    NeedsOwnerDecision,
    NeedsExperiment,
    NeedsArchitectureRevision,
    NeedsRepair,
    ExternalBlock,
    RecommendStop,
    RecommendPivot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateRemediation {
    pub gate: GateId,
    pub outcome: GateRemediationOutcome,
    pub attempt: u8,
    pub max_attempts: u8,
    pub reason: String,
    /// The next concrete Arena-owned action. This is intentionally descriptive
    /// rather than a workflow DSL; the coordinator remains the executor.
    pub action: String,
}

pub fn route_gate_remediation(gate: GateId, status: GateStatus, attempt: u8) -> GateRemediation {
    const MAX_ATTEMPTS: u8 = 2;
    let bounded_attempt = attempt.min(MAX_ATTEMPTS);
    let outcome = if status == GateStatus::Pass {
        GateRemediationOutcome::Satisfied
    } else if bounded_attempt >= MAX_ATTEMPTS {
        match gate {
            GateId::ProblemResearch | GateId::Positioning => GateRemediationOutcome::RecommendPivot,
            GateId::Ambiguity | GateId::BuildReadiness => {
                GateRemediationOutcome::NeedsOwnerDecision
            }
            GateId::Architecture | GateId::Reuse => GateRemediationOutcome::RecommendStop,
            _ => GateRemediationOutcome::ExternalBlock,
        }
    } else {
        match gate {
            GateId::ProblemResearch | GateId::Positioning => GateRemediationOutcome::NeedsResearch,
            GateId::Ambiguity => GateRemediationOutcome::NeedsOwnerDecision,
            GateId::Reuse | GateId::Architecture => {
                GateRemediationOutcome::NeedsArchitectureRevision
            }
            GateId::BuildReadiness | GateId::Implementation => GateRemediationOutcome::NeedsRepair,
            GateId::Release => GateRemediationOutcome::NeedsOwnerDecision,
            GateId::Vision => GateRemediationOutcome::NeedsExperiment,
        }
    };
    GateRemediation {
        gate,
        outcome,
        attempt: bounded_attempt,
        max_attempts: MAX_ATTEMPTS,
        reason: format!(
            "{gate:?} returned {status:?}; remediation is bounded at {MAX_ATTEMPTS} attempts"
        ),
        action: match outcome {
            GateRemediationOutcome::NeedsResearch => {
                "create targeted research for the missing decision claim".to_string()
            }
            GateRemediationOutcome::NeedsArchitectureRevision => {
                "re-run only invalidated architecture decision elements".to_string()
            }
            GateRemediationOutcome::NeedsRepair => {
                "repair the exact missing BuildReadiness predicate".to_string()
            }
            GateRemediationOutcome::NeedsExperiment => {
                "create and execute the bounded ExperimentContract".to_string()
            }
            GateRemediationOutcome::NeedsOwnerDecision => {
                "persist a typed owner question for the unresolved authority choice".to_string()
            }
            GateRemediationOutcome::ExternalBlock => {
                "persist the exact missing external prerequisite".to_string()
            }
            GateRemediationOutcome::RecommendStop => {
                "persist a stop recommendation for owner disposition".to_string()
            }
            GateRemediationOutcome::RecommendPivot => {
                "persist a pivot recommendation for owner disposition".to_string()
            }
            GateRemediationOutcome::Satisfied => "no remediation required".to_string(),
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedInputManifest {
    pub project_id: String,
    pub project_revision: u64,
    pub role: String,
    pub required_inputs: Vec<String>,
    pub resolved_content: String,
    pub packet_hash: Option<String>,
}

impl ResolvedInputManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.project_id.trim().is_empty() || self.role.trim().is_empty() {
            return Err("resolved input manifest identity is missing".to_string());
        }
        if self.required_inputs.is_empty() || self.resolved_content.trim().is_empty() {
            return Err("resolved input manifest must contain delivered content".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureProposalInput {
    pub evidence_id: String,
    /// The complete typed proposal is hashed into the immutable review packet.
    pub proposal: String,
    pub assumptions: Vec<String>,
    pub reuse_choices: Vec<String>,
    pub interfaces: Vec<String>,
    pub risks: Vec<String>,
    /// Kept as a compatibility alias for older persisted packets. New packets
    /// set this to the same value as `proposal`.
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureReviewPacket {
    pub project_id: String,
    pub project_revision: u64,
    pub proposal_a: ArchitectureProposalInput,
    #[serde(default)]
    pub proposal_b: Option<ArchitectureProposalInput>,
    pub packet_hash: String,
}

impl ArchitectureReviewPacket {
    pub fn resolve(
        project_id: String,
        project_revision: u64,
        proposal_a: ArchitectureProposalInput,
        proposal_b: ArchitectureProposalInput,
    ) -> Result<Self, String> {
        if proposal_a.evidence_id.trim().is_empty()
            || proposal_b.evidence_id.trim().is_empty()
            || (proposal_a.proposal.trim().is_empty() && proposal_a.content.trim().is_empty())
            || (proposal_b.proposal.trim().is_empty() && proposal_b.content.trim().is_empty())
        {
            return Err("architecture review packet requires both proposal contents".to_string());
        }
        if proposal_a.evidence_id == proposal_b.evidence_id {
            return Err("architecture review packet requires distinct proposals".to_string());
        }
        let canonical = serde_json::to_vec(&(
            project_id.clone(),
            project_revision,
            &proposal_a,
            &Some(proposal_b.clone()),
        ))
        .map_err(|error| format!("serialize architecture review packet: {error}"))?;
        let mut hasher = Sha256::new();
        hasher.update(canonical);
        let packet_hash = format!("sha256:{:x}", hasher.finalize());
        Ok(Self {
            project_id,
            project_revision,
            proposal_a,
            proposal_b: Some(proposal_b),
            packet_hash,
        })
    }

    pub fn resolve_established(
        project_id: String,
        project_revision: u64,
        proposal_a: ArchitectureProposalInput,
    ) -> Result<Self, String> {
        if proposal_a.evidence_id.trim().is_empty()
            || (proposal_a.proposal.trim().is_empty() && proposal_a.content.trim().is_empty())
        {
            return Err("established architecture packet requires proposal content".to_string());
        }
        let canonical = serde_json::to_vec(&(
            project_id.clone(),
            project_revision,
            &proposal_a,
            Option::<ArchitectureProposalInput>::None,
        ))
        .map_err(|error| format!("serialize established architecture packet: {error}"))?;
        let mut hasher = Sha256::new();
        hasher.update(canonical);
        let packet_hash = format!("sha256:{:x}", hasher.finalize());
        Ok(Self {
            project_id,
            project_revision,
            proposal_a,
            proposal_b: None,
            packet_hash,
        })
    }

    pub fn manifest(&self, role: &str) -> Result<ResolvedInputManifest, String> {
        self.manifest_at_revision(role, self.project_revision)
    }

    pub fn manifest_at_revision(
        &self,
        role: &str,
        project_revision: u64,
    ) -> Result<ResolvedInputManifest, String> {
        let resolved_content = serde_json::to_string(self)
            .map_err(|error| format!("serialize resolved architecture packet: {error}"))?;
        let mut required_inputs = vec![self.proposal_a.evidence_id.clone()];
        if let Some(proposal) = &self.proposal_b {
            required_inputs.push(proposal.evidence_id.clone());
        }
        Ok(ResolvedInputManifest {
            project_id: self.project_id.clone(),
            project_revision,
            role: role.to_string(),
            required_inputs,
            resolved_content,
            packet_hash: Some(self.packet_hash.clone()),
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureSelection {
    A,
    B,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureCompetitionMode {
    CompetingProposals,
    EstablishedPattern,
}

impl Default for ArchitectureCompetitionMode {
    fn default() -> Self {
        Self::CompetingProposals
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentDisposition {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentExecutorKind {
    DeterministicCommand,
    ToolProbe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExperimentOperation {
    CargoCheckLocked,
    FrontendBuild,
    GitHubRepositoryMetadata {
        url: String,
    },
    FileContains {
        relative_path: String,
        needle: String,
    },
}

impl ExperimentOperation {
    pub const fn executor_kind(&self) -> ExperimentExecutorKind {
        match self {
            Self::CargoCheckLocked | Self::FrontendBuild => {
                ExperimentExecutorKind::DeterministicCommand
            }
            Self::GitHubRepositoryMetadata { .. } | Self::FileContains { .. } => {
                ExperimentExecutorKind::ToolProbe
            }
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::CargoCheckLocked => "cargo check --locked".to_string(),
            Self::FrontendBuild => "npm run build".to_string(),
            Self::GitHubRepositoryMetadata { url } => {
                format!("GitHub repository metadata probe: {url}")
            }
            Self::FileContains {
                relative_path,
                needle,
            } => format!("bounded file probe: {relative_path} contains {needle:?}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperimentContract {
    pub experiment_id: String,
    pub synthesis_identity: String,
    pub assumption: String,
    pub executor_kind: ExperimentExecutorKind,
    pub operation: ExperimentOperation,
    pub expected_observation: String,
    pub pass_condition: String,
    pub fail_condition: String,
    pub inconclusive_condition: String,
    pub environment: String,
    pub timeout_seconds: u64,
    pub allowed_effects: Vec<String>,
    pub protected_paths: Vec<String>,
}

impl ExperimentContract {
    pub fn validate(&self) -> Result<(), String> {
        for (value, field) in [
            (&self.experiment_id, "experiment id"),
            (&self.synthesis_identity, "synthesis identity"),
            (&self.assumption, "assumption"),
            (&self.expected_observation, "expected observation"),
            (&self.pass_condition, "PASS condition"),
            (&self.fail_condition, "FAIL condition"),
            (&self.inconclusive_condition, "INCONCLUSIVE condition"),
            (&self.environment, "environment"),
        ] {
            if value.trim().is_empty() {
                return Err(format!("experiment contract requires {field}"));
            }
        }
        if self.executor_kind != self.operation.executor_kind() {
            return Err("experiment executor does not match its typed operation".to_string());
        }
        if self.timeout_seconds == 0
            || self.timeout_seconds > 900
            || self.allowed_effects.is_empty()
            || self.protected_paths.is_empty()
        {
            return Err(
                "experiment contract requires bounded timeout, effects, and protected paths"
                    .to_string(),
            );
        }
        match &self.operation {
            ExperimentOperation::GitHubRepositoryMetadata { url } => {
                let parsed = reqwest::Url::parse(url)
                    .map_err(|_| "GitHub metadata experiment URL is invalid".to_string())?;
                if parsed.scheme() != "https"
                    || parsed.host_str() != Some("api.github.com")
                    || !parsed.path().starts_with("/repos/")
                    || parsed.query().is_some()
                    || parsed.fragment().is_some()
                {
                    return Err(
                        "GitHub metadata experiment must use a canonical api.github.com /repos/ URL"
                            .to_string(),
                    );
                }
            }
            ExperimentOperation::FileContains {
                relative_path,
                needle,
            } => {
                if relative_path.trim().is_empty()
                    || relative_path.starts_with('/')
                    || relative_path.split('/').any(|part| part == "..")
                    || needle.is_empty()
                    || needle.len() > 4 * 1024
                {
                    return Err("bounded file experiment is invalid".to_string());
                }
            }
            ExperimentOperation::CargoCheckLocked | ExperimentOperation::FrontendBuild => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureSynthesis {
    pub packet_hash: String,
    pub selection: ArchitectureSelection,
    pub reviewer_dispositions: BTreeMap<String, String>,
    pub risky_assumptions: Vec<String>,
    pub experiment_needed: bool,
    #[serde(default)]
    pub experiment_contract: Option<ExperimentContract>,
    #[serde(default)]
    pub no_experiment_reason: Option<String>,
    pub reuse_decisions: Vec<ReuseProof>,
    pub owner_tradeoff: Option<String>,
}

impl ArchitectureSynthesis {
    pub fn validate_for(&self, packet: &ArchitectureReviewPacket) -> Result<(), String> {
        if self.packet_hash != packet.packet_hash {
            return Err("architecture synthesis is bound to a stale packet".to_string());
        }
        if self.reuse_decisions.is_empty() {
            return Err(
                "architecture synthesis requires project-specific reuse decisions".to_string(),
            );
        }
        if self.experiment_needed {
            self.experiment_contract
                .as_ref()
                .ok_or_else(|| {
                    "experiment-needed synthesis requires an ExperimentContract".to_string()
                })?
                .validate()?;
        } else if self
            .no_experiment_reason
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            return Err("synthesis must record why no experiment is required".to_string());
        }
        for decision in &self.reuse_decisions {
            decision.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReuseProof {
    pub capability: String,
    pub classification: String,
    pub candidate: String,
    pub alternatives: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub rationale: String,
}

impl ReuseProof {
    pub fn validate(&self) -> Result<(), String> {
        if self.capability.trim().is_empty()
            || self.candidate.trim().is_empty()
            || self.rationale.trim().is_empty()
        {
            return Err("reuse proof requires capability, candidate, and rationale".to_string());
        }
        if self.classification.eq_ignore_ascii_case("build")
            && (self.alternatives.is_empty() || self.evidence_ids.is_empty())
        {
            return Err("BUILD reuse proof requires alternatives and evidence".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClass {
    ExclusiveSessionRuntime,
    HeavyLsp,
    LightSemantic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceClaim {
    pub work_order_id: String,
    pub execution_epoch: u64,
    pub class: ResourceClass,
}

#[derive(Clone, Default)]
pub struct ResourceScheduler {
    active: Arc<Mutex<Option<ResourceClaim>>>,
}

impl ResourceScheduler {
    pub fn try_claim(
        &self,
        work_order_id: impl Into<String>,
        execution_epoch: u64,
        class: ResourceClass,
    ) -> Result<ResourceClaim, String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "resource scheduler lock poisoned".to_string())?;
        if let Some(current) = active.as_ref() {
            return Err(format!(
                "resource {:?} is held by {} at epoch {}",
                current.class, current.work_order_id, current.execution_epoch
            ));
        }
        let claim = ResourceClaim {
            work_order_id: work_order_id.into(),
            execution_epoch,
            class,
        };
        *active = Some(claim.clone());
        Ok(claim)
    }

    pub fn release(&self, claim: &ResourceClaim) -> Result<(), String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "resource scheduler lock poisoned".to_string())?;
        if active.as_ref() == Some(claim) {
            *active = None;
            Ok(())
        } else {
            Err("stale resource claim cannot release the active holder".to_string())
        }
    }

    pub fn is_held(&self) -> Result<bool, String> {
        Ok(self
            .active
            .lock()
            .map_err(|_| "resource scheduler lock poisoned".to_string())?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_have_explicit_omission_reasons() {
        assert_eq!(
            select_route("Add an existing feature"),
            ProductRoute::ExistingFeature
        );
        let plan = route_plan(ProductRoute::ExistingFeature);
        assert!(!plan.omitted_stages.is_empty());
        assert!(!plan.omitted_stages[0].reason.is_empty());
        assert!(
            route_plan(ProductRoute::Incident)
                .stages
                .contains(&PipelineStage::ReproduceDiagnose)
        );
    }

    #[test]
    fn route_classifier_does_not_treat_greenfield_fix_language_as_existing() {
        assert_eq!(
            select_route("Build an AI product to fix invoice reconciliation"),
            ProductRoute::NewProduct
        );
        assert_eq!(
            select_route("Fix the crash in this existing app"),
            ProductRoute::Incident
        );
        assert_eq!(
            select_route("Add CSV export to this existing repo"),
            ProductRoute::ExistingFeature
        );
        assert_eq!(
            select_route("Fix the typo in this existing repo"),
            ProductRoute::ExistingFeature
        );
        assert_eq!(
            select_route("Fix bug in this existing repo when export is empty"),
            ProductRoute::Incident
        );
        assert_eq!(
            select_route("Build a new product that helps teams fix invoice errors"),
            ProductRoute::NewProduct
        );
        assert_eq!(
            select_route("I want dark mode in my current app"),
            ProductRoute::ExistingFeature
        );
        assert_eq!(
            select_route("Build a new product that integrates with an existing app"),
            ProductRoute::NewProduct
        );
    }

    #[test]
    fn repository_context_routes_terse_change_requests_without_market_discovery() {
        assert_eq!(
            select_route_with_context("Add CSV export", true),
            ProductRoute::ExistingFeature
        );
        assert_eq!(
            select_route_with_context("Add CSV export", false),
            ProductRoute::NewProduct
        );
        assert_eq!(
            select_route_with_context("Fix the crash", true),
            ProductRoute::Incident
        );
    }

    #[test]
    fn explicit_greenfield_intent_is_not_overridden_by_a_scaffold_repository() {
        assert_eq!(
            select_route_with_context(
                "Build a new desktop product for evidence review from scratch",
                true
            ),
            ProductRoute::NewProduct
        );
        assert_eq!(
            select_route_with_context("Add CSV export", true),
            ProductRoute::ExistingFeature
        );
        assert_eq!(
            select_route_with_context("Fix the crash", true),
            ProductRoute::Incident
        );
    }

    #[test]
    fn greenfield_incident_domain_language_does_not_become_incident_route() {
        assert_eq!(
            select_route_with_context(
                "Build a new incident management app from scratch for small IT teams",
                true,
            ),
            ProductRoute::NewProduct
        );
        assert_eq!(
            select_route_with_context(
                "Fix the incident in this existing app after the latest release",
                true,
            ),
            ProductRoute::Incident
        );
    }

    #[test]
    fn architecture_competition_is_conditional() {
        assert_eq!(
            architecture_planning_mode(
                ProductRoute::ExistingFeature,
                "Add CSV export to this existing repo using the existing utility"
            ),
            ArchitecturePlanningMode::EstablishedPattern
        );
        assert_eq!(
            architecture_planning_mode(ProductRoute::ExistingFeature, "add a new billing model"),
            ArchitecturePlanningMode::CompetingProposals
        );
    }

    #[test]
    fn owner_can_reject_semantic_stop_or_pivot_recommendation() {
        assert_eq!(
            owner_decision_for_option(None, "continue_evaluation"),
            Ok(OwnerDecisionKind::ContinueEvaluation)
        );
    }

    #[test]
    fn validation_experiment_cannot_map_to_narrow_build() {
        assert_eq!(
            owner_decision_for_option(
                Some(DecisionOutcome::ValidationExperiment),
                "authorize_validation_experiment"
            ),
            Ok(OwnerDecisionKind::AuthorizeValidationExperiment)
        );
        assert!(
            owner_decision_for_option(Some(DecisionOutcome::ValidationExperiment), "narrow_build")
                .is_err()
        );
        assert!(
            owner_decision_for_option(Some(DecisionOutcome::NarrowBuild), "authorize_build")
                .is_ok()
        );
    }

    #[test]
    fn exclusive_resource_collision_is_rejected_and_release_is_exact() {
        let scheduler = ResourceScheduler::default();
        let first = scheduler
            .try_claim("a", 1, ResourceClass::ExclusiveSessionRuntime)
            .expect("first claim");
        assert!(
            scheduler
                .try_claim("b", 1, ResourceClass::ExclusiveSessionRuntime)
                .is_err()
        );
        assert!(
            scheduler
                .release(&ResourceClaim {
                    work_order_id: "b".to_string(),
                    execution_epoch: 1,
                    class: ResourceClass::ExclusiveSessionRuntime
                })
                .is_err()
        );
        scheduler.release(&first).expect("owner releases claim");
        assert!(!scheduler.is_held().expect("scheduler state"));
    }

    #[test]
    fn reviewer_manifest_requires_both_proposals() {
        let packet = ArchitectureReviewPacket::resolve(
            "project".to_string(),
            1,
            ArchitectureProposalInput {
                evidence_id: "a".to_string(),
                proposal: "proposal A".to_string(),
                assumptions: Vec::new(),
                reuse_choices: Vec::new(),
                interfaces: Vec::new(),
                risks: Vec::new(),
                content: "proposal A".to_string(),
            },
            ArchitectureProposalInput {
                evidence_id: "b".to_string(),
                proposal: "proposal B".to_string(),
                assumptions: Vec::new(),
                reuse_choices: Vec::new(),
                interfaces: Vec::new(),
                risks: Vec::new(),
                content: "proposal B".to_string(),
            },
        )
        .expect("complete packet");
        let manifest = packet.manifest("reviewer").expect("packet manifest");
        assert!(manifest.resolved_content.contains("proposal A"));
        assert!(manifest.resolved_content.contains("proposal B"));
        assert!(
            ArchitectureReviewPacket::resolve(
                "project".to_string(),
                1,
                ArchitectureProposalInput {
                    evidence_id: "a".to_string(),
                    proposal: "proposal A".to_string(),
                    assumptions: Vec::new(),
                    reuse_choices: Vec::new(),
                    interfaces: Vec::new(),
                    risks: Vec::new(),
                    content: "proposal A".to_string()
                },
                ArchitectureProposalInput {
                    evidence_id: "b".to_string(),
                    proposal: String::new(),
                    assumptions: Vec::new(),
                    reuse_choices: Vec::new(),
                    interfaces: Vec::new(),
                    risks: Vec::new(),
                    content: String::new()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn packet_hash_covers_typed_proposal_details() {
        let base = |assumptions: Vec<String>| ArchitectureProposalInput {
            evidence_id: "a".to_string(),
            proposal: "use the existing boundary".to_string(),
            assumptions,
            reuse_choices: vec!["reuse Delivery".to_string()],
            interfaces: vec!["typed handoff".to_string()],
            risks: vec!["stale candidate".to_string()],
            content: "use the existing boundary".to_string(),
        };
        let packet_a = ArchitectureReviewPacket::resolve(
            "project".to_string(),
            1,
            base(vec!["constraint one".to_string()]),
            ArchitectureProposalInput {
                evidence_id: "b".to_string(),
                proposal: "build a bounded adapter".to_string(),
                assumptions: vec!["adapter is isolated".to_string()],
                reuse_choices: vec!["wrap source".to_string()],
                interfaces: vec!["adapter API".to_string()],
                risks: vec!["source changes".to_string()],
                content: "build a bounded adapter".to_string(),
            },
        )
        .expect("packet");
        let packet_b = ArchitectureReviewPacket::resolve(
            "project".to_string(),
            1,
            base(vec!["different constraint".to_string()]),
            packet_a.proposal_b.clone().expect("competing packet B"),
        )
        .expect("packet");
        assert_ne!(packet_a.packet_hash, packet_b.packet_hash);
    }

    #[test]
    fn experiment_contract_requires_bounded_protocol() {
        let mut contract = ExperimentContract {
            experiment_id: "experiment-1".to_string(),
            synthesis_identity: "synthesis-1".to_string(),
            assumption: "the adapter compiles".to_string(),
            executor_kind: ExperimentExecutorKind::DeterministicCommand,
            operation: ExperimentOperation::CargoCheckLocked,
            expected_observation: "exit zero".to_string(),
            pass_condition: "exit zero".to_string(),
            fail_condition: "exit non-zero".to_string(),
            inconclusive_condition: "timeout".to_string(),
            environment: "isolated repository worktree".to_string(),
            timeout_seconds: 60,
            allowed_effects: vec!["read-only".to_string()],
            protected_paths: vec![".arena".to_string()],
        };
        assert!(contract.validate().is_ok());
        contract.timeout_seconds = 0;
        assert!(contract.validate().is_err());
        contract.timeout_seconds = 60;
        contract.executor_kind = ExperimentExecutorKind::ToolProbe;
        assert!(contract.validate().is_err());
    }

    #[test]
    fn synthesis_requires_current_packet_and_project_specific_reuse() {
        let packet = ArchitectureReviewPacket::resolve(
            "project".to_string(),
            1,
            ArchitectureProposalInput {
                evidence_id: "a".to_string(),
                proposal: "proposal A".to_string(),
                assumptions: Vec::new(),
                reuse_choices: Vec::new(),
                interfaces: Vec::new(),
                risks: Vec::new(),
                content: "proposal A".to_string(),
            },
            ArchitectureProposalInput {
                evidence_id: "b".to_string(),
                proposal: "proposal B".to_string(),
                assumptions: Vec::new(),
                reuse_choices: Vec::new(),
                interfaces: Vec::new(),
                risks: Vec::new(),
                content: "proposal B".to_string(),
            },
        )
        .expect("packet");
        let mut synthesis = ArchitectureSynthesis {
            packet_hash: "stale".to_string(),
            selection: ArchitectureSelection::A,
            reviewer_dispositions: BTreeMap::new(),
            risky_assumptions: Vec::new(),
            experiment_needed: false,
            experiment_contract: None,
            no_experiment_reason: Some(
                "no bounded risky assumption requires execution".to_string(),
            ),
            reuse_decisions: Vec::new(),
            owner_tradeoff: None,
        };
        assert!(synthesis.validate_for(&packet).is_err());
        synthesis.packet_hash = packet.packet_hash.clone();
        assert!(synthesis.validate_for(&packet).is_err());
    }

    #[test]
    fn generic_reuse_cannot_satisfy_project_proof() {
        assert!(
            ReuseProof {
                capability: "bounded implementation".to_string(),
                classification: "reuse".to_string(),
                candidate: String::new(),
                alternatives: Vec::new(),
                evidence_ids: Vec::new(),
                rationale: String::new(),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn every_non_pass_gate_gets_a_bounded_remediation() {
        let remediation = route_gate_remediation(GateId::Architecture, GateStatus::Blocked, 0);
        assert_ne!(remediation.outcome, GateRemediationOutcome::Satisfied);
        assert!(remediation.max_attempts > remediation.attempt);
    }

    #[test]
    fn scenario_routes_preserve_controller_intent() {
        let weak_product = route_plan(ProductRoute::NewProduct);
        assert!(weak_product.stages.contains(&PipelineStage::Decide));
        assert_eq!(
            owner_decision_for_option(None, "stop"),
            Ok(OwnerDecisionKind::StopRun)
        );

        let existing_feature = route_plan(ProductRoute::ExistingFeature);
        assert!(!existing_feature.stages.contains(&PipelineStage::Discover));

        let incident = route_plan(ProductRoute::Incident);
        assert_eq!(incident.stages[0], PipelineStage::ReproduceDiagnose);
    }

    #[test]
    fn scheduler_claim_and_session_runtime_admission_are_serial() {
        let scheduler = ResourceScheduler::default();
        let runtime = Arc::new(crate::session_runtime::SessionRuntime::new());
        let first = scheduler
            .try_claim("work-a", 7, ResourceClass::ExclusiveSessionRuntime)
            .expect("first role claim");
        let permit = runtime
            .try_acquire_start("run-a".to_string())
            .expect("first session admission");
        assert!(
            scheduler
                .try_claim("work-b", 7, ResourceClass::ExclusiveSessionRuntime)
                .is_err()
        );
        assert!(runtime.try_acquire_start("run-b".to_string()).is_err());
        drop(permit);
        scheduler.release(&first).expect("release first role claim");
        assert!(
            scheduler
                .try_claim("work-b", 7, ResourceClass::ExclusiveSessionRuntime)
                .is_ok()
        );
    }
}
