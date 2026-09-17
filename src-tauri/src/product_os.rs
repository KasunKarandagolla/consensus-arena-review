use crate::evidence_gates::{
    self, AmbiguityItem, AmbiguitySeverity, AmbiguityStatus, DecisionOutcome, EvidenceItem,
    GateDecision, GateId, GateInput, ReviewerRestatement,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Arena-owned status for a persisted owner decision. Runtime output may
/// propose a decision, but only an adopted current record can resolve an
/// owner-required ambiguity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Proposed,
    Adopted,
    Cancelled,
    Superseded,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAuthority {
    Owner,
    Technical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnerDecisionRecord {
    pub decision_id: String,
    pub question_id: String,
    pub selected_option: String,
    pub revision: u64,
    pub status: DecisionStatus,
    pub authority: DecisionAuthority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityResolver {
    Owner,
    Technical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorityAmbiguityRecord {
    pub ambiguity_id: String,
    pub question: String,
    pub affected_commitment: String,
    pub severity: AmbiguitySeverity,
    pub status: AmbiguityStatus,
    pub resolver: AmbiguityResolver,
    pub owner_decision_id: Option<String>,
    pub mitigation: Option<String>,
    pub revisit_trigger: Option<String>,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReuseClassification {
    Reuse,
    Wrap,
    Adapt,
    Compose,
    Build,
    Defer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReuseDecisionRecord {
    pub capability: String,
    pub classification: ReuseClassification,
    pub evidence_ids: Vec<String>,
}

/// Architecture proof is represented by references to current evidence, not
/// caller-supplied completion flags. The pure gate evaluator receives the
/// derived summary only after this record has been assembled by Arena.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureEvidenceRecords {
    pub architecture_version: u64,
    pub proposal_a_evidence_id: String,
    pub proposal_b_evidence_id: String,
    pub reuse_review_evidence_id: String,
    pub constraints_review_evidence_id: String,
    pub risk_experiment_evidence_ids: Vec<String>,
    pub red_team_evidence_id: String,
    pub dissent_evidence_id: String,
    pub unresolved_high_blocker_evidence_ids: Vec<String>,
}

/// The minimum current Product OS records required to assemble a Build
/// Package. This is persisted through existing Arena state; it is not a new
/// database or event-sourcing system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductAuthorityRecords {
    pub project_id: String,
    pub project_revision: u64,
    pub vision_version: u64,
    pub objective: String,
    pub target_user: String,
    pub requirements: Vec<String>,
    pub constraints: Vec<String>,
    pub non_goals: Vec<String>,
    pub interfaces: Vec<String>,
    pub risks: Vec<String>,
    pub acceptance_scenarios: Vec<String>,
    pub decision_outcome: Option<DecisionOutcome>,
    pub reviewer_restatement: Option<ReviewerRestatement>,
    pub evidence: Vec<EvidenceItem>,
    pub owner_decisions: Vec<OwnerDecisionRecord>,
    pub ambiguities: Vec<AuthorityAmbiguityRecord>,
    pub reuse_decisions: Vec<ReuseDecisionRecord>,
    pub architecture: ArchitectureEvidenceRecords,
    pub acceptance_profile_version: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildPackage {
    pub package_id: String,
    pub package_revision: u64,
    pub project_id: String,
    pub project_revision: u64,
    pub vision_version: u64,
    pub architecture_version: u64,
    pub acceptance_profile_version: Option<u64>,
    pub authority_fingerprint: String,
    pub objective: String,
    pub target_user: String,
    pub requirements: Vec<String>,
    pub constraints: Vec<String>,
    pub non_goals: Vec<String>,
    pub interfaces: Vec<String>,
    pub risks: Vec<String>,
    pub acceptance_scenarios: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub owner_decision_ids: Vec<String>,
    pub ambiguity_ids: Vec<String>,
    pub architecture_evidence_ids: Vec<String>,
    pub reuse_classifications: Vec<ReuseClassification>,
}

fn non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("Build Package requires {field}"))
    } else {
        Ok(())
    }
}

fn require_list(values: &[String], field: &str) -> Result<(), String> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        Err(format!("Build Package requires current {field}"))
    } else {
        Ok(())
    }
}

fn referenced_evidence<'a>(
    records: &'a ProductAuthorityRecords,
    evidence_id: &str,
) -> Result<&'a EvidenceItem, String> {
    let evidence = records
        .evidence
        .iter()
        .find(|item| item.evidence_id == evidence_id)
        .ok_or_else(|| format!("authoritative evidence reference is missing: {evidence_id}"))?;
    if !evidence.current {
        return Err(format!(
            "authoritative evidence reference is stale: {evidence_id}"
        ));
    }
    Ok(evidence)
}

fn referenced_many(records: &ProductAuthorityRecords, ids: &[String]) -> Result<(), String> {
    if ids.is_empty() {
        return Err("Build Package requires referenced evidence".to_string());
    }
    for id in ids {
        referenced_evidence(records, id)?;
    }
    Ok(())
}

fn adopted_owner_decision<'a>(
    records: &'a ProductAuthorityRecords,
    decision_id: &str,
) -> Option<&'a OwnerDecisionRecord> {
    records.owner_decisions.iter().find(|decision| {
        decision.decision_id == decision_id
            && decision.authority == DecisionAuthority::Owner
            && decision.status == DecisionStatus::Adopted
    })
}

fn validate_owner_ambiguities(records: &ProductAuthorityRecords) -> Result<(), String> {
    for ambiguity in &records.ambiguities {
        if ambiguity.resolver == AmbiguityResolver::Owner {
            let decision_id = ambiguity.owner_decision_id.as_deref().ok_or_else(|| {
                format!(
                    "owner-required ambiguity lacks a decision record: {}",
                    ambiguity.ambiguity_id
                )
            })?;
            if adopted_owner_decision(records, decision_id).is_none() {
                return Err(format!(
                    "owner-required ambiguity lacks a current adopted decision: {}",
                    ambiguity.ambiguity_id
                ));
            }
        }
        referenced_many(records, &ambiguity.evidence_ids)?;
    }
    Ok(())
}

fn validate_architecture(records: &ProductAuthorityRecords) -> Result<Vec<String>, String> {
    let architecture = &records.architecture;
    let ids = vec![
        architecture.proposal_a_evidence_id.clone(),
        architecture.proposal_b_evidence_id.clone(),
        architecture.reuse_review_evidence_id.clone(),
        architecture.constraints_review_evidence_id.clone(),
        architecture.red_team_evidence_id.clone(),
        architecture.dissent_evidence_id.clone(),
    ];
    for id in &ids {
        referenced_evidence(records, id)?;
    }
    referenced_many(records, &architecture.risk_experiment_evidence_ids)?;
    for id in &architecture.unresolved_high_blocker_evidence_ids {
        referenced_evidence(records, id)?;
    }
    Ok(ids
        .into_iter()
        .chain(architecture.risk_experiment_evidence_ids.iter().cloned())
        .chain(
            architecture
                .unresolved_high_blocker_evidence_ids
                .iter()
                .cloned(),
        )
        .collect())
}

fn fingerprint_records(records: &ProductAuthorityRecords) -> Result<String, String> {
    // Historical superseded evidence remains inspectable, but it is not part
    // of the current authority fingerprint. A stale unrelated record must not
    // invalidate an otherwise current package; referenced stale evidence is
    // still rejected by the assembler above.
    let mut current = records.clone();
    current.evidence.retain(|item| item.current);
    let bytes = serde_json::to_vec(&current)
        .map_err(|error| format!("serialize Product OS authority records: {error}"))?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn current_evidence_ids(records: &ProductAuthorityRecords) -> Vec<String> {
    records
        .evidence
        .iter()
        .filter(|item| item.current)
        .map(|item| item.evidence_id.clone())
        .collect()
}

/// Assemble the only production-facing Build Package path. The summaries
/// consumed by `evidence_gates::evaluate` are derived here from current
/// Arena-owned records, never accepted from worker/renderer booleans.
pub fn assemble_build_package(records: &ProductAuthorityRecords) -> Result<BuildPackage, String> {
    non_empty(&records.project_id, "project identity")?;
    non_empty(&records.objective, "objective")?;
    non_empty(&records.target_user, "target user")?;
    require_list(&records.requirements, "requirements")?;
    require_list(&records.constraints, "constraints")?;
    require_list(&records.non_goals, "non-goals")?;
    require_list(&records.interfaces, "implementation interfaces")?;
    require_list(&records.risks, "known risks")?;
    require_list(&records.acceptance_scenarios, "acceptance scenarios")?;
    let architecture_evidence_ids = validate_architecture(records)?;
    validate_owner_ambiguities(records)?;
    if records.decision_outcome != Some(DecisionOutcome::NarrowBuild) {
        return Err(
            "only an owner-approved NarrowBuild outcome can create a Build Package".to_string(),
        );
    }

    if records.reuse_decisions.is_empty() {
        return Err("Build Package requires reuse decisions".to_string());
    }
    for decision in &records.reuse_decisions {
        non_empty(&decision.capability, "reuse capability")?;
        if decision.classification == ReuseClassification::Build {
            referenced_many(records, &decision.evidence_ids)?;
        }
    }
    let authority_fingerprint = fingerprint_records(records)?;
    let package_revision = records.project_revision;
    let package_id = format!("{}:build-package:{package_revision}", records.project_id);
    let owner_decision_ids = records
        .owner_decisions
        .iter()
        .filter(|decision| decision.status == DecisionStatus::Adopted)
        .map(|decision| decision.decision_id.clone())
        .collect();

    Ok(BuildPackage {
        package_id,
        package_revision,
        project_id: records.project_id.clone(),
        project_revision: records.project_revision,
        vision_version: records.vision_version,
        architecture_version: records.architecture.architecture_version,
        acceptance_profile_version: records.acceptance_profile_version,
        authority_fingerprint,
        objective: records.objective.clone(),
        target_user: records.target_user.clone(),
        requirements: records.requirements.clone(),
        constraints: records.constraints.clone(),
        non_goals: records.non_goals.clone(),
        interfaces: records.interfaces.clone(),
        risks: records.risks.clone(),
        acceptance_scenarios: records.acceptance_scenarios.clone(),
        evidence_ids: current_evidence_ids(records),
        owner_decision_ids,
        ambiguity_ids: records
            .ambiguities
            .iter()
            .map(|ambiguity| ambiguity.ambiguity_id.clone())
            .collect(),
        architecture_evidence_ids,
        reuse_classifications: records
            .reuse_decisions
            .iter()
            .map(|decision| decision.classification.clone())
            .collect(),
    })
}

fn input_for_records(
    records: &ProductAuthorityRecords,
    package: &BuildPackage,
    gate_id: GateId,
) -> Result<GateInput, String> {
    let current_fingerprint = fingerprint_records(records)?;
    let architecture = &records.architecture;
    let mut required_evidence = Vec::new();
    if gate_id == GateId::Architecture {
        required_evidence.extend(package.architecture_evidence_ids.clone());
    }
    if gate_id == GateId::Reuse {
        required_evidence.extend(
            records
                .reuse_decisions
                .iter()
                .filter(|decision| decision.classification == ReuseClassification::Build)
                .flat_map(|decision| decision.evidence_ids.clone()),
        );
    }
    let ambiguities = records
        .ambiguities
        .iter()
        .map(|ambiguity| AmbiguityItem {
            ambiguity_id: ambiguity.ambiguity_id.clone(),
            affected_commitment: ambiguity.affected_commitment.clone(),
            severity: ambiguity.severity,
            status: ambiguity.status,
            mitigation: ambiguity.mitigation.clone(),
            revisit_trigger: ambiguity.revisit_trigger.clone(),
            owner_decision_required: ambiguity.resolver == AmbiguityResolver::Owner,
            owner_decision_recorded: ambiguity
                .owner_decision_id
                .as_deref()
                .is_some_and(|id| adopted_owner_decision(records, id).is_some()),
        })
        .collect::<Vec<_>>();
    let owner_decision_required = ambiguities
        .iter()
        .any(|ambiguity| ambiguity.owner_decision_required && !ambiguity.owner_decision_recorded);
    let build_ready = !package.objective.trim().is_empty()
        && !package.requirements.is_empty()
        && !package.constraints.is_empty()
        && !package.non_goals.is_empty()
        && !package.interfaces.is_empty()
        && !package.risks.is_empty()
        && !package.acceptance_scenarios.is_empty();

    Ok(GateInput {
        gate_id,
        package_id: package.package_id.clone(),
        package_revision: package.package_revision,
        current_revision: records.project_revision,
        authority_fingerprint: package.authority_fingerprint.clone(),
        current_authority_fingerprint: current_fingerprint,
        required_evidence,
        evidence: records
            .evidence
            .iter()
            .filter(|item| item.current)
            .cloned()
            .collect(),
        ambiguities,
        next_irreversible_commitment: records
            .ambiguities
            .first()
            .map(|ambiguity| ambiguity.affected_commitment.clone()),
        reviewer_restatement: records.reviewer_restatement.clone(),
        decision_outcome: records.decision_outcome,
        owner_decision_required,
        owner_decision_recorded: !owner_decision_required,
        reuse_scan_complete: !records.reuse_decisions.is_empty(),
        build_capabilities_have_evidence: records
            .reuse_decisions
            .iter()
            .any(|decision| decision.classification == ReuseClassification::Build),
        architecture: Some(evidence_gates::ArchitectureProof {
            proposal_count: 2,
            hard_constraints_checked: referenced_evidence(
                records,
                &architecture.constraints_review_evidence_id,
            )
            .is_ok(),
            reuse_scan_complete: !records.reuse_decisions.is_empty(),
            risky_assumptions_tested: !architecture.risk_experiment_evidence_ids.is_empty(),
            red_team_complete: referenced_evidence(records, &architecture.red_team_evidence_id)
                .is_ok(),
            unresolved_high_technical_blocker: !architecture
                .unresolved_high_blocker_evidence_ids
                .is_empty(),
            dissent_preserved: referenced_evidence(records, &architecture.dissent_evidence_id)
                .is_ok(),
        }),
        build_ready,
        implementation: None,
        release: None,
    })
}

impl BuildPackage {
    /// Evaluate a package against the current Arena records. A changed
    /// project/decision/evidence fingerprint therefore becomes stale without
    /// invalidating unrelated packages.
    pub fn evaluate_current(
        &self,
        records: &ProductAuthorityRecords,
        gate_id: GateId,
    ) -> Result<GateDecision, String> {
        let input = input_for_records(records, self, gate_id)?;
        Ok(evidence_gates::evaluate(&input))
    }

    pub fn is_current_for(&self, records: &ProductAuthorityRecords) -> Result<bool, String> {
        Ok(self.project_id == records.project_id
            && self.project_revision == records.project_revision
            && self.authority_fingerprint == fingerprint_records(records)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence_gates::{EvidenceProvenance, GateStatus};

    fn evidence(id: &str, summary: &str) -> EvidenceItem {
        EvidenceItem {
            evidence_id: id.to_string(),
            claim: summary.to_string(),
            source_reference: format!("source:{id}"),
            captured_at: "2026-09-17T00:00:00Z".to_string(),
            summary: summary.to_string(),
            provenance: EvidenceProvenance::RuntimeProven,
            current: true,
        }
    }

    fn records() -> ProductAuthorityRecords {
        ProductAuthorityRecords {
            project_id: "project-1".to_string(),
            project_revision: 1,
            vision_version: 1,
            objective: "Make the bounded candidate change".to_string(),
            target_user: "founder".to_string(),
            requirements: vec!["one requirement".to_string()],
            constraints: vec!["no deployment".to_string()],
            non_goals: vec!["no redesign".to_string()],
            interfaces: vec!["existing Delivery".to_string()],
            risks: vec!["worker output is untrusted".to_string()],
            acceptance_scenarios: vec!["candidate verifier passes".to_string()],
            decision_outcome: Some(DecisionOutcome::NarrowBuild),
            reviewer_restatement: Some(ReviewerRestatement {
                intended_outcome: "Make the bounded candidate change".to_string(),
                success_condition: "Verifier passes the current candidate".to_string(),
                invented_behaviors: Vec::new(),
            }),
            evidence: (1..=8)
                .map(|index| evidence(&format!("e{index}"), "current evidence"))
                .collect(),
            owner_decisions: vec![OwnerDecisionRecord {
                decision_id: "d1".to_string(),
                question_id: "q1".to_string(),
                selected_option: "proceed".to_string(),
                revision: 1,
                status: DecisionStatus::Adopted,
                authority: DecisionAuthority::Owner,
            }],
            ambiguities: vec![AuthorityAmbiguityRecord {
                ambiguity_id: "a1".to_string(),
                question: "Proceed?".to_string(),
                affected_commitment: "build".to_string(),
                severity: AmbiguitySeverity::Low,
                status: AmbiguityStatus::Resolved,
                resolver: AmbiguityResolver::Owner,
                owner_decision_id: Some("d1".to_string()),
                mitigation: None,
                revisit_trigger: None,
                evidence_ids: vec!["e1".to_string()],
            }],
            reuse_decisions: vec![ReuseDecisionRecord {
                capability: "Delivery".to_string(),
                classification: ReuseClassification::Reuse,
                evidence_ids: vec!["e2".to_string()],
            }],
            architecture: ArchitectureEvidenceRecords {
                architecture_version: 1,
                proposal_a_evidence_id: "e3".to_string(),
                proposal_b_evidence_id: "e4".to_string(),
                reuse_review_evidence_id: "e2".to_string(),
                constraints_review_evidence_id: "e5".to_string(),
                risk_experiment_evidence_ids: vec!["e6".to_string()],
                red_team_evidence_id: "e7".to_string(),
                dissent_evidence_id: "e8".to_string(),
                unresolved_high_blocker_evidence_ids: Vec::new(),
            },
            acceptance_profile_version: Some(1),
        }
    }

    #[test]
    fn builder_derives_architecture_and_build_readiness_from_references() {
        let records = records();
        let package = assemble_build_package(&records).expect("authority records should assemble");
        assert_eq!(
            package
                .evaluate_current(&records, GateId::Architecture)
                .expect("evaluate")
                .status,
            GateStatus::Pass
        );
        assert_eq!(
            package
                .evaluate_current(&records, GateId::BuildReadiness)
                .expect("evaluate")
                .status,
            GateStatus::Pass
        );
    }

    #[test]
    fn missing_architecture_reference_cannot_claim_red_team_pass() {
        let mut records = records();
        records.architecture.red_team_evidence_id = "missing".to_string();
        assert!(assemble_build_package(&records).is_err());
    }

    #[test]
    fn stale_owner_decision_cannot_resolve_owner_ambiguity() {
        let mut records = records();
        records.owner_decisions[0].status = DecisionStatus::Stale;
        assert!(assemble_build_package(&records).is_err());
    }

    #[test]
    fn changed_authoritative_records_make_package_stale() {
        let records = records();
        let package = assemble_build_package(&records).expect("assemble");
        let mut changed = records.clone();
        changed.project_revision = 2;
        assert!(!package.is_current_for(&changed).expect("fingerprint"));
        assert_eq!(
            package
                .evaluate_current(&changed, GateId::BuildReadiness)
                .expect("evaluate")
                .status,
            GateStatus::Stale
        );
    }

    #[test]
    fn technical_record_cannot_satisfy_owner_required_decision() {
        let mut records = records();
        records.owner_decisions[0].authority = DecisionAuthority::Technical;
        assert!(assemble_build_package(&records).is_err());
    }

    #[test]
    fn runtime_completion_cannot_forge_implementation_pass() {
        let records = records();
        let package = assemble_build_package(&records).expect("assemble");
        let decision = package
            .evaluate_current(&records, GateId::Implementation)
            .expect("evaluate");
        assert_eq!(decision.status, GateStatus::MissingEvidence);
    }

    #[test]
    fn package_identity_survives_delivery_state_serialization() {
        let records = records();
        let package = assemble_build_package(&records).expect("assemble");
        let encoded = serde_json::to_string(&package).expect("serialize package");
        let decoded: BuildPackage = serde_json::from_str(&encoded).expect("parse package");
        assert_eq!(decoded.package_id, package.package_id);
        assert_eq!(decoded.authority_fingerprint, package.authority_fingerprint);
    }

    #[test]
    fn unrelated_historical_evidence_does_not_invalidate_current_package() {
        let records = records();
        let package = assemble_build_package(&records).expect("assemble");
        let mut changed = records.clone();
        changed.evidence.push(EvidenceItem {
            evidence_id: "historical-unrelated".to_string(),
            claim: "old claim".to_string(),
            source_reference: "source:old".to_string(),
            captured_at: "2026-09-16T00:00:00Z".to_string(),
            summary: "superseded".to_string(),
            provenance: EvidenceProvenance::Documented,
            current: false,
        });
        assert!(package.is_current_for(&changed).expect("fingerprint"));
        assert_eq!(
            package
                .evaluate_current(&changed, GateId::BuildReadiness)
                .expect("evaluate")
                .status,
            GateStatus::Pass
        );
    }
}
