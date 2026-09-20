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

pub fn select_route(intent: &str) -> ProductRoute {
    let lower = intent.to_ascii_lowercase();
    if [
        "incident",
        "outage",
        "regression",
        "production bug",
        "service down",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        ProductRoute::Incident
    } else if [
        "existing feature",
        "extend",
        "modify",
        "change",
        "add to",
        "fix",
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
        "stop" => OwnerDecisionKind::StopRun,
        "pivot" => OwnerDecisionKind::PivotRun,
        "approve_apply" => OwnerDecisionKind::ApproveApply,
        "approve_release" => OwnerDecisionKind::ApproveRelease,
        _ => return Err("owner decision option is not valid for the current question".to_string()),
    };
    Ok(decision)
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
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureReviewPacket {
    pub project_id: String,
    pub project_revision: u64,
    pub proposal_a: ArchitectureProposalInput,
    pub proposal_b: ArchitectureProposalInput,
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
            || proposal_a.content.trim().is_empty()
            || proposal_b.content.trim().is_empty()
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
            &proposal_b,
        ))
        .map_err(|error| format!("serialize architecture review packet: {error}"))?;
        let mut hasher = Sha256::new();
        hasher.update(canonical);
        let packet_hash = format!("sha256:{:x}", hasher.finalize());
        Ok(Self {
            project_id,
            project_revision,
            proposal_a,
            proposal_b,
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
        Ok(ResolvedInputManifest {
            project_id: self.project_id.clone(),
            project_revision,
            role: role.to_string(),
            required_inputs: vec![
                self.proposal_a.evidence_id.clone(),
                self.proposal_b.evidence_id.clone(),
            ],
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
pub struct ArchitectureSynthesis {
    pub packet_hash: String,
    pub selection: ArchitectureSelection,
    pub reviewer_dispositions: BTreeMap<String, String>,
    pub risky_assumptions: Vec<String>,
    pub experiment_needed: bool,
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
                content: "proposal A".to_string(),
            },
            ArchitectureProposalInput {
                evidence_id: "b".to_string(),
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
                    content: "proposal A".to_string()
                },
                ArchitectureProposalInput {
                    evidence_id: "b".to_string(),
                    content: String::new()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn synthesis_requires_current_packet_and_project_specific_reuse() {
        let packet = ArchitectureReviewPacket::resolve(
            "project".to_string(),
            1,
            ArchitectureProposalInput {
                evidence_id: "a".to_string(),
                content: "proposal A".to_string(),
            },
            ArchitectureProposalInput {
                evidence_id: "b".to_string(),
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
