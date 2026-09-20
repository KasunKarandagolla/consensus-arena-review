//! Frozen founder-facing functional state contract for the post-M09 GUI.
//!
//! This DTO deliberately hides raw model transcripts, MCP payloads, browser
//! selectors, and workflow-internal storage details. It is a pure projection
//! of durable Arena authority and runtime evidence.

use crate::consultation_broker::{
    ConsultationTransactionState, ConsultationWorkOrder,
};
use crate::delivery::{DeliveryPhase, DeliveryState};
use crate::evidence_gates::EvidenceVerification;
use crate::pipeline_contract::{OwnerDecisionKind, ProductRoute};
use crate::product_os::{ProductAuthorityRecords, ProductWorkOrder, ProductWorkOrderStatus};
use crate::product_os_coordinator::{
    CoordinatorPhase, CoordinatorStatus, ProductCoordinatorRun,
};
use crate::quality_workflows::ToolUseReceipt;
use serde::{Deserialize, Serialize};

pub const FUNCTIONAL_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerPhase {
    Discover,
    Decide,
    Deliver,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionalLifecycle {
    Ready,
    Running,
    WaitingForOwner,
    Blocked,
    Reconciling,
    Cancelling,
    Cancelled,
    Completed,
    Stopped,
    Pivoted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionalStage {
    Intake,
    IntentNormalization,
    ContextInspection,
    ReproduceDiagnose,
    EvidencePlanning,
    Researching,
    VerifyingClaims,
    ProductChallenge,
    ArchitecturePlanning,
    ArchitectureProposals,
    ArchitectureReview,
    ArchitectureSynthesis,
    ExperimentPlanning,
    ExperimentRunning,
    BuildReadiness,
    FreezeAndIsolate,
    Implementing,
    Integrating,
    ReviewingCandidate,
    Verifying,
    Repairing,
    ReleaseReadiness,
    WaitingForApply,
    Applying,
    Publishing,
    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceState {
    pub total: usize,
    pub independently_verified: usize,
    pub unresolved: usize,
    pub contradicted: usize,
    pub consultation_advice: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureState {
    pub selected: Option<String>,
    pub rationale: Option<String>,
    pub packet_hash: Option<String>,
    pub unresolved_blockers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentState {
    pub required: bool,
    pub experiment_id: Option<String>,
    pub state: String,
    pub latest_evidence_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerDecisionState {
    pub pending: bool,
    pub kind: Option<OwnerDecisionKind>,
    pub options: Vec<String>,
    pub question_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationState {
    pub outcome: Option<String>,
    pub reason: Option<String>,
    pub action: Option<String>,
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsultationActivity {
    pub request_count: usize,
    pub completed: usize,
    pub unknown_outcome: usize,
    pub blocked_or_recovery: usize,
    pub active_request_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolActivity {
    pub receipt_count: usize,
    pub observed_tool_events: u64,
    pub profiles: Vec<String>,
    pub unavailable_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryStateView {
    pub session_id: Option<String>,
    pub phase: Option<DeliveryPhase>,
    pub candidate_commit: Option<String>,
    pub attempt: u32,
    pub waiting_question: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewState {
    pub semantic_receipts: usize,
    pub browser_qa_records: usize,
    pub context_manifest_records: usize,
    pub incomplete_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStateView {
    pub verdict: Option<String>,
    pub receipt_id: Option<String>,
    pub candidate_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyState {
    pub available: bool,
    pub applied: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductFunctionalState {
    pub contract_version: u32,
    pub project_id: String,
    pub run_id: String,
    pub route: ProductRoute,
    pub owner_phase: OwnerPhase,
    pub internal_stage: FunctionalStage,
    pub lifecycle: FunctionalLifecycle,
    pub active_work_wave: Vec<String>,
    pub evidence: EvidenceState,
    pub architecture: ArchitectureState,
    pub experiment: ExperimentState,
    pub owner_decision: OwnerDecisionState,
    pub remediation: RemediationState,
    pub consultation: ConsultationActivity,
    pub tools: ToolActivity,
    pub delivery: DeliveryStateView,
    pub review: ReviewState,
    pub verification: VerificationStateView,
    pub apply: ApplyState,
    pub degraded_notices: Vec<String>,
}

fn lifecycle(status: &CoordinatorStatus) -> FunctionalLifecycle {
    match status {
        CoordinatorStatus::Admitted => FunctionalLifecycle::Ready,
        CoordinatorStatus::Running => FunctionalLifecycle::Running,
        CoordinatorStatus::WaitingForOwner => FunctionalLifecycle::WaitingForOwner,
        CoordinatorStatus::Blocked | CoordinatorStatus::Failed => FunctionalLifecycle::Blocked,
        CoordinatorStatus::Reconciling => FunctionalLifecycle::Reconciling,
        CoordinatorStatus::Cancelling => FunctionalLifecycle::Cancelling,
        CoordinatorStatus::Cancelled => FunctionalLifecycle::Cancelled,
        CoordinatorStatus::Completed => FunctionalLifecycle::Completed,
        CoordinatorStatus::Stopped => FunctionalLifecycle::Stopped,
        CoordinatorStatus::Pivoted => FunctionalLifecycle::Pivoted,
    }
}

fn stage(
    run: &ProductCoordinatorRun,
    delivery: Option<&DeliveryState>,
) -> FunctionalStage {
    if let Some(delivery) = delivery {
        return match delivery.phase {
            DeliveryPhase::Preparing | DeliveryPhase::AuthoringAcceptance => {
                FunctionalStage::FreezeAndIsolate
            }
            DeliveryPhase::WaitingForUser => FunctionalStage::FreezeAndIsolate,
            DeliveryPhase::AcceptanceReady => FunctionalStage::Implementing,
            DeliveryPhase::Implementing => FunctionalStage::Implementing,
            DeliveryPhase::Verifying => {
                if delivery.semantic_reviews.is_empty() {
                    FunctionalStage::Verifying
                } else {
                    FunctionalStage::ReviewingCandidate
                }
            }
            DeliveryPhase::Repairing => FunctionalStage::Repairing,
            DeliveryPhase::Verified => FunctionalStage::WaitingForApply,
            DeliveryPhase::Applied => FunctionalStage::Applying,
            DeliveryPhase::Cancelled | DeliveryPhase::Failed => FunctionalStage::Terminal,
        };
    }
    match run.phase {
        CoordinatorPhase::Research => {
            if run.research_work_order_ids.is_empty() {
                FunctionalStage::EvidencePlanning
            } else if run.verifier_work_order_ids.len() < run.research_work_order_ids.len() {
                FunctionalStage::Researching
            } else {
                FunctionalStage::VerifyingClaims
            }
        }
        CoordinatorPhase::ReproduceDiagnose => FunctionalStage::ReproduceDiagnose,
        CoordinatorPhase::ProductReview => FunctionalStage::ProductChallenge,
        CoordinatorPhase::Architecture => {
            if run.architecture_work_order_ids.is_empty() {
                FunctionalStage::ArchitecturePlanning
            } else if run.architecture_evidence_ids.len() < 2 && run.reuse_work_order_id.is_none() {
                FunctionalStage::ArchitectureProposals
            } else if run.feasibility_work_order_id.is_some() {
                FunctionalStage::ExperimentRunning
            } else if run.architecture_evidence_ids.len() >= 1
                && run.build_package_id.is_none()
            {
                FunctionalStage::ArchitectureReview
            } else {
                FunctionalStage::ArchitectureSynthesis
            }
        }
        CoordinatorPhase::Package => FunctionalStage::BuildReadiness,
        CoordinatorPhase::Delivery => FunctionalStage::FreezeAndIsolate,
        CoordinatorPhase::Terminal => {
            if run.delivery_session_id.is_some() {
                FunctionalStage::ReleaseReadiness
            } else {
                FunctionalStage::Terminal
            }
        }
    }
}

fn owner_phase(stage: FunctionalStage) -> OwnerPhase {
    match stage {
        FunctionalStage::Intake
        | FunctionalStage::IntentNormalization
        | FunctionalStage::ContextInspection
        | FunctionalStage::ReproduceDiagnose
        | FunctionalStage::EvidencePlanning
        | FunctionalStage::Researching
        | FunctionalStage::VerifyingClaims => OwnerPhase::Discover,
        FunctionalStage::ProductChallenge
        | FunctionalStage::ArchitecturePlanning
        | FunctionalStage::ArchitectureProposals
        | FunctionalStage::ArchitectureReview
        | FunctionalStage::ArchitectureSynthesis
        | FunctionalStage::ExperimentPlanning
        | FunctionalStage::ExperimentRunning
        | FunctionalStage::BuildReadiness => OwnerPhase::Decide,
        FunctionalStage::FreezeAndIsolate
        | FunctionalStage::Implementing
        | FunctionalStage::Integrating
        | FunctionalStage::ReviewingCandidate
        | FunctionalStage::Verifying
        | FunctionalStage::Repairing => OwnerPhase::Deliver,
        FunctionalStage::ReleaseReadiness
        | FunctionalStage::WaitingForApply
        | FunctionalStage::Applying
        | FunctionalStage::Publishing
        | FunctionalStage::Terminal => OwnerPhase::Release,
    }
}

fn decision_options(kind: Option<OwnerDecisionKind>) -> Vec<String> {
    match kind {
        Some(OwnerDecisionKind::AuthorizeValidationExperiment) => vec![
            "authorize_validation_experiment".to_string(),
            "stop".to_string(),
            "pivot".to_string(),
        ],
        Some(OwnerDecisionKind::AuthorizeNarrowBuild) => vec![
            "authorize_narrow_build".to_string(),
            "stop".to_string(),
            "pivot".to_string(),
        ],
        Some(OwnerDecisionKind::AuthorizeBuild) => vec![
            "authorize_build".to_string(),
            "stop".to_string(),
            "pivot".to_string(),
        ],
        Some(OwnerDecisionKind::ApproveApply) => {
            vec!["approve_apply".to_string(), "stop".to_string()]
        }
        Some(OwnerDecisionKind::ApproveRelease) => {
            vec!["approve_release".to_string(), "stop".to_string()]
        }
        Some(OwnerDecisionKind::StopRun) | Some(OwnerDecisionKind::PivotRun) | None => Vec::new(),
    }
}

pub fn build_functional_state(
    run: &ProductCoordinatorRun,
    records: &ProductAuthorityRecords,
    work_orders: &[ProductWorkOrder],
    delivery: Option<&DeliveryState>,
    consultations: &[ConsultationWorkOrder],
    tool_receipts: &[ToolUseReceipt],
) -> ProductFunctionalState {
    let internal_stage = stage(run, delivery);
    let evidence = EvidenceState {
        total: records.evidence.len(),
        independently_verified: records
            .evidence
            .iter()
            .filter(|item| {
                item.verification == Some(EvidenceVerification::IndependentlyVerified)
            })
            .count(),
        unresolved: records
            .evidence
            .iter()
            .filter(|item| item.verification == Some(EvidenceVerification::Unresolved))
            .count(),
        contradicted: records
            .evidence
            .iter()
            .filter(|item| item.verification == Some(EvidenceVerification::Contradicted))
            .count(),
        consultation_advice: records
            .evidence
            .iter()
            .filter(|item| item.kind == Some(crate::evidence_gates::EvidenceKind::ConsultationAdvice))
            .count(),
    };
    let synthesis = records.architecture.synthesis.as_ref();
    let architecture = ArchitectureState {
        selected: synthesis.map(|value| format!("{:?}", value.selection)),
        rationale: synthesis.and_then(|value| {
            value
                .owner_tradeoff
                .clone()
                .or_else(|| value.no_experiment_reason.clone())
        }),
        packet_hash: synthesis.map(|value| value.packet_hash.clone()),
        unresolved_blockers: records.architecture.unresolved_high_blocker_evidence_ids.len(),
    };
    let latest_experiment_evidence = records
        .architecture
        .risk_experiment_evidence_ids
        .last()
        .cloned();
    let experiment = ExperimentState {
        required: synthesis.is_some_and(|value| value.experiment_needed),
        experiment_id: run
            .pending_experiment
            .as_ref()
            .map(|value| value.experiment_id.clone())
            .or_else(|| {
                synthesis
                    .and_then(|value| value.experiment_contract.as_ref())
                    .map(|value| value.experiment_id.clone())
            }),
        state: if run.pending_experiment.is_some() {
            "pending_owner_or_execution".to_string()
        } else if run.feasibility_work_order_id.is_some() {
            "completed_or_admitted".to_string()
        } else {
            "not_required".to_string()
        },
        latest_evidence_id: latest_experiment_evidence,
    };
    let owner_decision = OwnerDecisionState {
        pending: run.pending_owner_decision.is_some(),
        kind: run.pending_owner_decision,
        options: decision_options(run.pending_owner_decision),
        question_id: run.owner_question_id.clone(),
    };
    let remediation = RemediationState {
        outcome: run
            .last_remediation
            .as_ref()
            .map(|value| format!("{:?}", value.outcome)),
        reason: run.last_remediation.as_ref().map(|value| value.reason.clone()),
        action: run.last_remediation.as_ref().map(|value| value.action.clone()),
        blocker: run.error.clone().or_else(|| run.remediation_question.clone()),
    };
    let consultation = ConsultationActivity {
        request_count: consultations.len(),
        completed: consultations
            .iter()
            .filter(|order| order.state == ConsultationTransactionState::Complete)
            .count(),
        unknown_outcome: consultations
            .iter()
            .filter(|order| order.state == ConsultationTransactionState::UnknownOutcome)
            .count(),
        blocked_or_recovery: consultations
            .iter()
            .filter(|order| {
                matches!(
                    order.state,
                    ConsultationTransactionState::OwnerRecovery
                        | ConsultationTransactionState::Cancelled
                ) || order.failure.is_some()
            })
            .count(),
        active_request_ids: consultations
            .iter()
            .filter(|order| !order.state.is_terminal())
            .map(|order| order.request_id.clone())
            .collect(),
    };
    let mut profiles = tool_receipts
        .iter()
        .map(|receipt| receipt.profile.clone())
        .collect::<Vec<_>>();
    profiles.sort();
    profiles.dedup();
    let tools = ToolActivity {
        receipt_count: tool_receipts.len(),
        observed_tool_events: tool_receipts
            .iter()
            .map(|receipt| u64::from(receipt.tool_count))
            .sum(),
        unavailable_count: tool_receipts
            .iter()
            .filter(|receipt| receipt.status != "complete")
            .count(),
        profiles,
    };
    let delivery_view = DeliveryStateView {
        session_id: delivery.map(|value| value.session_id.clone()),
        phase: delivery.map(|value| value.phase.clone()),
        candidate_commit: delivery.and_then(|value| value.candidate_commit.clone()),
        attempt: delivery.map_or(0, |value| value.attempt),
        waiting_question: delivery
            .and_then(|value| value.pending_question.as_ref())
            .map(|value| value.text.clone()),
    };
    let review = ReviewState {
        semantic_receipts: delivery.map_or(0, |value| value.semantic_reviews.len()),
        browser_qa_records: delivery.map_or(0, |value| {
            value
                .evidence
                .iter()
                .filter(|item| item.kind == "browser_qa_advisory")
                .count()
        }),
        context_manifest_records: delivery.map_or(0, |value| {
            value
                .evidence
                .iter()
                .filter(|item| item.kind == "candidate_review_context")
                .count()
        }),
        incomplete_notice: delivery.and_then(|value| {
            value
                .last_worker_summary
                .as_ref()
                .filter(|summary| summary.contains("review") && summary.contains("incomplete"))
                .cloned()
        }),
    };
    let verification = VerificationStateView {
        verdict: delivery
            .and_then(|value| value.last_verification.as_ref())
            .map(|value| value.verdict.clone()),
        receipt_id: delivery
            .and_then(|value| value.last_verification.as_ref())
            .map(|value| value.verification_id.clone()),
        candidate_commit: delivery.and_then(|value| value.candidate_commit.clone()),
    };
    let (apply_available, applied, apply_reason) = match delivery.map(|value| &value.phase) {
        Some(DeliveryPhase::Verified) => (
            true,
            false,
            "Verified candidate is eligible for explicit owner-authorized Safe Apply".to_string(),
        ),
        Some(DeliveryPhase::Applied) => (
            false,
            true,
            "Safe Apply completed".to_string(),
        ),
        _ => (
            false,
            false,
            "Apply is unavailable until the exact candidate is Verified".to_string(),
        ),
    };
    let mut active_work_wave = work_orders
        .iter()
        .filter(|order| {
            matches!(
                order.status,
                ProductWorkOrderStatus::Admitted
                    | ProductWorkOrderStatus::Running
                    | ProductWorkOrderStatus::ReconciliationRequired
            )
        })
        .map(|order| format!("{:?}:{}", order.role, order.work_order_id))
        .collect::<Vec<_>>();
    active_work_wave.extend(
        consultations
            .iter()
            .filter(|order| !order.state.is_terminal())
            .map(|order| format!("Consultation:{:?}:{}", order.state, order.request_id)),
    );
    let mut degraded_notices = Vec::new();
    if consultations
        .iter()
        .any(|order| order.state == ConsultationTransactionState::UnknownOutcome)
    {
        degraded_notices.push(
            "A consultation has unknown submission outcome; Arena will not automatically resend."
                .to_string(),
        );
    }
    if tools.unavailable_count > 0 {
        degraded_notices.push(format!(
            "{} advisory tool execution(s) were unavailable or incomplete.",
            tools.unavailable_count
        ));
    }
    if run.status == CoordinatorStatus::Failed {
        degraded_notices.push(
            "Internal coordinator failure is exposed as a blocked lifecycle state until recovery."
                .to_string(),
        );
    }
    ProductFunctionalState {
        contract_version: FUNCTIONAL_CONTRACT_VERSION,
        project_id: run.project_id.clone(),
        run_id: run.run_id.clone(),
        route: run.route,
        owner_phase: owner_phase(internal_stage),
        internal_stage,
        lifecycle: lifecycle(&run.status),
        active_work_wave,
        evidence,
        architecture,
        experiment,
        owner_decision,
        remediation,
        consultation,
        tools,
        delivery: delivery_view,
        review,
        verification,
        apply: ApplyState {
            available: apply_available,
            applied,
            reason: apply_reason,
        },
        degraded_notices,
    }
}

pub async fn load_functional_state(
    db: std::sync::Arc<std::sync::Mutex<crate::transcript_store::TranscriptStore>>,
    run_id: String,
) -> Result<ProductFunctionalState, String> {
    crate::db_helpers::run_blocking(move || {
        let store = db.lock().map_err(|_| {
            crate::errors::AgentError::DatabaseError(
                "functional state store lock poisoned".to_string(),
            )
        })?;
        let run = store
            .get_product_coordinator_run(&run_id)?
            .ok_or_else(|| {
                crate::errors::AgentError::DatabaseError(
                    "Product OS run is unknown".to_string(),
                )
            })?;
        let raw_records = store
            .get_product_authority(&run.project_id)?
            .ok_or_else(|| {
                crate::errors::AgentError::DatabaseError(
                    "Product OS authority is missing".to_string(),
                )
            })?;
        let records: ProductAuthorityRecords =
            serde_json::from_str(&raw_records).map_err(|error| {
                crate::errors::AgentError::DatabaseError(format!(
                    "parse Product OS authority for functional state: {error}"
                ))
            })?;
        let work_orders = store.list_product_work_orders(&run.project_id)?;
        let delivery = match run.delivery_session_id.as_deref() {
            Some(session_id) => store
                .get_delivery_state(session_id)?
                .map(|raw| {
                    serde_json::from_str::<DeliveryState>(&raw).map_err(|error| {
                        crate::errors::AgentError::DatabaseError(format!(
                            "parse Delivery state for functional state: {error}"
                        ))
                    })
                })
                .transpose()?,
            None => None,
        };
        let mut consultations = Vec::new();
        for request_id in &run.consultation_request_ids {
            if let Some(order) = store.get_consultation_work_order(request_id)? {
                consultations.push(order);
            }
        }
        let mut tool_receipts = Vec::new();
        for order in &work_orders {
            tool_receipts.extend(store.list_tool_use_receipts(&order.work_order_id)?);
        }
        Ok(build_functional_state(
            &run,
            &records,
            &work_orders,
            delivery.as_ref(),
            &consultations,
            &tool_receipts,
        ))
    })
    .await
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_phase_mapping_keeps_gui_broad_and_internal_stage_precise() {
        assert_eq!(
            owner_phase(FunctionalStage::ArchitectureReview),
            OwnerPhase::Decide
        );
        assert_eq!(
            owner_phase(FunctionalStage::ReviewingCandidate),
            OwnerPhase::Deliver
        );
        assert_eq!(
            owner_phase(FunctionalStage::WaitingForApply),
            OwnerPhase::Release
        );
    }

    #[test]
    fn owner_options_are_typed_not_free_form() {
        assert_eq!(
            decision_options(Some(OwnerDecisionKind::ApproveApply)),
            vec!["approve_apply".to_string(), "stop".to_string()]
        );
        assert!(decision_options(None).is_empty());
    }
}
