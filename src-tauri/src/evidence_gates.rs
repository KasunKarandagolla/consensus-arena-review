use serde::{Deserialize, Serialize};

/// The nine lightweight exit contracts are evaluated inside the existing
/// Discover/Decide/Deliver/Release phases. They are records and predicates,
/// not nine workflow services.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateId {
    Vision,
    ProblemResearch,
    Positioning,
    Ambiguity,
    Reuse,
    Architecture,
    BuildReadiness,
    Implementation,
    Release,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pass,
    Blocked,
    Stale,
    MissingEvidence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Stop,
    Pivot,
    ValidationExperiment,
    NarrowBuild,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProvenance {
    Documented,
    SourceConfirmed,
    RuntimeProven,
    OwnerReported,
    Inferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguitySeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityStatus {
    Open,
    Resolved,
    Mitigated,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceItem {
    pub evidence_id: String,
    pub claim: String,
    pub source_reference: String,
    pub captured_at: String,
    pub summary: String,
    pub provenance: EvidenceProvenance,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmbiguityItem {
    pub ambiguity_id: String,
    pub affected_commitment: String,
    pub severity: AmbiguitySeverity,
    pub status: AmbiguityStatus,
    pub mitigation: Option<String>,
    pub revisit_trigger: Option<String>,
    pub owner_decision_required: bool,
    pub owner_decision_recorded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerRestatement {
    pub intended_outcome: String,
    pub success_condition: String,
    pub invented_behaviors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureProof {
    pub proposal_count: u8,
    pub hard_constraints_checked: bool,
    pub reuse_scan_complete: bool,
    pub risky_assumptions_tested: bool,
    pub red_team_complete: bool,
    pub unresolved_high_technical_blocker: bool,
    pub dissent_preserved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImplementationProof {
    pub candidate_revision: u64,
    pub verified_candidate_revision: u64,
    pub verifier_pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseProof {
    pub candidate_evidence: bool,
    pub package_evidence: bool,
    pub install_evidence: bool,
    pub security_evidence: bool,
    pub qa_evidence: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateInput {
    pub gate_id: GateId,
    pub package_id: String,
    pub package_revision: u64,
    pub current_revision: u64,
    /// Arena-owned fingerprint for the exact intent/decision/evidence package
    /// evaluated by this predicate. Workers and consultation responses cannot
    /// choose or advance it.
    pub authority_fingerprint: String,
    pub current_authority_fingerprint: String,
    pub required_evidence: Vec<String>,
    pub evidence: Vec<EvidenceItem>,
    pub ambiguities: Vec<AmbiguityItem>,
    pub next_irreversible_commitment: Option<String>,
    pub reviewer_restatement: Option<ReviewerRestatement>,
    pub decision_outcome: Option<DecisionOutcome>,
    pub owner_decision_required: bool,
    pub owner_decision_recorded: bool,
    pub reuse_scan_complete: bool,
    pub build_capabilities_have_evidence: bool,
    pub architecture: Option<ArchitectureProof>,
    pub build_ready: bool,
    pub implementation: Option<ImplementationProof>,
    pub release: Option<ReleaseProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateDecision {
    pub gate_id: GateId,
    pub package_id: String,
    pub package_revision: u64,
    pub status: GateStatus,
    pub outcome: Option<DecisionOutcome>,
    pub owner_authority_required: bool,
    pub reason: String,
    pub evidence_refs: Vec<String>,
}

fn decision(input: &GateInput, status: GateStatus, reason: impl Into<String>) -> GateDecision {
    GateDecision {
        gate_id: input.gate_id,
        package_id: input.package_id.clone(),
        package_revision: input.package_revision,
        status,
        outcome: input.decision_outcome,
        owner_authority_required: input.owner_decision_required && !input.owner_decision_recorded,
        reason: reason.into(),
        evidence_refs: input
            .evidence
            .iter()
            .map(|item| item.evidence_id.clone())
            .collect(),
    }
}

fn missing_required_evidence(input: &GateInput) -> Option<String> {
    input
        .required_evidence
        .iter()
        .find(|required| {
            !input
                .evidence
                .iter()
                .any(|item| item.evidence_id == **required)
        })
        .cloned()
}

fn has_open_high_ambiguity(input: &GateInput) -> bool {
    input.ambiguities.iter().any(|item| {
        item.affected_commitment
            == input
                .next_irreversible_commitment
                .as_deref()
                .unwrap_or_default()
            && matches!(
                item.severity,
                AmbiguitySeverity::Critical | AmbiguitySeverity::High
            )
            && matches!(
                item.status,
                AmbiguityStatus::Open | AmbiguityStatus::Deferred
            )
    })
}

fn has_unmitigated_medium_ambiguity(input: &GateInput) -> bool {
    input.ambiguities.iter().any(|item| {
        item.affected_commitment
            == input
                .next_irreversible_commitment
                .as_deref()
                .unwrap_or_default()
            && item.severity == AmbiguitySeverity::Medium
            && matches!(item.status, AmbiguityStatus::Open)
            && (item.mitigation.is_none() || item.revisit_trigger.is_none())
    })
}

/// Evaluate one gate using only the current Arena-owned package. Callers must
/// obtain this package from Arena authority; worker/consultation claims alone
/// are not authorization to construct a passing package.
pub fn evaluate(input: &GateInput) -> GateDecision {
    if input.package_revision != input.current_revision {
        return decision(
            input,
            GateStatus::Stale,
            "evidence package revision is stale",
        );
    }
    if input.authority_fingerprint.is_empty()
        || input.current_authority_fingerprint.is_empty()
        || input.authority_fingerprint != input.current_authority_fingerprint
    {
        return decision(
            input,
            if input.authority_fingerprint.is_empty()
                || input.current_authority_fingerprint.is_empty()
            {
                GateStatus::MissingEvidence
            } else {
                GateStatus::Stale
            },
            "Arena authority/evidence fingerprint is missing or stale",
        );
    }
    if input.evidence.iter().any(|item| !item.current) {
        return decision(
            input,
            GateStatus::Stale,
            "one or more evidence items are stale",
        );
    }
    if let Some(missing) = missing_required_evidence(input) {
        return decision(
            input,
            GateStatus::MissingEvidence,
            format!("required evidence is missing: {missing}"),
        );
    }
    if input.owner_decision_required && !input.owner_decision_recorded {
        return decision(
            input,
            GateStatus::Blocked,
            "owner decision is required before this gate can pass",
        );
    }

    match input.gate_id {
        GateId::Vision => match &input.reviewer_restatement {
            None => decision(
                input,
                GateStatus::MissingEvidence,
                "fresh reviewer restatement is missing",
            ),
            Some(review) if review.intended_outcome.trim().is_empty() => decision(
                input,
                GateStatus::Blocked,
                "reviewer did not restate the intended outcome",
            ),
            Some(review) if review.success_condition.trim().is_empty() => decision(
                input,
                GateStatus::Blocked,
                "reviewer did not restate the success condition",
            ),
            Some(review) if !review.invented_behaviors.is_empty() => decision(
                input,
                GateStatus::Blocked,
                "fresh reviewer invented unsupported product behavior",
            ),
            Some(_) => decision(
                input,
                GateStatus::Pass,
                "fresh reviewer restated the current intent",
            ),
        },
        GateId::ProblemResearch | GateId::Positioning => {
            if input.evidence.is_empty() {
                decision(
                    input,
                    GateStatus::MissingEvidence,
                    "decision-relevant evidence is missing",
                )
            } else if input.decision_outcome.is_none() {
                decision(
                    input,
                    GateStatus::Blocked,
                    "research has no explicit next decision",
                )
            } else {
                decision(
                    input,
                    GateStatus::Pass,
                    "research supports an explicit bounded next decision",
                )
            }
        }
        GateId::Ambiguity => {
            if input
                .next_irreversible_commitment
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            {
                decision(
                    input,
                    GateStatus::Blocked,
                    "next irreversible commitment is not identified",
                )
            } else if has_open_high_ambiguity(input) {
                decision(
                    input,
                    GateStatus::Blocked,
                    "critical/high ambiguity affects the next commitment",
                )
            } else if has_unmitigated_medium_ambiguity(input) {
                decision(
                    input,
                    GateStatus::Blocked,
                    "medium ambiguity lacks mitigation or revisit trigger",
                )
            } else {
                decision(
                    input,
                    GateStatus::Pass,
                    "no unresolved critical/high ambiguity blocks the next commitment",
                )
            }
        }
        GateId::Reuse => {
            if !input.reuse_scan_complete {
                decision(input, GateStatus::Blocked, "reuse scan is incomplete")
            } else if input.build_capabilities_have_evidence && input.evidence.is_empty() {
                decision(
                    input,
                    GateStatus::MissingEvidence,
                    "BUILD capability lacks alternative evidence",
                )
            } else {
                decision(
                    input,
                    GateStatus::Pass,
                    "reuse classifications and BUILD evidence are present",
                )
            }
        }
        GateId::Architecture => match &input.architecture {
            None => decision(
                input,
                GateStatus::MissingEvidence,
                "architecture competition record is missing",
            ),
            Some(proof) if proof.proposal_count < 2 => decision(
                input,
                GateStatus::Blocked,
                "architecture gate requires two materially different proposals",
            ),
            Some(proof)
                if !proof.hard_constraints_checked
                    || !proof.reuse_scan_complete
                    || !proof.risky_assumptions_tested
                    || !proof.red_team_complete
                    || proof.unresolved_high_technical_blocker
                    || !proof.dissent_preserved =>
            {
                decision(
                    input,
                    GateStatus::Blocked,
                    "architecture evidence is incomplete or has an unresolved high blocker",
                )
            }
            Some(_) => decision(
                input,
                GateStatus::Pass,
                "architecture competition and challenge evidence are complete",
            ),
        },
        GateId::BuildReadiness => {
            if input.build_ready {
                decision(
                    input,
                    GateStatus::Pass,
                    "accepted product and architecture package is actionable",
                )
            } else {
                decision(
                    input,
                    GateStatus::Blocked,
                    "build package still requires product or technical clarification",
                )
            }
        }
        GateId::Implementation => match &input.implementation {
            None => decision(
                input,
                GateStatus::MissingEvidence,
                "current verifier evidence is missing",
            ),
            Some(proof)
                if !proof.verifier_pass
                    || proof.candidate_revision != proof.verified_candidate_revision =>
            {
                decision(
                    input,
                    GateStatus::Blocked,
                    "independent verifier has not passed the current candidate",
                )
            }
            Some(_) => decision(
                input,
                GateStatus::Pass,
                "independent verifier passed the current candidate",
            ),
        },
        GateId::Release => match &input.release {
            None => decision(
                input,
                GateStatus::MissingEvidence,
                "release evidence package is missing",
            ),
            Some(proof)
                if !proof.candidate_evidence
                    || !proof.package_evidence
                    || !proof.install_evidence
                    || !proof.security_evidence
                    || !proof.qa_evidence =>
            {
                decision(
                    input,
                    GateStatus::Blocked,
                    "candidate, package, install, security, and QA evidence are all required",
                )
            }
            Some(_) => decision(input, GateStatus::Pass, "release evidence is complete"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(gate_id: GateId) -> GateInput {
        GateInput {
            gate_id,
            package_id: "package-1".to_string(),
            package_revision: 1,
            current_revision: 1,
            authority_fingerprint: "authority-1".to_string(),
            current_authority_fingerprint: "authority-1".to_string(),
            required_evidence: Vec::new(),
            evidence: vec![EvidenceItem {
                evidence_id: "evidence-1".to_string(),
                claim: "bounded claim".to_string(),
                source_reference: "source-1".to_string(),
                captured_at: "2026-09-17T00:00:00Z".to_string(),
                summary: "current evidence".to_string(),
                provenance: EvidenceProvenance::SourceConfirmed,
                current: true,
            }],
            ambiguities: Vec::new(),
            next_irreversible_commitment: Some("commitment-1".to_string()),
            reviewer_restatement: None,
            decision_outcome: Some(DecisionOutcome::NarrowBuild),
            owner_decision_required: false,
            owner_decision_recorded: false,
            reuse_scan_complete: true,
            build_capabilities_have_evidence: false,
            architecture: None,
            build_ready: true,
            implementation: Some(ImplementationProof {
                candidate_revision: 1,
                verified_candidate_revision: 1,
                verifier_pass: true,
            }),
            release: Some(ReleaseProof {
                candidate_evidence: true,
                package_evidence: true,
                install_evidence: true,
                security_evidence: true,
                qa_evidence: true,
            }),
        }
    }

    #[test]
    fn stale_revision_cannot_pass_any_gate() {
        let mut input = base(GateId::Vision);
        input.reviewer_restatement = Some(ReviewerRestatement {
            intended_outcome: "outcome".to_string(),
            success_condition: "condition".to_string(),
            invented_behaviors: Vec::new(),
        });
        input.current_revision = 2;
        assert_eq!(evaluate(&input).status, GateStatus::Stale);
    }

    #[test]
    fn stale_evidence_item_reopens_the_gate() {
        let mut input = base(GateId::ProblemResearch);
        input.evidence[0].current = false;
        assert_eq!(evaluate(&input).status, GateStatus::Stale);
    }

    #[test]
    fn changed_authority_fingerprint_reopens_the_gate() {
        let mut input = base(GateId::ProblemResearch);
        input.current_authority_fingerprint = "authority-2".to_string();
        assert_eq!(evaluate(&input).status, GateStatus::Stale);
    }

    #[test]
    fn missing_authority_fingerprint_is_not_inferred_as_current() {
        let mut input = base(GateId::ProblemResearch);
        input.authority_fingerprint.clear();
        assert_eq!(evaluate(&input).status, GateStatus::MissingEvidence);
    }

    #[test]
    fn missing_required_evidence_is_not_inferred_as_pass() {
        let mut input = base(GateId::Reuse);
        input.required_evidence = vec!["reuse-alternatives".to_string()];
        assert_eq!(evaluate(&input).status, GateStatus::MissingEvidence);
    }

    #[test]
    fn build_requires_evidence_when_reuse_scan_classifies_build() {
        let mut input = base(GateId::Reuse);
        input.evidence.clear();
        input.build_capabilities_have_evidence = true;
        assert_eq!(evaluate(&input).status, GateStatus::MissingEvidence);
    }

    #[test]
    fn vision_rejects_invented_behavior_and_missing_restatement() {
        let mut input = base(GateId::Vision);
        assert_eq!(evaluate(&input).status, GateStatus::MissingEvidence);
        input.reviewer_restatement = Some(ReviewerRestatement {
            intended_outcome: "outcome".to_string(),
            success_condition: "condition".to_string(),
            invented_behaviors: vec!["unsupported feature".to_string()],
        });
        assert_eq!(evaluate(&input).status, GateStatus::Blocked);
    }

    #[test]
    fn ambiguity_gate_requires_high_resolution_and_medium_mitigation() {
        let mut input = base(GateId::Ambiguity);
        input.ambiguities.push(AmbiguityItem {
            ambiguity_id: "a1".to_string(),
            affected_commitment: "commitment-1".to_string(),
            severity: AmbiguitySeverity::High,
            status: AmbiguityStatus::Open,
            mitigation: None,
            revisit_trigger: None,
            owner_decision_required: false,
            owner_decision_recorded: false,
        });
        assert_eq!(evaluate(&input).status, GateStatus::Blocked);
        input.ambiguities[0].severity = AmbiguitySeverity::Medium;
        input.ambiguities[0].mitigation = Some("bounded spike".to_string());
        input.ambiguities[0].revisit_trigger = Some("before apply".to_string());
        assert_eq!(evaluate(&input).status, GateStatus::Pass);
    }

    #[test]
    fn architecture_gate_requires_competition_and_red_team() {
        let mut input = base(GateId::Architecture);
        input.architecture = Some(ArchitectureProof {
            proposal_count: 1,
            hard_constraints_checked: true,
            reuse_scan_complete: true,
            risky_assumptions_tested: true,
            red_team_complete: true,
            unresolved_high_technical_blocker: false,
            dissent_preserved: true,
        });
        assert_eq!(evaluate(&input).status, GateStatus::Blocked);
        input
            .architecture
            .as_mut()
            .expect("test proof")
            .proposal_count = 2;
        assert_eq!(evaluate(&input).status, GateStatus::Pass);
    }

    #[test]
    fn implementation_requires_current_verifier_pass() {
        let mut input = base(GateId::Implementation);
        input
            .implementation
            .as_mut()
            .expect("test proof")
            .candidate_revision = 2;
        assert_eq!(evaluate(&input).status, GateStatus::Blocked);
        input
            .implementation
            .as_mut()
            .expect("test proof")
            .candidate_revision = 1;
        assert_eq!(evaluate(&input).status, GateStatus::Pass);
    }

    #[test]
    fn owner_decision_is_not_invented_by_technical_evaluation() {
        let mut input = base(GateId::Positioning);
        input.owner_decision_required = true;
        assert_eq!(evaluate(&input).status, GateStatus::Blocked);
        assert!(evaluate(&input).owner_authority_required);
        input.owner_decision_recorded = true;
        assert_eq!(evaluate(&input).status, GateStatus::Pass);
    }

    #[test]
    fn release_requires_all_evidence_categories() {
        let mut input = base(GateId::Release);
        input.release.as_mut().expect("test proof").qa_evidence = false;
        assert_eq!(evaluate(&input).status, GateStatus::Blocked);
        input.release.as_mut().expect("test proof").qa_evidence = true;
        assert_eq!(evaluate(&input).status, GateStatus::Pass);
    }
}
