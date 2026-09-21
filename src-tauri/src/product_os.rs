use crate::evidence_gates::{
    self, AmbiguityItem, AmbiguitySeverity, AmbiguityStatus, DecisionOutcome, EvidenceItem,
    EvidenceKind, EvidenceOrigin, EvidenceSource, EvidenceVerification, GateDecision, GateId,
    GateInput, ReviewerRestatement,
};
use crate::pipeline_contract::{
    ArchitectureCompetitionMode, ArchitectureSynthesis, ProductRoute, ResolvedInputManifest,
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
    #[serde(default)]
    pub question_id: String,
    pub question: String,
    pub affected_commitment: String,
    pub severity: AmbiguitySeverity,
    pub status: AmbiguityStatus,
    pub resolver: AmbiguityResolver,
    pub owner_decision_id: Option<String>,
    pub mitigation: Option<String>,
    pub revisit_trigger: Option<String>,
    pub evidence_ids: Vec<String>,
    /// Only Arena's admission path may set this marker. A serialized record
    /// without it is not allowed to satisfy an owner-authority gate.
    #[serde(default)]
    pub arena_admitted: bool,
    #[serde(default)]
    pub admitted_revision: u64,
}

/// The small set of Product OS work-order roles needed by research. This is
/// deliberately not a general role/capability framework.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductWorkOrderRole {
    ResearchLead,
    Researcher,
    FactVerifier,
    ProductDirector,
    ArchitectA,
    ArchitectB,
    ChiefEngineer,
    ReuseReviewer,
    ConstraintsReviewer,
    RedTeamReviewer,
    DissentReviewer,
    FeasibilityReviewer,
    BrowserQa,
}

/// The two intentionally narrow research entry points. KnownSource preserves
/// the existing official GitHub path; WebDiscovery delegates discovery to the
/// qualified read-only OpenCode web tools and still enters Arena as a proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductResearchMode {
    KnownSource,
    WebDiscovery,
    ChannelResearch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductResearchCategory {
    UserProblem,
    CompetitorStatusQuo,
    PriorArtReuse,
    TechnicalCurrentFact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductWorkOrderStatus {
    Admitted,
    Running,
    Completed,
    Cancelled,
    Superseded,
    Failed,
    Unavailable,
    ReconciliationRequired,
}

/// Durable metadata for an Arena-owned Product OS task. The live process
/// lease remains SessionRuntime's responsibility; this record is the restart
/// and result-correlation boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductWorkOrder {
    pub work_order_id: String,
    pub project_id: String,
    pub session_id: String,
    #[serde(default)]
    pub runtime_session_id: Option<String>,
    pub run_generation: u64,
    pub role: ProductWorkOrderRole,
    pub status: ProductWorkOrderStatus,
    pub project_revision: u64,
    pub question: Option<String>,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub research_mode: Option<ProductResearchMode>,
    #[serde(default)]
    pub research_category: Option<ProductResearchCategory>,
    /// Optional explicit research platform for a channel-specialist child.
    #[serde(default)]
    pub research_channel: Option<crate::work_graph::ResearchChannel>,
    pub parent_work_order_id: Option<String>,
    /// Arena-owned per-work-order model binding. None inherits the configured
    /// default; workers cannot choose or mutate this value themselves.
    #[serde(default)]
    pub model_id: Option<String>,
    /// Bounded hierarchy depth for manager -> specialist -> child specialist
    /// delegation. Arena, not the model, creates the actual child work order.
    #[serde(default)]
    pub delegation_depth: u8,
    pub evidence_id: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    /// The resolved content delivered to the role. References alone are not
    /// an input manifest and cannot satisfy an architecture review.
    #[serde(default)]
    pub input_manifest: Option<ResolvedInputManifest>,
    pub result_ref: Option<String>,
    pub cancellation_reason: Option<String>,
    pub superseded_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
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
    #[serde(default)]
    pub candidate: String,
    #[serde(default)]
    pub alternatives: Vec<String>,
    #[serde(default)]
    pub rationale: String,
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
    #[serde(default)]
    pub competition_mode: ArchitectureCompetitionMode,
    #[serde(default)]
    pub synthesis: Option<ArchitectureSynthesis>,
}

/// The minimum current Product OS records required to assemble a Build
/// Package. This is persisted through existing Arena state; it is not a new
/// database or event-sourcing system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductAuthorityRecords {
    pub project_id: String,
    pub project_revision: u64,
    pub vision_version: u64,
    #[serde(default)]
    pub route: ProductRoute,
    pub objective: String,
    pub target_user: String,
    pub requirements: Vec<String>,
    pub constraints: Vec<String>,
    pub non_goals: Vec<String>,
    pub interfaces: Vec<String>,
    pub risks: Vec<String>,
    pub acceptance_scenarios: Vec<String>,
    pub decision_outcome: Option<DecisionOutcome>,
    /// The adopted Arena owner decision that authorizes `decision_outcome`.
    /// A bare serialized enum is never sufficient to admit a Build Package.
    #[serde(default)]
    pub product_direction_decision_id: Option<String>,
    pub reviewer_restatement: Option<ReviewerRestatement>,
    pub evidence: Vec<EvidenceItem>,
    pub owner_decisions: Vec<OwnerDecisionRecord>,
    pub ambiguities: Vec<AuthorityAmbiguityRecord>,
    /// Arena-owned classification memory. A caller changing the serialized
    /// resolver field cannot remove an owner gate through a worker payload.
    #[serde(default)]
    pub owner_required_ambiguity_ids: Vec<String>,
    pub reuse_decisions: Vec<ReuseDecisionRecord>,
    pub architecture: ArchitectureEvidenceRecords,
    pub acceptance_profile_version: Option<u64>,
}

/// A bounded, reviewed product scope admission. This is intentionally typed
/// rather than a raw ProductAuthorityRecords update so callers cannot smuggle
/// gate facts or a decision outcome into authoritative state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductScopeAdmission {
    pub objective: String,
    pub target_user: String,
    pub requirements: Vec<String>,
    pub constraints: Vec<String>,
    pub non_goals: Vec<String>,
    pub interfaces: Vec<String>,
    pub risks: Vec<String>,
    pub acceptance_scenarios: Vec<String>,
    pub reviewer_restatement: ReviewerRestatement,
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

/// Admit a reviewed bounded scope. A material scope update invalidates a
/// previously adopted direction; the owner must make a new current decision.
pub fn admit_product_scope(
    records: &mut ProductAuthorityRecords,
    scope: ProductScopeAdmission,
) -> Result<(), String> {
    non_empty(&scope.objective, "objective")?;
    non_empty(&scope.target_user, "target user")?;
    require_list(&scope.requirements, "requirements")?;
    require_list(&scope.constraints, "constraints")?;
    require_list(&scope.non_goals, "non-goals")?;
    require_list(&scope.interfaces, "implementation interfaces")?;
    require_list(&scope.risks, "known risks")?;
    require_list(&scope.acceptance_scenarios, "acceptance scenarios")?;
    non_empty(
        &scope.reviewer_restatement.intended_outcome,
        "reviewer restatement outcome",
    )?;
    non_empty(
        &scope.reviewer_restatement.success_condition,
        "reviewer restatement success condition",
    )?;

    records.objective = scope.objective;
    records.target_user = scope.target_user;
    records.requirements = scope.requirements;
    records.constraints = scope.constraints;
    records.non_goals = scope.non_goals;
    records.interfaces = scope.interfaces;
    records.risks = scope.risks;
    records.acceptance_scenarios = scope.acceptance_scenarios;
    records.reviewer_restatement = Some(scope.reviewer_restatement);
    records.decision_outcome = None;
    records.product_direction_decision_id = None;

    // A material scope revision invalidates every downstream technical
    // commitment. Research evidence remains available as historical/current
    // input, but reuse classification, architecture synthesis, and acceptance
    // profile must be rebuilt for the new scope before Delivery can re-enter.
    records.reuse_decisions.clear();
    records.architecture = ArchitectureEvidenceRecords {
        architecture_version: records.architecture.architecture_version.saturating_add(1),
        proposal_a_evidence_id: String::new(),
        proposal_b_evidence_id: String::new(),
        reuse_review_evidence_id: String::new(),
        constraints_review_evidence_id: String::new(),
        risk_experiment_evidence_ids: Vec::new(),
        red_team_evidence_id: String::new(),
        dissent_evidence_id: String::new(),
        unresolved_high_blocker_evidence_ids: Vec::new(),
        competition_mode: ArchitectureCompetitionMode::CompetingProposals,
        synthesis: None,
    };
    records.acceptance_profile_version = None;
    records.vision_version = records.vision_version.saturating_add(1);
    records.project_revision = records.project_revision.saturating_add(1);
    Ok(())
}

/// Invalidate downstream technical commitments after new owner guidance or
/// newly-required evidence changes the decision basis. Current research
/// evidence remains inspectable; architecture, build direction and frozen
/// acceptance must be re-established against the new revision.
pub fn invalidate_downstream_for_owner_guidance(records: &mut ProductAuthorityRecords) {
    records.decision_outcome = None;
    records.product_direction_decision_id = None;
    for ambiguity in &mut records.ambiguities {
        if ambiguity.resolver == AmbiguityResolver::Owner
            && ambiguity.status == AmbiguityStatus::Open
        {
            ambiguity.status = AmbiguityStatus::Deferred;
            ambiguity.mitigation = Some("superseded by newer durable owner guidance".to_string());
            ambiguity.revisit_trigger =
                Some("re-open only if the newer owner guidance requires this choice".to_string());
        }
    }
    records.owner_required_ambiguity_ids.clear();
    records.reuse_decisions.clear();
    records.architecture = ArchitectureEvidenceRecords {
        architecture_version: records.architecture.architecture_version.saturating_add(1),
        proposal_a_evidence_id: String::new(),
        proposal_b_evidence_id: String::new(),
        reuse_review_evidence_id: String::new(),
        constraints_review_evidence_id: String::new(),
        risk_experiment_evidence_ids: Vec::new(),
        red_team_evidence_id: String::new(),
        dissent_evidence_id: String::new(),
        unresolved_high_blocker_evidence_ids: Vec::new(),
        competition_mode: ArchitectureCompetitionMode::CompetingProposals,
        synthesis: None,
    };
    records.acceptance_profile_version = None;
    records.project_revision = records.project_revision.saturating_add(1);
}

/// Adopt an owner-approved bounded product direction. This is the only
/// Product OS operation that can set a current NarrowBuild direction.
pub fn adopt_product_direction(
    records: &mut ProductAuthorityRecords,
    selected_option: String,
) -> Result<String, String> {
    if selected_option.trim() != "narrow_build" {
        return Err("Build Package admission requires the owner option narrow_build".to_string());
    }
    let decision_id = if records.route == ProductRoute::NewProduct {
        records
            .owner_decisions
            .iter()
            .rev()
            .find(|decision| {
                decision.authority == DecisionAuthority::Owner
                    && decision.status == DecisionStatus::Adopted
                    && matches!(
                        decision.selected_option.as_str(),
                        "narrow_build" | "authorize_narrow_build"
                    )
                    && decision.revision.saturating_add(1) == records.project_revision
                    && records.ambiguities.iter().any(|ambiguity| {
                        ambiguity.arena_admitted
                            && ambiguity.resolver == AmbiguityResolver::Owner
                            && ambiguity.status == AmbiguityStatus::Resolved
                            && ambiguity.question_id == decision.question_id
                            && ambiguity.owner_decision_id.as_deref()
                                == Some(decision.decision_id.as_str())
                    })
            })
            .map(|decision| decision.decision_id.clone())
            .ok_or_else(|| {
                "NewProduct NarrowBuild requires a fresh explicit owner decision before product-direction adoption"
                    .to_string()
            })?
    } else {
        let decision_id = format!(
            "product-direction:{}:{}",
            records.project_id, records.project_revision
        );
        records.owner_decisions.push(OwnerDecisionRecord {
            decision_id: decision_id.clone(),
            question_id: format!("founder-mandate:{}", records.project_id),
            selected_option,
            revision: records.project_revision,
            status: DecisionStatus::Adopted,
            authority: DecisionAuthority::Owner,
        });
        decision_id
    };
    records.decision_outcome = Some(DecisionOutcome::NarrowBuild);
    records.product_direction_decision_id = Some(decision_id.clone());
    records.project_revision = records.project_revision.saturating_add(1);
    Ok(decision_id)
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

fn referenced_kind(
    records: &ProductAuthorityRecords,
    evidence_id: &str,
    expected: EvidenceKind,
) -> Result<(), String> {
    let evidence = referenced_evidence(records, evidence_id)?;
    if evidence.kind != Some(expected) {
        return Err(format!(
            "authoritative evidence reference has the wrong role: {evidence_id}"
        ));
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
    for ambiguity_id in &records.owner_required_ambiguity_ids {
        let ambiguity = records
            .ambiguities
            .iter()
            .find(|item| item.ambiguity_id == *ambiguity_id)
            .ok_or_else(|| format!("owner-required ambiguity is missing: {ambiguity_id}"))?;
        if ambiguity.resolver != AmbiguityResolver::Owner || !ambiguity.arena_admitted {
            return Err(format!(
                "owner-required ambiguity classification was weakened: {}",
                ambiguity.ambiguity_id
            ));
        }
        let decision_id = ambiguity.owner_decision_id.as_deref().ok_or_else(|| {
            format!(
                "owner-required ambiguity lacks a decision record: {}",
                ambiguity.ambiguity_id
            )
        })?;
        let decision = adopted_owner_decision(records, decision_id);
        if decision.is_none_or(|decision| {
            decision.question_id != ambiguity.question_id
                || decision.revision != ambiguity.admitted_revision
                || ambiguity.status != AmbiguityStatus::Resolved
        }) {
            return Err(format!(
                "owner-required ambiguity lacks a current adopted decision: {}",
                ambiguity.ambiguity_id
            ));
        }
    }
    for ambiguity in &records.ambiguities {
        if ambiguity.resolver == AmbiguityResolver::Owner
            && !records
                .owner_required_ambiguity_ids
                .iter()
                .any(|id| id == &ambiguity.ambiguity_id)
        {
            return Err(format!(
                "owner-required ambiguity lacks Arena classification: {}",
                ambiguity.ambiguity_id
            ));
        }
        referenced_many(records, &ambiguity.evidence_ids)?;
    }
    Ok(())
}

/// Admit an ambiguity proposal through Arena. The caller's resolver
/// classification is intentionally ignored: uncertain ambiguity is
/// conservative owner-required authority, not an agent-controlled field.
pub fn admit_ambiguity(
    records: &mut ProductAuthorityRecords,
    ambiguity_id: String,
    question_id: String,
    question: String,
    affected_commitment: String,
    severity: AmbiguitySeverity,
    evidence_ids: Vec<String>,
) -> Result<(), String> {
    if ambiguity_id.trim().is_empty()
        || question_id.trim().is_empty()
        || question.trim().is_empty()
        || affected_commitment.trim().is_empty()
    {
        return Err("ambiguity admission requires identity, question, and commitment".to_string());
    }
    if evidence_ids.iter().any(|id| id.trim().is_empty()) {
        return Err("ambiguity evidence references must be non-empty".to_string());
    }
    for evidence_id in &evidence_ids {
        referenced_evidence(records, evidence_id)?;
    }
    records
        .ambiguities
        .retain(|item| item.ambiguity_id != ambiguity_id);
    records
        .owner_required_ambiguity_ids
        .retain(|id| id != &ambiguity_id);
    let admitted_revision = records.project_revision.saturating_add(1);
    records.ambiguities.push(AuthorityAmbiguityRecord {
        ambiguity_id,
        question_id,
        question,
        affected_commitment,
        severity,
        status: AmbiguityStatus::Open,
        resolver: AmbiguityResolver::Owner,
        owner_decision_id: None,
        mitigation: None,
        revisit_trigger: None,
        evidence_ids,
        arena_admitted: true,
        admitted_revision,
    });
    records.owner_required_ambiguity_ids.push(
        records
            .ambiguities
            .last()
            .map(|item| item.ambiguity_id.clone())
            .ok_or_else(|| "ambiguity admission failed".to_string())?,
    );
    records.project_revision = records.project_revision.saturating_add(1);
    Ok(())
}

/// Adopt a human owner decision against the exact current admitted question.
/// Renderer payloads do not carry authority/status fields into this function.
pub fn adopt_owner_decision(
    records: &mut ProductAuthorityRecords,
    ambiguity_id: &str,
    question_id: &str,
    selected_option: String,
) -> Result<String, String> {
    if selected_option.trim().is_empty() {
        return Err("owner decision requires a selected option".to_string());
    }
    let ambiguity_index = records
        .ambiguities
        .iter()
        .position(|item| item.ambiguity_id == ambiguity_id)
        .ok_or_else(|| "owner ambiguity is unknown".to_string())?;
    let ambiguity = &records.ambiguities[ambiguity_index];
    if !ambiguity.arena_admitted
        || ambiguity.resolver != AmbiguityResolver::Owner
        || ambiguity.question_id != question_id
        || ambiguity.status != AmbiguityStatus::Open
        || records.project_revision != ambiguity.admitted_revision
    {
        return Err("owner decision is stale, mismatched, or not owner-required".to_string());
    }
    let decision_id = format!("{ambiguity_id}:owner-decision:{}", records.project_revision);
    let admitted_revision = ambiguity.admitted_revision;
    records.owner_decisions.push(OwnerDecisionRecord {
        decision_id: decision_id.clone(),
        question_id: question_id.to_string(),
        selected_option,
        revision: admitted_revision,
        status: DecisionStatus::Adopted,
        authority: DecisionAuthority::Owner,
    });
    if let Some(ambiguity) = records.ambiguities.get_mut(ambiguity_index) {
        ambiguity.owner_decision_id = Some(decision_id.clone());
        ambiguity.status = AmbiguityStatus::Resolved;
    }
    records.project_revision = records.project_revision.saturating_add(1);
    Ok(decision_id)
}

fn validate_architecture(records: &ProductAuthorityRecords) -> Result<Vec<String>, String> {
    let architecture = &records.architecture;
    if architecture.proposal_a_evidence_id.trim().is_empty() {
        return Err("architecture requires proposal A evidence".to_string());
    }
    if architecture.competition_mode == ArchitectureCompetitionMode::CompetingProposals
        && (architecture.proposal_b_evidence_id.trim().is_empty()
            || architecture.proposal_a_evidence_id == architecture.proposal_b_evidence_id)
    {
        return Err("architecture requires two distinct independent proposals".to_string());
    }
    let mut ids = Vec::new();
    if !architecture.proposal_a_evidence_id.is_empty() {
        referenced_kind(
            records,
            &architecture.proposal_a_evidence_id,
            EvidenceKind::ArchitectureProposal,
        )?;
        ids.push(architecture.proposal_a_evidence_id.clone());
    }
    if !architecture.proposal_b_evidence_id.is_empty() {
        referenced_kind(
            records,
            &architecture.proposal_b_evidence_id,
            EvidenceKind::ArchitectureProposal,
        )?;
        ids.push(architecture.proposal_b_evidence_id.clone());
    }
    if architecture.competition_mode == ArchitectureCompetitionMode::CompetingProposals
        && ids.len() != 2
    {
        return Err("competing architecture admission requires two proposals".to_string());
    }
    referenced_kind(
        records,
        &architecture.reuse_review_evidence_id,
        EvidenceKind::ReuseReview,
    )?;
    referenced_kind(
        records,
        &architecture.constraints_review_evidence_id,
        EvidenceKind::ConstraintsReview,
    )?;
    referenced_kind(
        records,
        &architecture.red_team_evidence_id,
        EvidenceKind::RedTeamReview,
    )?;
    referenced_kind(
        records,
        &architecture.dissent_evidence_id,
        EvidenceKind::Dissent,
    )?;
    for id in &architecture.risk_experiment_evidence_ids {
        referenced_kind(records, id, EvidenceKind::RiskExperiment)?;
    }
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

/// Admit a worker/researcher proposal as unverified evidence. This reuses the
/// existing Product OS evidence vector; it never advances a gate or package.
pub fn submit_research_proposal(
    records: &mut ProductAuthorityRecords,
    mut proposal: EvidenceItem,
) -> Result<(), String> {
    if proposal.evidence_id.trim().is_empty()
        || proposal.claim.trim().is_empty()
        || proposal.origin.is_none()
        || proposal.kind != Some(EvidenceKind::ResearchClaim)
    {
        return Err(
            "research proposal requires identity, claim, origin, and research kind".to_string(),
        );
    }
    if proposal.origin == Some(EvidenceOrigin::Verifier)
        || proposal.verification == Some(EvidenceVerification::IndependentlyVerified)
    {
        return Err("research proposals cannot self-declare independent verification".to_string());
    }
    proposal.current = true;
    proposal.verification = Some(EvidenceVerification::Unverified);
    if records
        .evidence
        .iter()
        .any(|item| item.evidence_id == proposal.evidence_id && item.current)
    {
        return Err("current research evidence identity already exists".to_string());
    }
    records.evidence.push(proposal);
    records.project_revision = records.project_revision.saturating_add(1);
    Ok(())
}

/// Finalize one current research claim from an Arena-owned reviewer work order
/// and primary-source reference. A contradiction is retained as evidence but
/// cannot satisfy the research gate.
pub fn finalize_research_claim(
    records: &mut ProductAuthorityRecords,
    evidence_id: &str,
    verifier_work_order_id: &str,
    source: EvidenceSource,
    verification: EvidenceVerification,
) -> Result<(), String> {
    if verifier_work_order_id.trim().is_empty()
        || source.reference.trim().is_empty()
        || source.checked_at.trim().is_empty()
        || source.version_or_scope.trim().is_empty()
        || !matches!(
            verification,
            EvidenceVerification::IndependentlyVerified
                | EvidenceVerification::Contradicted
                | EvidenceVerification::Unresolved
        )
    {
        return Err("research verification requires reviewer identity, source scope, and a bounded disposition".to_string());
    }
    let item = records
        .evidence
        .iter_mut()
        .find(|item| item.evidence_id == evidence_id && item.current)
        .ok_or_else(|| "research evidence is unknown or stale".to_string())?;
    if item.kind != Some(EvidenceKind::ResearchClaim) {
        return Err("only research claims can be independently finalized".to_string());
    }
    item.source = Some(source);
    item.verifier_work_order_id = Some(verifier_work_order_id.to_string());
    item.verification = Some(verification);
    Ok(())
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
    let direction_id = records
        .product_direction_decision_id
        .as_deref()
        .ok_or_else(|| {
            "NarrowBuild outcome lacks an Arena-adopted product direction decision".to_string()
        })?;
    let direction = adopted_owner_decision(records, direction_id)
        .ok_or_else(|| "NarrowBuild outcome lacks a current owner decision".to_string())?;
    let direction_is_bound = if records.route == ProductRoute::NewProduct {
        matches!(
            direction.selected_option.as_str(),
            "narrow_build" | "authorize_narrow_build"
        ) && records.ambiguities.iter().any(|ambiguity| {
            ambiguity.arena_admitted
                && ambiguity.resolver == AmbiguityResolver::Owner
                && ambiguity.status == AmbiguityStatus::Resolved
                && ambiguity.question_id == direction.question_id
                && ambiguity.owner_decision_id.as_deref() == Some(direction_id)
        })
    } else {
        direction.question_id == format!("founder-mandate:{}", records.project_id)
            && direction.selected_option == "narrow_build"
    };
    if !direction_is_bound {
        return Err(
            "NarrowBuild outcome is not bound to the current product direction".to_string(),
        );
    }

    if records.reuse_decisions.is_empty() {
        return Err("Build Package requires reuse decisions".to_string());
    }
    for decision in &records.reuse_decisions {
        non_empty(&decision.capability, "reuse capability")?;
        non_empty(&decision.candidate, "reuse candidate")?;
        non_empty(&decision.rationale, "reuse rationale")?;
        if decision.candidate == decision.capability {
            return Err("reuse decision must identify a project-specific candidate".to_string());
        }
        if decision.classification == ReuseClassification::Build {
            if decision.alternatives.is_empty() {
                return Err("BUILD reuse decision requires mature alternatives".to_string());
            }
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
            owner_decision_required: records
                .owner_required_ambiguity_ids
                .iter()
                .any(|id| id == &ambiguity.ambiguity_id),
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
            .filter(|item| {
                item.current
                    && (!matches!(gate_id, GateId::ProblemResearch | GateId::Positioning)
                        || item.kind == Some(EvidenceKind::ResearchClaim))
            })
            .cloned()
            .collect(),
        ambiguities,
        next_irreversible_commitment: records
            .ambiguities
            .iter()
            .find(|ambiguity| {
                matches!(
                    ambiguity.status,
                    AmbiguityStatus::Open | AmbiguityStatus::Deferred
                )
            })
            .map(|ambiguity| ambiguity.affected_commitment.clone())
            .or_else(|| match records.decision_outcome {
                Some(DecisionOutcome::NarrowBuild) => {
                    Some("implementation of the accepted bounded scope".to_string())
                }
                Some(DecisionOutcome::ValidationExperiment) => {
                    Some("execution of the accepted bounded validation experiment".to_string())
                }
                Some(DecisionOutcome::Stop | DecisionOutcome::Pivot) | None => None,
            }),
        reviewer_restatement: records.reviewer_restatement.clone(),
        decision_outcome: records.decision_outcome,
        research_required: records.route == ProductRoute::NewProduct,
        research_omission_reason: match records.route {
            ProductRoute::NewProduct => None,
            ProductRoute::ExistingFeature => Some(
                "existing feature route uses repository/context inspection instead of broad market discovery"
                    .to_string(),
            ),
            ProductRoute::Incident => Some(
                "incident route begins with reproduction/diagnosis instead of broad market discovery"
                    .to_string(),
            ),
        },
        owner_decision_required,
        owner_decision_recorded: !owner_decision_required,
        reuse_scan_complete: !records.reuse_decisions.is_empty(),
        build_capabilities_have_evidence: records
            .reuse_decisions
            .iter()
            .any(|decision| decision.classification == ReuseClassification::Build),
        architecture: Some(evidence_gates::ArchitectureProof {
            // Count the actual admitted proposal identities. Established
            // patterns may legitimately have one; competing architecture must
            // have two distinct proposals.
            proposal_count: u8::from(!architecture.proposal_a_evidence_id.is_empty())
                + u8::from(!architecture.proposal_b_evidence_id.is_empty()),
            established_pattern: architecture.competition_mode
                == ArchitectureCompetitionMode::EstablishedPattern,
            hard_constraints_checked: referenced_evidence(
                records,
                &architecture.constraints_review_evidence_id,
            )
            .is_ok(),
            reuse_scan_complete: !records.reuse_decisions.is_empty(),
            risky_assumptions_tested: architecture.synthesis.as_ref().is_some_and(|synthesis| {
                !synthesis.experiment_needed
                    || architecture.risk_experiment_evidence_ids.iter().any(|evidence_id| {
                        records.evidence.iter().any(|item| {
                            item.current
                                && item.evidence_id == *evidence_id
                                && item.kind == Some(EvidenceKind::RiskExperiment)
                                && item.verification
                                    == Some(EvidenceVerification::IndependentlyVerified)
                        })
                    })
            }),
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
            origin: Some(EvidenceOrigin::Verifier),
            verification: Some(EvidenceVerification::IndependentlyVerified),
            kind: None,
            source: Some(EvidenceSource {
                reference: format!("source:{id}"),
                url: None,
                title: None,
                checked_at: "2026-09-17T00:00:00Z".to_string(),
                version_or_scope: "test fixture".to_string(),
            }),
            verifier_work_order_id: Some("test-work-order".to_string()),
            contradiction_ids: Vec::new(),
            decision_impact: false,
            revisit_trigger: None,
            decision_question: None,
        }
    }

    fn records() -> ProductAuthorityRecords {
        ProductAuthorityRecords {
            project_id: "project-1".to_string(),
            project_revision: 1,
            vision_version: 1,
            route: ProductRoute::NewProduct,
            objective: "Make the bounded candidate change".to_string(),
            target_user: "founder".to_string(),
            requirements: vec!["one requirement".to_string()],
            constraints: vec!["no deployment".to_string()],
            non_goals: vec!["no redesign".to_string()],
            interfaces: vec!["existing Delivery".to_string()],
            risks: vec!["worker output is untrusted".to_string()],
            acceptance_scenarios: vec!["candidate verifier passes".to_string()],
            decision_outcome: Some(DecisionOutcome::NarrowBuild),
            product_direction_decision_id: Some("direction-owner-decision".to_string()),
            reviewer_restatement: Some(ReviewerRestatement {
                intended_outcome: "Make the bounded candidate change".to_string(),
                success_condition: "Verifier passes the current candidate".to_string(),
                invented_behaviors: Vec::new(),
            }),
            evidence: {
                let mut values = (1..=8)
                    .map(|index| evidence(&format!("e{index}"), "current evidence"))
                    .collect::<Vec<_>>();
                values[1].kind = Some(EvidenceKind::ReuseReview);
                values[2].kind = Some(EvidenceKind::ArchitectureProposal);
                values[3].kind = Some(EvidenceKind::ArchitectureProposal);
                values[4].kind = Some(EvidenceKind::ConstraintsReview);
                values[5].kind = Some(EvidenceKind::RiskExperiment);
                values[6].kind = Some(EvidenceKind::RedTeamReview);
                values[7].kind = Some(EvidenceKind::Dissent);
                values
            },
            owner_decisions: vec![
                OwnerDecisionRecord {
                    decision_id: "direction-owner-decision".to_string(),
                    question_id: "direction-question".to_string(),
                    selected_option: "narrow_build".to_string(),
                    revision: 1,
                    status: DecisionStatus::Adopted,
                    authority: DecisionAuthority::Owner,
                },
                OwnerDecisionRecord {
                    decision_id: "d1".to_string(),
                    question_id: "q1".to_string(),
                    selected_option: "proceed".to_string(),
                    revision: 1,
                    status: DecisionStatus::Adopted,
                    authority: DecisionAuthority::Owner,
                },
            ],
            ambiguities: vec![
                AuthorityAmbiguityRecord {
                    ambiguity_id: "direction-ambiguity".to_string(),
                    question_id: "direction-question".to_string(),
                    question: "Authorize the bounded build?".to_string(),
                    affected_commitment: "product direction".to_string(),
                    severity: AmbiguitySeverity::High,
                    status: AmbiguityStatus::Resolved,
                    resolver: AmbiguityResolver::Owner,
                    owner_decision_id: Some("direction-owner-decision".to_string()),
                    mitigation: None,
                    revisit_trigger: None,
                    evidence_ids: vec!["e1".to_string()],
                    arena_admitted: true,
                    admitted_revision: 1,
                },
                AuthorityAmbiguityRecord {
                    ambiguity_id: "a1".to_string(),
                    question_id: "q1".to_string(),
                    question: "Proceed?".to_string(),
                    affected_commitment: "build".to_string(),
                    severity: AmbiguitySeverity::Low,
                    status: AmbiguityStatus::Resolved,
                    resolver: AmbiguityResolver::Owner,
                    owner_decision_id: Some("d1".to_string()),
                    mitigation: None,
                    revisit_trigger: None,
                    evidence_ids: vec!["e1".to_string()],
                    arena_admitted: true,
                    admitted_revision: 1,
                },
            ],
            owner_required_ambiguity_ids: vec!["direction-ambiguity".to_string(), "a1".to_string()],
            reuse_decisions: vec![ReuseDecisionRecord {
                capability: "Delivery".to_string(),
                classification: ReuseClassification::Reuse,
                evidence_ids: vec!["e2".to_string()],
                candidate: "existing Delivery boundary".to_string(),
                alternatives: vec!["new Delivery subsystem".to_string()],
                rationale: "the current boundary already owns delivery authority".to_string(),
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
                competition_mode:
                    crate::pipeline_contract::ArchitectureCompetitionMode::CompetingProposals,
                synthesis: Some(ArchitectureSynthesis {
                    packet_hash: "packet-1".to_string(),
                    selection: crate::pipeline_contract::ArchitectureSelection::A,
                    reviewer_dispositions: std::collections::BTreeMap::from([(
                        "reuse".to_string(),
                        "retain existing Delivery boundary".to_string(),
                    )]),
                    risky_assumptions: vec!["candidate remains bounded".to_string()],
                    experiment_needed: false,
                    experiment_contract: None,
                    no_experiment_reason: Some("current boundary is established".to_string()),
                    reuse_decisions: vec![crate::pipeline_contract::ReuseProof {
                        capability: "Delivery".to_string(),
                        classification: "REUSE".to_string(),
                        candidate: "existing Delivery boundary".to_string(),
                        alternatives: vec!["new Delivery subsystem".to_string()],
                        evidence_ids: vec!["e2".to_string()],
                        rationale: "the current boundary already owns delivery authority"
                            .to_string(),
                    }],
                    owner_tradeoff: Some("bounded change preserves existing authority".to_string()),
                }),
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
    fn architecture_cannot_reuse_one_proposal_as_two_independent_proposals() {
        let mut records = records();
        records.architecture.proposal_b_evidence_id =
            records.architecture.proposal_a_evidence_id.clone();
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
    fn bare_narrow_build_enum_cannot_admit_a_package() {
        let mut records = records();
        records.product_direction_decision_id = None;
        records.decision_outcome = None;
        records.owner_decisions.clear();
        records.ambiguities.clear();
        records.owner_required_ambiguity_ids.clear();
        assert!(assemble_build_package(&records).is_err());
        assert!(adopt_product_direction(&mut records, "narrow_build".to_string()).is_err());

        admit_ambiguity(
            &mut records,
            "explicit-direction".to_string(),
            "explicit-direction-question".to_string(),
            "Authorize the bounded build?".to_string(),
            "product direction".to_string(),
            AmbiguitySeverity::High,
            vec!["e1".to_string()],
        )
        .expect("admit direction question");
        let decision_id = adopt_owner_decision(
            &mut records,
            "explicit-direction",
            "explicit-direction-question",
            "narrow_build".to_string(),
        )
        .expect("fresh owner direction");
        let adopted_id = adopt_product_direction(&mut records, "narrow_build".to_string())
            .expect("adopt bound product direction");
        assert_eq!(adopted_id, decision_id);
        assert_eq!(records.product_direction_decision_id, Some(decision_id));
        assert!(assemble_build_package(&records).is_ok());
    }

    #[test]
    fn material_scope_change_invalidates_adopted_direction() {
        let mut records = records();
        let scope = ProductScopeAdmission {
            objective: "Changed bounded candidate".to_string(),
            target_user: "founder".to_string(),
            requirements: vec!["one changed requirement".to_string()],
            constraints: vec!["no deployment".to_string()],
            non_goals: vec!["no redesign".to_string()],
            interfaces: vec!["existing Delivery".to_string()],
            risks: vec!["worker output is untrusted".to_string()],
            acceptance_scenarios: vec!["candidate verifier passes".to_string()],
            reviewer_restatement: ReviewerRestatement {
                intended_outcome: "Changed bounded candidate".to_string(),
                success_condition: "Verifier passes the changed candidate".to_string(),
                invented_behaviors: Vec::new(),
            },
        };
        admit_product_scope(&mut records, scope).expect("scope admission");
        assert_eq!(records.decision_outcome, None);
        assert_eq!(records.product_direction_decision_id, None);
        assert!(assemble_build_package(&records).is_err());
    }

    #[test]
    fn technical_record_cannot_satisfy_owner_required_decision() {
        let mut records = records();
        records.owner_decisions[0].authority = DecisionAuthority::Technical;
        assert!(assemble_build_package(&records).is_err());
    }

    #[test]
    fn owner_required_ambiguity_cannot_be_downgraded_in_a_payload() {
        let mut records = records();
        records.ambiguities[0].resolver = AmbiguityResolver::Technical;
        assert!(assemble_build_package(&records).is_err());
    }

    #[test]
    fn arena_ambiguity_admission_forces_owner_and_binds_decision() {
        let mut records = records();
        records.ambiguities.clear();
        records.owner_required_ambiguity_ids.clear();
        records.decision_outcome = None;
        records.product_direction_decision_id = None;
        let starting_revision = records.project_revision;
        admit_ambiguity(
            &mut records,
            "a2".to_string(),
            "q2".to_string(),
            "Which option?".to_string(),
            "build".to_string(),
            AmbiguitySeverity::High,
            vec!["e1".to_string()],
        )
        .expect("Arena admits ambiguity");
        assert_eq!(records.ambiguities[0].resolver, AmbiguityResolver::Owner);
        assert_eq!(records.project_revision, starting_revision + 1);
        assert!(
            adopt_owner_decision(&mut records, "a2", "wrong-question", "proceed".to_string(),)
                .is_err()
        );
        adopt_owner_decision(&mut records, "a2", "q2", "narrow_build".to_string())
            .expect("current owner decision");
        adopt_product_direction(&mut records, "narrow_build".to_string())
            .expect("current owner decision authorizes product direction");
        assert!(assemble_build_package(&records).is_ok());
    }

    #[test]
    fn narrow_build_without_open_ambiguity_still_identifies_next_commitment() {
        let records = records();
        let package = assemble_build_package(&records).expect("package");
        let decision = package
            .evaluate_current(&records, GateId::Ambiguity)
            .expect("ambiguity gate");
        assert_eq!(decision.status, GateStatus::Pass);
    }

    #[test]
    fn established_pattern_requires_real_proposal_and_reports_actual_count() {
        let mut records = records();
        records.architecture.competition_mode = ArchitectureCompetitionMode::EstablishedPattern;
        records.architecture.proposal_b_evidence_id.clear();
        let package =
            assemble_build_package(&records).expect("single established proposal is valid");
        let input =
            input_for_records(&records, &package, GateId::Architecture).expect("gate input");
        assert_eq!(
            input
                .architecture
                .expect("architecture proof")
                .proposal_count,
            1
        );

        records.architecture.proposal_a_evidence_id.clear();
        assert!(assemble_build_package(&records).is_err());
    }

    #[test]
    fn greenfield_direction_cannot_be_minted_without_fresh_owner_authority() {
        let mut records = records();
        records.decision_outcome = None;
        records.product_direction_decision_id = None;
        records.owner_decisions.clear();
        records.ambiguities.clear();
        records.owner_required_ambiguity_ids.clear();
        assert!(adopt_product_direction(&mut records, "narrow_build".to_string()).is_err());

        // A raw owner-looking record without an Arena-admitted owner question
        // is still insufficient.
        records.owner_decisions.push(OwnerDecisionRecord {
            decision_id: "forged-owner".to_string(),
            question_id: "forged-question".to_string(),
            selected_option: "narrow_build".to_string(),
            revision: records.project_revision.saturating_sub(1),
            status: DecisionStatus::Adopted,
            authority: DecisionAuthority::Owner,
        });
        assert!(adopt_product_direction(&mut records, "narrow_build".to_string()).is_err());
    }

    #[test]
    fn existing_change_can_use_initial_founder_mandate_without_redundant_question() {
        let mut records = records();
        records.route = ProductRoute::ExistingFeature;
        records.decision_outcome = None;
        records.product_direction_decision_id = None;
        records.owner_decisions.clear();
        assert!(adopt_product_direction(&mut records, "narrow_build".to_string()).is_ok());
    }

    #[test]
    fn failed_risk_experiment_cannot_satisfy_architecture_gate() {
        let mut records = records();
        let experiment_id = records.architecture.risk_experiment_evidence_ids[0].clone();
        let experiment = records
            .evidence
            .iter_mut()
            .find(|item| item.evidence_id == experiment_id)
            .expect("risk experiment evidence");
        experiment.kind = Some(EvidenceKind::RiskExperiment);
        experiment.verification = Some(EvidenceVerification::Contradicted);
        records
            .architecture
            .synthesis
            .as_mut()
            .expect("synthesis")
            .experiment_needed = true;
        let package = assemble_build_package(&records).expect("package remains inspectable");
        assert_ne!(
            package
                .evaluate_current(&records, GateId::Architecture)
                .expect("evaluate")
                .status,
            GateStatus::Pass
        );
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
            origin: None,
            verification: None,
            kind: None,
            source: None,
            verifier_work_order_id: None,
            contradiction_ids: Vec::new(),
            decision_impact: false,
            revisit_trigger: None,
            decision_question: None,
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

    #[test]
    fn research_proposal_requires_independent_primary_source_verification() {
        let mut records = records();
        let mut proposal = evidence("research-1", "the bounded source claim");
        proposal.origin = Some(EvidenceOrigin::AgentClaim);
        proposal.kind = Some(EvidenceKind::ResearchClaim);
        proposal.verification = None;
        proposal.source = None;
        proposal.verifier_work_order_id = None;
        submit_research_proposal(&mut records, proposal).expect("admit unverified proposal");

        let package = assemble_build_package(&records).expect("package still assembles");
        assert_eq!(
            package
                .evaluate_current(&records, GateId::ProblemResearch)
                .expect("evaluate research gate")
                .status,
            GateStatus::MissingEvidence
        );

        finalize_research_claim(
            &mut records,
            "research-1",
            "reviewer-work-order-1",
            EvidenceSource {
                reference: "github:github/github-mcp-server".to_string(),
                url: Some("https://github.com/github/github-mcp-server".to_string()),
                title: Some("GitHub MCP Server".to_string()),
                checked_at: "2026-09-18T00:00:00Z".to_string(),
                version_or_scope: "public repository metadata".to_string(),
            },
            EvidenceVerification::IndependentlyVerified,
        )
        .expect("finalize independently checked claim");
        let current_package = assemble_build_package(&records).expect("reassemble current package");
        assert_eq!(
            current_package
                .evaluate_current(&records, GateId::ProblemResearch)
                .expect("evaluate verified research gate")
                .status,
            GateStatus::Pass
        );
    }

    #[test]
    fn contradicted_research_claim_remains_evidence_but_cannot_pass() {
        let mut records = records();
        let mut proposal = evidence("research-2", "a contradicted claim");
        proposal.origin = Some(EvidenceOrigin::Web);
        proposal.kind = Some(EvidenceKind::ResearchClaim);
        proposal.verification = None;
        proposal.source = None;
        proposal.verifier_work_order_id = None;
        submit_research_proposal(&mut records, proposal).expect("admit proposal");
        finalize_research_claim(
            &mut records,
            "research-2",
            "reviewer-work-order-2",
            EvidenceSource {
                reference: "web:contradictory-primary-source".to_string(),
                url: Some("https://example.invalid/primary".to_string()),
                title: Some("Contradictory source".to_string()),
                checked_at: "2026-09-18T00:00:00Z".to_string(),
                version_or_scope: "bounded contradiction check".to_string(),
            },
            EvidenceVerification::Contradicted,
        )
        .expect("retain contradiction disposition");
        let package = assemble_build_package(&records).expect("package remains inspectable");
        assert_eq!(
            package
                .evaluate_current(&records, GateId::ProblemResearch)
                .expect("evaluate contradicted research")
                .status,
            GateStatus::MissingEvidence
        );
    }
}
