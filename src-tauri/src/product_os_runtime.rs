//! Arena-owned Product OS research admission and durable work-order boundary.
//!
//! This module deliberately composes the existing TranscriptStore and
//! SessionRuntime. It is not a second scheduler, database, or role framework.

use crate::db_helpers;
use crate::errors::AgentError;
use crate::evidence_gates::{
    AmbiguitySeverity, EvidenceItem, EvidenceKind, EvidenceOrigin, EvidenceProvenance,
    EvidenceSource, EvidenceVerification, GateDecision, GateId,
};
use crate::product_os::{
    self, AuthorityAmbiguityRecord, ProductAuthorityRecords, ProductScopeAdmission,
    ProductWorkOrder, ProductWorkOrderRole, ProductWorkOrderStatus, ReuseClassification,
    ReuseDecisionRecord,
};
use crate::session_runtime::SessionRuntime;
use crate::transcript_store::TranscriptStore;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

const MAX_SOURCE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductAuthoritySnapshot {
    pub records: ProductAuthorityRecords,
    pub work_orders: Vec<ProductWorkOrder>,
    pub research_gate: Option<GateDecision>,
}

/// A typed Arena-owned Product Director review. It is deliberately limited to
/// the evidence roles required by the Product OS handoff; researchers cannot
/// submit one and renderers have no raw record mutation command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ProductReviewAdmission {
    pub evidence_id: String,
    pub kind: EvidenceKind,
    pub claim: String,
    pub summary: String,
    pub source_reference: String,
    pub decision_impact: bool,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct ProductGateEvaluation {
    pub package: product_os::BuildPackage,
    pub decisions: Vec<GateDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ArchitectureAdmission {
    pub proposal_a_evidence_id: String,
    pub proposal_b_evidence_id: String,
    pub reuse_review_evidence_id: String,
    pub constraints_review_evidence_id: String,
    pub risk_experiment_evidence_ids: Vec<String>,
    pub red_team_evidence_id: String,
    pub dissent_evidence_id: String,
    pub unresolved_high_blocker_evidence_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct GithubObservation {
    url: String,
    title: String,
    repository: String,
    default_branch: String,
    scope: String,
}

fn db_error(error: AgentError) -> String {
    format!("Product OS persistence failed: {error}")
}

fn now() -> i64 {
    Utc::now().timestamp()
}

fn serialize_records(records: &ProductAuthorityRecords) -> Result<String, String> {
    serde_json::to_string(records)
        .map_err(|error| format!("serialize Product OS authority: {error}"))
}

fn load_records(
    store: &TranscriptStore,
    project_id: &str,
) -> Result<ProductAuthorityRecords, String> {
    let raw = store
        .get_product_authority(project_id)
        .map_err(db_error)?
        .ok_or_else(|| "Product OS project is not admitted".to_string())?;
    serde_json::from_str(&raw).map_err(|error| format!("parse Product OS authority: {error}"))
}

fn save_records(
    store: &mut TranscriptStore,
    records: &ProductAuthorityRecords,
) -> Result<(), String> {
    let raw = serialize_records(records)?;
    store
        .save_product_authority(&records.project_id, &raw, now())
        .map_err(db_error)
}

fn persist_records_and_order(
    store: &mut TranscriptStore,
    records: &ProductAuthorityRecords,
    work_order: &ProductWorkOrder,
) -> Result<(), String> {
    let records_json = serialize_records(records)?;
    store
        .save_product_authority_and_work_order(&records.project_id, &records_json, work_order)
        .map_err(db_error)
}

fn initial_records(project_id: &str, question: &str) -> ProductAuthorityRecords {
    ProductAuthorityRecords {
        project_id: project_id.to_string(),
        project_revision: 1,
        vision_version: 1,
        objective: question.to_string(),
        target_user: "founder".to_string(),
        requirements: vec!["Research the bounded question from a primary source".to_string()],
        constraints: vec!["Read-only source retrieval; no product mutation".to_string()],
        non_goals: vec!["No broad crawler or research database".to_string()],
        interfaces: vec!["Arena Product OS evidence boundary".to_string()],
        risks: vec!["Source may change and must be independently rechecked".to_string()],
        acceptance_scenarios: vec![
            "Verified source evidence remains current after reopen".to_string()
        ],
        decision_outcome: None,
        product_direction_decision_id: None,
        reviewer_restatement: None,
        evidence: Vec::new(),
        owner_decisions: Vec::new(),
        ambiguities: Vec::new(),
        owner_required_ambiguity_ids: Vec::new(),
        reuse_decisions: Vec::new(),
        architecture: product_os::ArchitectureEvidenceRecords {
            architecture_version: 1,
            proposal_a_evidence_id: String::new(),
            proposal_b_evidence_id: String::new(),
            reuse_review_evidence_id: String::new(),
            constraints_review_evidence_id: String::new(),
            risk_experiment_evidence_ids: Vec::new(),
            red_team_evidence_id: String::new(),
            dissent_evidence_id: String::new(),
            unresolved_high_blocker_evidence_ids: Vec::new(),
        },
        acceptance_profile_version: None,
    }
}

fn normalize_github_url(source_url: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(source_url.trim())
        .map_err(|_| "research source URL is invalid".to_string())?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("api.github.com")
        || !parsed.path().starts_with("/repos/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "research source must be a query-free official GitHub repository API URL".to_string(),
        );
    }
    Ok(parsed)
}

async fn retrieve_github_metadata(source_url: &str) -> Result<GithubObservation, String> {
    let url = normalize_github_url(source_url)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("consensus-arena-product-research/1")
        .build()
        .map_err(|_| "research HTTP client could not start".to_string())?;
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| "research source request failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "research source returned HTTP {}",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_SOURCE_BYTES as u64)
    {
        return Err("research source exceeded the bounded response size".to_string());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "research source response could not be read".to_string())?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err("research source exceeded the bounded response size".to_string());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "research source was not the expected GitHub metadata document".to_string())?;
    let repository = value
        .get("full_name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "research source omitted repository identity".to_string())?;
    let default_branch = value
        .get("default_branch")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "research source omitted default branch".to_string())?;
    Ok(GithubObservation {
        url: url.to_string(),
        title: format!("GitHub repository metadata: {repository}"),
        repository: repository.to_string(),
        default_branch: default_branch.to_string(),
        scope: "official GitHub repository metadata; default_branch".to_string(),
    })
}

fn new_work_order(
    project_id: &str,
    question: Option<String>,
    role: ProductWorkOrderRole,
    project_revision: u64,
    parent_work_order_id: Option<String>,
    source_ref: Option<String>,
) -> ProductWorkOrder {
    let work_order_id = uuid::Uuid::new_v4().to_string();
    let timestamp = now();
    ProductWorkOrder {
        work_order_id: work_order_id.clone(),
        project_id: project_id.to_string(),
        session_id: format!("product-os:{work_order_id}"),
        run_generation: 0,
        role,
        status: ProductWorkOrderStatus::Admitted,
        project_revision,
        parent_work_order_id,
        evidence_id: None,
        result_ref: None,
        cancellation_reason: None,
        superseded_by: None,
        question,
        source_ref,
        created_at: timestamp,
        updated_at: timestamp,
    }
}

pub async fn create_research_work_order(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    question: String,
    source_url: String,
) -> Result<ProductWorkOrder, String> {
    if project_id.trim().is_empty() || question.trim().is_empty() {
        return Err("research work order requires project identity and question".to_string());
    }
    let normalized = normalize_github_url(&source_url)?.to_string();
    let project_id_for_db = project_id.clone();
    let question_for_db = question.trim().to_string();
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut records = match store
            .get_product_authority(&project_id_for_db)
            .map_err(|error| error)?
        {
            Some(raw) => serde_json::from_str(&raw).map_err(|error| {
                AgentError::DatabaseError(format!("parse Product OS authority: {error}"))
            })?,
            None => initial_records(&project_id_for_db, &question_for_db),
        };
        if records.project_id != project_id_for_db {
            return Err(AgentError::DatabaseError(
                "Product OS project identity mismatch".to_string(),
            ));
        }
        let order = new_work_order(
            &project_id_for_db,
            Some(question_for_db.clone()),
            ProductWorkOrderRole::Researcher,
            records.project_revision,
            None,
            Some(normalized.clone()),
        );
        records.objective = records.objective.trim().to_string();
        let records_json = serde_json::to_string(&records).map_err(|error| {
            AgentError::DatabaseError(format!("serialize Product OS authority: {error}"))
        })?;
        store.save_product_authority_and_work_order(&project_id_for_db, &records_json, &order)?;
        Ok(order)
    })
    .await
    .map_err(db_error)
}

/// Create a bounded Product Director work order. Completion is allowed only
/// through the typed scope/review operations below, which verify project
/// identity and current revision before they mutate durable authority.
#[allow(dead_code)]
pub async fn create_product_director_work_order(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    subject: String,
) -> Result<ProductWorkOrder, String> {
    if project_id.trim().is_empty() || subject.trim().is_empty() {
        return Err(
            "Product Director work order requires project identity and subject".to_string(),
        );
    }
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        let order = new_work_order(
            &project_id,
            Some(subject.clone()),
            ProductWorkOrderRole::ProductDirector,
            records.project_revision,
            None,
            None,
        );
        persist_records_and_order(&mut store, &records, &order)
            .map_err(AgentError::DatabaseError)?;
        Ok(order)
    })
    .await
    .map_err(db_error)
}

#[allow(dead_code)]
fn current_admitted_director(
    store: &TranscriptStore,
    project_id: &str,
    work_order_id: &str,
    revision: u64,
) -> Result<ProductWorkOrder, AgentError> {
    let order = store
        .get_product_work_order(work_order_id)?
        .ok_or_else(|| {
            AgentError::DatabaseError("Product Director work order is unknown".to_string())
        })?;
    if order.project_id != project_id
        || order.role != ProductWorkOrderRole::ProductDirector
        || order.status != ProductWorkOrderStatus::Admitted
        || order.project_revision != revision
    {
        return Err(AgentError::DatabaseError(
            "Product Director work order is stale or not admissible".to_string(),
        ));
    }
    Ok(order)
}

#[allow(dead_code)]
fn review_evidence(
    admission: &ProductReviewAdmission,
    work_order_id: &str,
) -> Result<EvidenceItem, String> {
    if admission.evidence_id.trim().is_empty()
        || admission.claim.trim().is_empty()
        || admission.summary.trim().is_empty()
        || admission.source_reference.trim().is_empty()
    {
        return Err(
            "Product Director review requires identity, claim, summary, and source reference"
                .to_string(),
        );
    }
    if !matches!(
        admission.kind,
        EvidenceKind::ArchitectureProposal
            | EvidenceKind::ReuseReview
            | EvidenceKind::ConstraintsReview
            | EvidenceKind::RedTeamReview
            | EvidenceKind::Dissent
    ) {
        return Err("Product Director review kind is not admissible".to_string());
    }
    Ok(EvidenceItem {
        evidence_id: admission.evidence_id.clone(),
        claim: admission.claim.clone(),
        source_reference: admission.source_reference.clone(),
        captured_at: Utc::now().to_rfc3339(),
        summary: admission.summary.clone(),
        provenance: EvidenceProvenance::SourceConfirmed,
        current: true,
        origin: Some(EvidenceOrigin::Verifier),
        verification: None,
        kind: Some(admission.kind),
        source: Some(EvidenceSource {
            reference: admission.source_reference.clone(),
            url: None,
            title: Some("Arena Product Director review".to_string()),
            checked_at: Utc::now().to_rfc3339(),
            version_or_scope: format!("Arena-owned review work order {work_order_id}"),
        }),
        verifier_work_order_id: Some(work_order_id.to_string()),
        contradiction_ids: Vec::new(),
        decision_impact: admission.decision_impact,
        revisit_trigger: Some("revisit if the bounded product scope or source changes".to_string()),
    })
}

#[allow(dead_code)]
pub async fn admit_product_scope_from_review(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    work_order_id: String,
    scope: ProductScopeAdmission,
) -> Result<ProductAuthorityRecords, String> {
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        let mut order = current_admitted_director(
            &store,
            &project_id,
            &work_order_id,
            records.project_revision,
        )?;
        product_os::admit_product_scope(&mut records, scope.clone())
            .map_err(AgentError::DatabaseError)?;
        order.status = ProductWorkOrderStatus::Completed;
        order.result_ref = Some("bounded product scope admitted".to_string());
        order.updated_at = now();
        persist_records_and_order(&mut store, &records, &order)
            .map_err(AgentError::DatabaseError)?;
        Ok(records)
    })
    .await
    .map_err(db_error)
}

/// Admit one completed, typed Product Director review. It cannot create
/// research claims, gate inputs, or a product direction.
#[allow(dead_code)]
pub async fn admit_product_review(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    work_order_id: String,
    admission: ProductReviewAdmission,
) -> Result<ProductWorkOrder, String> {
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        let mut order = current_admitted_director(
            &store,
            &project_id,
            &work_order_id,
            records.project_revision,
        )?;
        if records
            .evidence
            .iter()
            .any(|item| item.current && item.evidence_id == admission.evidence_id)
        {
            return Err(AgentError::DatabaseError(
                "current Product Director evidence identity already exists".to_string(),
            ));
        }
        let evidence =
            review_evidence(&admission, &order.work_order_id).map_err(AgentError::DatabaseError)?;
        records.evidence.push(evidence);
        records.project_revision = records.project_revision.saturating_add(1);
        order.status = ProductWorkOrderStatus::Completed;
        order.evidence_id = Some(admission.evidence_id.clone());
        order.result_ref = Some("typed Product Director review admitted".to_string());
        order.updated_at = now();
        persist_records_and_order(&mut store, &records, &order)
            .map_err(AgentError::DatabaseError)?;
        Ok(order)
    })
    .await
    .map_err(db_error)
}

/// Perform a bounded, real primary-source risk experiment under the existing
/// SessionRuntime lease. It records an observation, not a caller-supplied
/// success boolean.
#[allow(dead_code)]
pub async fn run_product_github_risk_spike(
    db: Arc<Mutex<TranscriptStore>>,
    runtime: Arc<SessionRuntime>,
    work_order_id: String,
    source_url: String,
) -> Result<ProductWorkOrder, String> {
    let normalized_source = normalize_github_url(&source_url)?.to_string();
    let source_for_preflight = normalized_source.clone();
    let preflight = {
        let db = db.clone();
        let id = work_order_id.clone();
        db_helpers::run_blocking(move || {
            let mut store = db.lock().map_err(|_| {
                AgentError::DatabaseError("transcript store lock poisoned".to_string())
            })?;
            let records = load_records(
                &store,
                &store
                    .get_product_work_order(&id)?
                    .ok_or_else(|| {
                        AgentError::DatabaseError("risk-spike work order is unknown".to_string())
                    })?
                    .project_id,
            )
            .map_err(AgentError::DatabaseError)?;
            let mut order = current_admitted_director(
                &store,
                &records.project_id,
                &id,
                records.project_revision,
            )?;
            order.source_ref = Some(source_for_preflight.clone());
            order.status = ProductWorkOrderStatus::Running;
            order.updated_at = now();
            store.save_product_work_order(&order)?;
            Ok(order)
        })
        .await
        .map_err(db_error)?
    };
    let db_for_task = db.clone();
    let id_for_task = work_order_id.clone();
    execute_owned(runtime, work_order_id, move |generation| {
        let db = db_for_task.clone();
        async move {
            let mut running = preflight;
            running.run_generation = generation;
            let id = id_for_task.clone();
            db_helpers::run_blocking({
                let db = db.clone();
                let running = running.clone();
                move || {
                    let mut store = db.lock().map_err(|_| {
                        AgentError::DatabaseError("transcript store lock poisoned".to_string())
                    })?;
                    store.save_product_work_order(&running)
                }
            })
            .await
            .map_err(db_error)?;
            let observation = match retrieve_github_metadata(&normalized_source).await {
                Ok(value) => value,
                Err(error) => {
                    mark_failed(db.clone(), &id, generation, &error).await;
                    return Err(error);
                }
            };
            let db_for_finalize = db.clone();
            let id_for_finalize = id.clone();
            let finalized = db_helpers::run_blocking(move || {
                let mut store = db_for_finalize.lock().map_err(|_| {
                    AgentError::DatabaseError("transcript store lock poisoned".to_string())
                })?;
                let mut order = store.get_product_work_order(&id_for_finalize)?.ok_or_else(|| {
                    AgentError::DatabaseError("risk-spike work order disappeared".to_string())
                })?;
                if order.status != ProductWorkOrderStatus::Running || order.run_generation != generation {
                    return Err(AgentError::DatabaseError("risk-spike result is stale or cancelled".to_string()));
                }
                let mut records = load_records(&store, &order.project_id)
                    .map_err(AgentError::DatabaseError)?;
                let evidence_id = format!("{}:risk-spike", order.work_order_id);
                records.evidence.push(EvidenceItem {
                    evidence_id: evidence_id.clone(),
                    claim: format!(
                        "Bounded official GitHub API retrieval returned repository {} with default branch {}.",
                        observation.repository, observation.default_branch
                    ),
                    source_reference: observation.url.clone(),
                    captured_at: Utc::now().to_rfc3339(),
                    summary: "Risk experiment confirmed bounded read-only metadata retrieval and response parsing.".to_string(),
                    provenance: EvidenceProvenance::RuntimeProven,
                    current: true,
                    origin: Some(EvidenceOrigin::GitHub),
                    verification: Some(EvidenceVerification::IndependentlyVerified),
                    kind: Some(EvidenceKind::RiskExperiment),
                    source: Some(EvidenceSource {
                        reference: observation.url.clone(),
                        url: Some(observation.url.clone()),
                        title: Some(observation.title.clone()),
                        checked_at: Utc::now().to_rfc3339(),
                        version_or_scope: observation.scope.clone(),
                    }),
                    verifier_work_order_id: Some(order.work_order_id.clone()),
                    contradiction_ids: Vec::new(),
                    decision_impact: true,
                    revisit_trigger: Some("re-run if GitHub endpoint behavior changes".to_string()),
                });
                records.project_revision = records.project_revision.saturating_add(1);
                order.status = ProductWorkOrderStatus::Completed;
                order.evidence_id = Some(evidence_id);
                order.result_ref = Some(observation.url.clone());
                order.updated_at = now();
                persist_records_and_order(&mut store, &records, &order)
                    .map_err(AgentError::DatabaseError)?;
                Ok(order)
            })
            .await
            .map_err(db_error);
            match finalized {
                Ok(order) => Ok(order),
                Err(error) => {
                    mark_failed(db.clone(), &id, generation, &error).await;
                    Err(error)
                }
            }
        }
    })
    .await
}

#[allow(dead_code)]
fn completed_director_evidence(
    store: &TranscriptStore,
    project_id: &str,
    evidence_id: &str,
    expected_kind: EvidenceKind,
    records: &ProductAuthorityRecords,
) -> Result<ProductWorkOrder, AgentError> {
    let evidence = records
        .evidence
        .iter()
        .find(|item| item.evidence_id == evidence_id && item.current)
        .ok_or_else(|| {
            AgentError::DatabaseError("review evidence is unknown or stale".to_string())
        })?;
    if evidence.kind != Some(expected_kind) {
        return Err(AgentError::DatabaseError(
            "review evidence has the wrong semantic role".to_string(),
        ));
    }
    store
        .list_product_work_orders(project_id)?
        .into_iter()
        .find(|order| {
            order.role == ProductWorkOrderRole::ProductDirector
                && order.status == ProductWorkOrderStatus::Completed
                && order.evidence_id.as_deref() == Some(evidence_id)
        })
        .ok_or_else(|| {
            AgentError::DatabaseError(
                "review evidence is not bound to a completed Product Director work order"
                    .to_string(),
            )
        })
}

#[allow(dead_code)]
pub async fn adopt_product_reuse_decision(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    work_order_id: String,
    capability: String,
    classification: ReuseClassification,
    evidence_ids: Vec<String>,
) -> Result<ProductAuthorityRecords, String> {
    if capability.trim().is_empty() {
        return Err("reuse decision requires a capability".to_string());
    }
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        let order = store
            .get_product_work_order(&work_order_id)?
            .ok_or_else(|| {
                AgentError::DatabaseError("reuse reviewer work order is unknown".to_string())
            })?;
        if order.project_id != project_id
            || order.role != ProductWorkOrderRole::ProductDirector
            || order.status != ProductWorkOrderStatus::Completed
        {
            return Err(AgentError::DatabaseError(
                "reuse decision is not bound to a completed Product Director review".to_string(),
            ));
        }
        let review_id = order.evidence_id.as_deref().ok_or_else(|| {
            AgentError::DatabaseError("reuse reviewer has no admitted evidence".to_string())
        })?;
        completed_director_evidence(
            &store,
            &project_id,
            review_id,
            EvidenceKind::ReuseReview,
            &records,
        )?;
        if classification == ReuseClassification::Build && evidence_ids.is_empty() {
            return Err(AgentError::DatabaseError(
                "BUILD reuse classification requires alternative evidence".to_string(),
            ));
        }
        for evidence_id in &evidence_ids {
            let current = records
                .evidence
                .iter()
                .any(|item| item.current && item.evidence_id == *evidence_id);
            if !current {
                return Err(AgentError::DatabaseError(
                    "reuse decision references stale or unknown evidence".to_string(),
                ));
            }
        }
        records
            .reuse_decisions
            .retain(|item| item.capability != capability);
        records.reuse_decisions.push(ReuseDecisionRecord {
            capability: capability.clone(),
            classification: classification.clone(),
            evidence_ids: evidence_ids.clone(),
        });
        records.project_revision = records.project_revision.saturating_add(1);
        save_records(&mut store, &records).map_err(AgentError::DatabaseError)?;
        Ok(records)
    })
    .await
    .map_err(db_error)
}

#[allow(dead_code)]
pub async fn adopt_product_architecture(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    admission: ArchitectureAdmission,
) -> Result<ProductAuthorityRecords, String> {
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        if admission.proposal_a_evidence_id == admission.proposal_b_evidence_id {
            return Err(AgentError::DatabaseError(
                "architecture admission requires distinct proposal evidence".to_string(),
            ));
        }
        let proposal_a = completed_director_evidence(
            &store,
            &project_id,
            &admission.proposal_a_evidence_id,
            EvidenceKind::ArchitectureProposal,
            &records,
        )?;
        let proposal_b = completed_director_evidence(
            &store,
            &project_id,
            &admission.proposal_b_evidence_id,
            EvidenceKind::ArchitectureProposal,
            &records,
        )?;
        if proposal_a.work_order_id == proposal_b.work_order_id {
            return Err(AgentError::DatabaseError(
                "architecture proposals must originate from separate Product Director work orders"
                    .to_string(),
            ));
        }
        completed_director_evidence(
            &store,
            &project_id,
            &admission.reuse_review_evidence_id,
            EvidenceKind::ReuseReview,
            &records,
        )?;
        completed_director_evidence(
            &store,
            &project_id,
            &admission.constraints_review_evidence_id,
            EvidenceKind::ConstraintsReview,
            &records,
        )?;
        completed_director_evidence(
            &store,
            &project_id,
            &admission.red_team_evidence_id,
            EvidenceKind::RedTeamReview,
            &records,
        )?;
        completed_director_evidence(
            &store,
            &project_id,
            &admission.dissent_evidence_id,
            EvidenceKind::Dissent,
            &records,
        )?;
        if admission.risk_experiment_evidence_ids.is_empty() {
            return Err(AgentError::DatabaseError(
                "architecture admission requires a risk experiment".to_string(),
            ));
        }
        for evidence_id in &admission.risk_experiment_evidence_ids {
            completed_director_evidence(
                &store,
                &project_id,
                evidence_id,
                EvidenceKind::RiskExperiment,
                &records,
            )?;
        }
        for evidence_id in &admission.unresolved_high_blocker_evidence_ids {
            if !records
                .evidence
                .iter()
                .any(|item| item.current && item.evidence_id == *evidence_id)
            {
                return Err(AgentError::DatabaseError(
                    "architecture blocker reference is stale or unknown".to_string(),
                ));
            }
        }
        records.architecture = product_os::ArchitectureEvidenceRecords {
            architecture_version: records.architecture.architecture_version.saturating_add(1),
            proposal_a_evidence_id: admission.proposal_a_evidence_id.clone(),
            proposal_b_evidence_id: admission.proposal_b_evidence_id.clone(),
            reuse_review_evidence_id: admission.reuse_review_evidence_id.clone(),
            constraints_review_evidence_id: admission.constraints_review_evidence_id.clone(),
            risk_experiment_evidence_ids: admission.risk_experiment_evidence_ids.clone(),
            red_team_evidence_id: admission.red_team_evidence_id.clone(),
            dissent_evidence_id: admission.dissent_evidence_id.clone(),
            unresolved_high_blocker_evidence_ids: admission
                .unresolved_high_blocker_evidence_ids
                .clone(),
        };
        records.project_revision = records.project_revision.saturating_add(1);
        save_records(&mut store, &records).map_err(AgentError::DatabaseError)?;
        Ok(records)
    })
    .await
    .map_err(db_error)
}

#[allow(dead_code)]
pub async fn adopt_narrow_build_direction(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
) -> Result<ProductAuthorityRecords, String> {
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        product_os::adopt_product_direction(&mut records, "narrow_build".to_string())
            .map_err(AgentError::DatabaseError)?;
        save_records(&mut store, &records).map_err(AgentError::DatabaseError)?;
        Ok(records)
    })
    .await
    .map_err(db_error)
}

#[allow(dead_code)]
pub async fn assemble_current_build_package(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
) -> Result<product_os::BuildPackage, String> {
    db_helpers::run_blocking(move || {
        let store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        product_os::assemble_build_package(&records).map_err(AgentError::DatabaseError)
    })
    .await
    .map_err(db_error)
}

#[allow(dead_code)]
pub async fn evaluate_current_preimplementation_gates(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
) -> Result<ProductGateEvaluation, String> {
    db_helpers::run_blocking(move || {
        let store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        let package =
            product_os::assemble_build_package(&records).map_err(AgentError::DatabaseError)?;
        let gate_ids = [
            GateId::Vision,
            GateId::ProblemResearch,
            GateId::Positioning,
            GateId::Ambiguity,
            GateId::Reuse,
            GateId::Architecture,
            GateId::BuildReadiness,
        ];
        let decisions = gate_ids
            .into_iter()
            .map(|gate_id| {
                package
                    .evaluate_current(&records, gate_id)
                    .map_err(AgentError::DatabaseError)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ProductGateEvaluation { package, decisions })
    })
    .await
    .map_err(db_error)
}

async fn execute_owned<T, F, Fut>(
    runtime: Arc<SessionRuntime>,
    work_order_id: String,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(u64) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, String>> + Send + 'static,
{
    let permit = runtime
        .try_acquire_start(work_order_id)
        .map_err(|error| error.to_string())?;
    let owner = permit.owner();
    let task_owner = owner.clone();
    let (result_tx, result_rx) = oneshot::channel();
    let (activate_tx, activate_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        if activate_rx.await.is_err() {
            return;
        }
        let result = operation(task_owner.run_generation).await;
        let _ = result_tx.send(result);
    });
    permit.commit(handle, activate_tx)?;
    let result = result_rx
        .await
        .map_err(|_| "Product OS work order stopped before returning a result".to_string())?;
    if !runtime.mark_completed(&owner) {
        return Err("Product OS work order lost its current runtime ownership".to_string());
    }
    result
}

async fn mark_failed(
    db: Arc<Mutex<TranscriptStore>>,
    work_order_id: &str,
    generation: u64,
    message: &str,
) {
    let id = work_order_id.to_string();
    let safe_message = message.chars().take(240).collect::<String>();
    let _ = db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        if let Some(mut order) = store.get_product_work_order(&id)? {
            if order.run_generation == generation && order.status == ProductWorkOrderStatus::Running
            {
                order.status = ProductWorkOrderStatus::Failed;
                order.result_ref = Some(safe_message.clone());
                order.updated_at = now();
                store.save_product_work_order(&order)?;
            }
        }
        Ok(())
    })
    .await;
}

pub async fn run_research_work_order(
    db: Arc<Mutex<TranscriptStore>>,
    runtime: Arc<SessionRuntime>,
    work_order_id: String,
    source_url: String,
) -> Result<ProductWorkOrder, String> {
    let normalized_source = normalize_github_url(&source_url)?.to_string();
    let preflight = {
        let db = db.clone();
        let id = work_order_id.clone();
        db_helpers::run_blocking(move || {
            let store = db.lock().map_err(|_| {
                AgentError::DatabaseError("transcript store lock poisoned".to_string())
            })?;
            let mut order = store.get_product_work_order(&id)?.ok_or_else(|| {
                AgentError::DatabaseError("research work order is unknown".to_string())
            })?;
            if order.role != ProductWorkOrderRole::Researcher
                || !matches!(
                    order.status,
                    ProductWorkOrderStatus::Admitted
                        | ProductWorkOrderStatus::ReconciliationRequired
                )
            {
                return Err(AgentError::DatabaseError(
                    "research work order is not current and admissible".to_string(),
                ));
            }
            if order.source_ref.as_deref() != Some(normalized_source.as_str()) {
                return Err(AgentError::DatabaseError(
                    "research source does not match the admitted work order".to_string(),
                ));
            }
            order.status = ProductWorkOrderStatus::Running;
            order.updated_at = now();
            Ok(order)
        })
        .await
        .map_err(db_error)?
    };
    let db_for_task = db.clone();
    let source_url_for_task = source_url.clone();
    let id_for_task = work_order_id.clone();
    let result = execute_owned(runtime, work_order_id.clone(), move |generation| {
        let db = db_for_task.clone();
        async move {
            let mut running = preflight;
            running.run_generation = generation;
            let id = id_for_task.clone();
            db_helpers::run_blocking({
                let db = db.clone();
                let running = running.clone();
                move || {
                    let mut store = db.lock().map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
                    store.save_product_work_order(&running)
                }
            }).await.map_err(db_error)?;
            let observation = match retrieve_github_metadata(&source_url_for_task).await {
                Ok(value) => value,
                Err(error) => {
                    mark_failed(db.clone(), &id, generation, &error).await;
                    return Err(error);
                }
            };
            let finalized = db_helpers::run_blocking({
                let db = db.clone();
                let id_for_db = id.clone();
                let observation = observation.clone();
                move || {
                    let mut store = db.lock().map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
                    let mut order = store.get_product_work_order(&id_for_db)?.ok_or_else(|| AgentError::DatabaseError("research work order disappeared".to_string()))?;
                    if order.status != ProductWorkOrderStatus::Running || order.run_generation != generation {
                        return Err(AgentError::DatabaseError("research result is stale or cancelled".to_string()));
                    }
                    let mut records = load_records(&store, &order.project_id).map_err(AgentError::DatabaseError)?;
                    let evidence_id = format!("{}:evidence", order.work_order_id);
                    let question = order.question.clone().unwrap_or_else(|| "bounded source question".to_string());
                    let claim = format!("{question} Official primary-source metadata for {} reports default branch {}.", observation.repository, observation.default_branch);
                    let proposal = EvidenceItem {
                        evidence_id: evidence_id.clone(),
                        claim,
                        source_reference: observation.url.clone(),
                        captured_at: Utc::now().to_rfc3339(),
                        summary: format!("Retrieved {} from the official GitHub API.", observation.repository),
                        provenance: EvidenceProvenance::RuntimeProven,
                        current: true,
                        origin: Some(EvidenceOrigin::GitHub),
                        verification: Some(EvidenceVerification::Unverified),
                        kind: Some(EvidenceKind::ResearchClaim),
                        source: None,
                        verifier_work_order_id: None,
                        contradiction_ids: Vec::new(),
                        decision_impact: true,
                        revisit_trigger: Some("recheck when repository metadata changes".to_string()),
                    };
                    product_os::submit_research_proposal(&mut records, proposal).map_err(AgentError::DatabaseError)?;
                    order.status = ProductWorkOrderStatus::Completed;
                    order.evidence_id = Some(evidence_id);
                    order.result_ref = Some(observation.url.clone());
                    order.updated_at = now();
                    persist_records_and_order(&mut store, &records, &order).map_err(AgentError::DatabaseError)?;
                    Ok(order)
                }
            }).await.map_err(db_error);
            let finalized = match finalized {
                Ok(order) => order,
                Err(error) => {
                    mark_failed(db.clone(), &id, generation, &error).await;
                    return Err(error);
                }
            };
            Ok(finalized)
        }
    }).await;
    result
}

pub async fn create_fact_verifier_work_order(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    evidence_id: String,
    source_url: String,
) -> Result<ProductWorkOrder, String> {
    let normalized = normalize_github_url(&source_url)?.to_string();
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        let evidence = records
            .evidence
            .iter()
            .find(|item| item.evidence_id == evidence_id && item.current)
            .ok_or_else(|| {
                AgentError::DatabaseError("research evidence is unknown or stale".to_string())
            })?;
        if evidence.kind != Some(EvidenceKind::ResearchClaim)
            || evidence.verification != Some(EvidenceVerification::Unverified)
        {
            return Err(AgentError::DatabaseError(
                "only current unverified research can be assigned to a verifier".to_string(),
            ));
        }
        let researcher = store
            .list_product_work_orders(&project_id)?
            .into_iter()
            .find(|order| {
                order.role == ProductWorkOrderRole::Researcher
                    && order.evidence_id.as_deref() == Some(evidence_id.as_str())
                    && order.status == ProductWorkOrderStatus::Completed
            })
            .ok_or_else(|| {
                AgentError::DatabaseError(
                    "originating researcher work order is missing".to_string(),
                )
            })?;
        let order = new_work_order(
            &project_id,
            Some("Independently verify the current research claim".to_string()),
            ProductWorkOrderRole::FactVerifier,
            records.project_revision,
            Some(researcher.work_order_id),
            Some(normalized.clone()),
        );
        let json = serde_json::to_string(&records).map_err(|error| {
            AgentError::DatabaseError(format!("serialize Product OS authority: {error}"))
        })?;
        store.save_product_authority_and_work_order(&project_id, &json, &order)?;
        Ok(order)
    })
    .await
    .map_err(db_error)
}

pub async fn run_fact_verifier_work_order(
    db: Arc<Mutex<TranscriptStore>>,
    runtime: Arc<SessionRuntime>,
    work_order_id: String,
    source_url: String,
) -> Result<ProductWorkOrder, String> {
    let normalized_source = normalize_github_url(&source_url)?.to_string();
    let db_for_preflight = db.clone();
    let id_for_preflight = work_order_id.clone();
    let preflight = db_helpers::run_blocking(move || {
        let mut store = db_for_preflight
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut order = store
            .get_product_work_order(&id_for_preflight)?
            .ok_or_else(|| {
                AgentError::DatabaseError("verifier work order is unknown".to_string())
            })?;
        if order.role != ProductWorkOrderRole::FactVerifier
            || !matches!(
                order.status,
                ProductWorkOrderStatus::Admitted | ProductWorkOrderStatus::ReconciliationRequired
            )
        {
            return Err(AgentError::DatabaseError(
                "verifier work order is not current and admissible".to_string(),
            ));
        }
        if order.source_ref.as_deref() != Some(normalized_source.as_str()) {
            return Err(AgentError::DatabaseError(
                "verification source does not match the admitted work order".to_string(),
            ));
        }
        order.status = ProductWorkOrderStatus::Running;
        order.updated_at = now();
        store.save_product_work_order(&order)?;
        Ok(order)
    })
    .await
    .map_err(db_error)?;
    let db_for_task = db.clone();
    let id_for_task = work_order_id.clone();
    let result = execute_owned(runtime, work_order_id, move |generation| {
        let db = db_for_task.clone();
        async move {
            let mut running = preflight;
            running.run_generation = generation;
            let id = id_for_task.clone();
            db_helpers::run_blocking({
                let db = db.clone();
                let running = running.clone();
                move || {
                    let mut store = db.lock().map_err(|_| {
                        AgentError::DatabaseError("transcript store lock poisoned".to_string())
                    })?;
                    store.save_product_work_order(&running)
                }
            })
            .await
            .map_err(db_error)?;
            let observation = match retrieve_github_metadata(&source_url).await {
                Ok(value) => value,
                Err(error) => {
                    mark_failed(db.clone(), &id, generation, &error).await;
                    return Err(error);
                }
            };
            let finalized = db_helpers::run_blocking({
                let db = db.clone();
                let id_for_db = id.clone();
                let observation = observation.clone();
                move || {
                    let mut store = db.lock().map_err(|_| {
                        AgentError::DatabaseError("transcript store lock poisoned".to_string())
                    })?;
                    let mut order = store.get_product_work_order(&id_for_db)?.ok_or_else(|| {
                        AgentError::DatabaseError("verifier work order disappeared".to_string())
                    })?;
                    if order.status != ProductWorkOrderStatus::Running
                        || order.run_generation != generation
                    {
                        return Err(AgentError::DatabaseError(
                            "verifier result is stale or cancelled".to_string(),
                        ));
                    }
                    let mut records = load_records(&store, &order.project_id)
                        .map_err(AgentError::DatabaseError)?;
                    let evidence_id = order
                        .parent_work_order_id
                        .as_deref()
                        .and_then(|parent_id| {
                            store.get_product_work_order(parent_id).ok().flatten()
                        })
                        .and_then(|parent| parent.evidence_id)
                        .ok_or_else(|| {
                            AgentError::DatabaseError(
                                "verifier parent evidence is missing".to_string(),
                            )
                        })?;
                    let evidence = records
                        .evidence
                        .iter()
                        .find(|item| item.evidence_id == evidence_id && item.current)
                        .ok_or_else(|| {
                            AgentError::DatabaseError("verifier evidence is stale".to_string())
                        })?;
                    if evidence.verification != Some(EvidenceVerification::Unverified)
                        || order.project_revision != records.project_revision
                    {
                        return Err(AgentError::DatabaseError(
                            "verifier evidence or authority revision is stale".to_string(),
                        ));
                    }
                    let source = EvidenceSource {
                        reference: observation.url.clone(),
                        url: Some(observation.url.clone()),
                        title: Some(observation.title.clone()),
                        checked_at: Utc::now().to_rfc3339(),
                        version_or_scope: observation.scope.clone(),
                    };
                    product_os::finalize_research_claim(
                        &mut records,
                        &evidence_id,
                        &order.work_order_id,
                        source,
                        EvidenceVerification::IndependentlyVerified,
                    )
                    .map_err(AgentError::DatabaseError)?;
                    records.project_revision = records.project_revision.saturating_add(1);
                    order.status = ProductWorkOrderStatus::Completed;
                    order.evidence_id = Some(evidence_id);
                    order.result_ref = Some(source_url.clone());
                    order.updated_at = now();
                    persist_records_and_order(&mut store, &records, &order)
                        .map_err(AgentError::DatabaseError)?;
                    Ok(order)
                }
            })
            .await
            .map_err(db_error);
            let finalized = match finalized {
                Ok(order) => order,
                Err(error) => {
                    mark_failed(db.clone(), &id, generation, &error).await;
                    return Err(error);
                }
            };
            Ok(finalized)
        }
    })
    .await;
    result
}

pub async fn cancel_product_work_order(
    db: Arc<Mutex<TranscriptStore>>,
    runtime: Arc<SessionRuntime>,
    work_order_id: String,
    reason: String,
) -> Result<ProductWorkOrder, String> {
    if let Some(owner) = runtime.current_owner() {
        if owner.session_id == work_order_id {
            if let Some(guard) = runtime.stop_owner(&owner).await? {
                guard.finish();
            }
        }
    }
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut order = store
            .get_product_work_order(&work_order_id)?
            .ok_or_else(|| {
                AgentError::DatabaseError("Product OS work order is unknown".to_string())
            })?;
        if matches!(
            order.status,
            ProductWorkOrderStatus::Completed | ProductWorkOrderStatus::Cancelled
        ) {
            return Err(AgentError::DatabaseError(
                "terminal Product OS work order cannot be cancelled".to_string(),
            ));
        }
        order.status = ProductWorkOrderStatus::Cancelled;
        order.cancellation_reason = Some(reason.chars().take(240).collect());
        order.updated_at = now();
        store.save_product_work_order(&order)?;
        Ok(order)
    })
    .await
    .map_err(db_error)
}

pub async fn reconcile_after_restart(
    db: Arc<Mutex<TranscriptStore>>,
    runtime: Arc<SessionRuntime>,
) -> Result<(), String> {
    let active = runtime.current_owner().map(|owner| owner.session_id);
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let project_ids = store.list_product_projects()?;
        for project_id in project_ids {
            for mut order in store.list_product_work_orders(&project_id)? {
                if order.status == ProductWorkOrderStatus::Running
                    && active.as_deref() != Some(order.work_order_id.as_str())
                {
                    order.status = ProductWorkOrderStatus::ReconciliationRequired;
                    order.cancellation_reason =
                        Some("runtime was not active after Arena reopen".to_string());
                    order.updated_at = now();
                    store.save_product_work_order(&order)?;
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(db_error)
}

pub async fn snapshot(
    db: Arc<Mutex<TranscriptStore>>,
    runtime: Arc<SessionRuntime>,
    project_id: String,
) -> Result<Option<ProductAuthoritySnapshot>, String> {
    reconcile_after_restart(db.clone(), runtime).await?;
    db_helpers::run_blocking(move || {
        let store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let Some(records) = store.get_product_authority(&project_id)? else {
            return Ok(None);
        };
        let records = serde_json::from_str(&records).map_err(|error| {
            AgentError::DatabaseError(format!("parse Product OS authority: {error}"))
        })?;
        let work_orders = store.list_product_work_orders(&project_id)?;
        let research_gate = product_os::assemble_build_package(&records)
            .ok()
            .and_then(|package| {
                package
                    .evaluate_current(&records, GateId::ProblemResearch)
                    .ok()
            });
        Ok(Some(ProductAuthoritySnapshot {
            records,
            work_orders,
            research_gate,
        }))
    })
    .await
    .map_err(db_error)
}

pub async fn latest_snapshot(
    db: Arc<Mutex<TranscriptStore>>,
    runtime: Arc<SessionRuntime>,
) -> Result<Option<ProductAuthoritySnapshot>, String> {
    let project_id = db_helpers::run_blocking({
        let db = db.clone();
        move || {
            let store = db.lock().map_err(|_| {
                AgentError::DatabaseError("transcript store lock poisoned".to_string())
            })?;
            store.get_latest_product_project()
        }
    })
    .await
    .map_err(db_error)?;
    match project_id {
        Some(project_id) => snapshot(db, runtime, project_id).await,
        None => Ok(None),
    }
}

pub async fn admit_ambiguity(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    work_order_id: String,
    ambiguity_id: String,
    question_id: String,
    question: String,
    affected_commitment: String,
    severity: AmbiguitySeverity,
    evidence_ids: Vec<String>,
) -> Result<AuthorityAmbiguityRecord, String> {
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let order = store
            .get_product_work_order(&work_order_id)?
            .ok_or_else(|| {
                AgentError::DatabaseError("ambiguity proposer work order is unknown".to_string())
            })?;
        if order.project_id != project_id
            || !matches!(
                order.role,
                ProductWorkOrderRole::Researcher | ProductWorkOrderRole::ProductDirector
            )
            || order.status != ProductWorkOrderStatus::Completed
        {
            return Err(AgentError::DatabaseError(
                "ambiguity proposer is not a current Arena work order".to_string(),
            ));
        }
        let mut records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        let proposal_is_current = order.project_revision == records.project_revision
            || (order.evidence_id.as_ref().is_some_and(|evidence_id| {
                records
                    .evidence
                    .iter()
                    .any(|item| item.current && item.evidence_id == *evidence_id)
            }) && records.project_revision == order.project_revision.saturating_add(1));
        if !proposal_is_current {
            return Err(AgentError::DatabaseError(
                "ambiguity proposal is stale for the current authority revision".to_string(),
            ));
        }
        product_os::admit_ambiguity(
            &mut records,
            ambiguity_id.clone(),
            question_id.clone(),
            question.clone(),
            affected_commitment.clone(),
            severity,
            evidence_ids.clone(),
        )
        .map_err(AgentError::DatabaseError)?;
        let admitted = records.ambiguities.last().cloned().ok_or_else(|| {
            AgentError::DatabaseError("ambiguity admission produced no record".to_string())
        })?;
        save_records(&mut store, &records).map_err(AgentError::DatabaseError)?;
        Ok(admitted)
    })
    .await
    .map_err(db_error)
}

pub async fn adopt_owner_decision(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    ambiguity_id: String,
    question_id: String,
    selected_option: String,
) -> Result<ProductAuthorityRecords, String> {
    db_helpers::run_blocking(move || {
        let mut store = db
            .lock()
            .map_err(|_| AgentError::DatabaseError("transcript store lock poisoned".to_string()))?;
        let mut records = load_records(&store, &project_id).map_err(AgentError::DatabaseError)?;
        product_os::adopt_owner_decision(
            &mut records,
            &ambiguity_id,
            &question_id,
            selected_option.clone(),
        )
        .map_err(AgentError::DatabaseError)?;
        save_records(&mut store, &records).map_err(AgentError::DatabaseError)?;
        Ok(records)
    })
    .await
    .map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn official_github_metadata_is_retrieved_with_bounded_source_path() {
        let observation =
            retrieve_github_metadata("https://api.github.com/repos/github/github-mcp-server")
                .await
                .expect("official GitHub metadata should be reachable");
        assert_eq!(observation.repository, "github/github-mcp-server");
        assert!(!observation.default_branch.is_empty());
    }

    #[tokio::test]
    async fn real_research_and_verification_survive_store_reopen() {
        let path = std::env::temp_dir().join(format!("arena-m05b-{}.db", uuid::Uuid::new_v4()));
        let store = TranscriptStore::open(path.to_string_lossy().as_ref())
            .expect("temporary Product OS store should open");
        let db = Arc::new(Mutex::new(store));
        let runtime = Arc::new(SessionRuntime::new());
        let source = "https://api.github.com/repos/github/github-mcp-server";
        let researcher = create_research_work_order(
            db.clone(),
            "m05b-real-dogfood".to_string(),
            "What default branch does the official GitHub MCP repository currently report?"
                .to_string(),
            source.to_string(),
        )
        .await
        .expect("research work order admission");
        let researcher_id = researcher.work_order_id.clone();
        let completed_research = run_research_work_order(
            db.clone(),
            runtime.clone(),
            researcher.work_order_id,
            source.to_string(),
        )
        .await
        .expect("real research execution");
        let initial_snapshot =
            snapshot(db.clone(), runtime.clone(), "m05b-real-dogfood".to_string())
                .await
                .expect("research snapshot")
                .expect("research project");
        let evidence_id = completed_research.evidence_id.expect("research evidence");
        let evidence = initial_snapshot
            .records
            .evidence
            .iter()
            .find(|item| item.evidence_id == evidence_id)
            .expect("persisted proposal");
        assert_eq!(
            evidence.verification,
            Some(EvidenceVerification::Unverified)
        );
        assert!(initial_snapshot.research_gate.is_none());

        let ambiguity = admit_ambiguity(
            db.clone(),
            "m05b-real-dogfood".to_string(),
            researcher_id,
            "a-real".to_string(),
            "q-real".to_string(),
            "Should this bounded evidence inform the next build decision?".to_string(),
            "next-build".to_string(),
            AmbiguitySeverity::Medium,
            vec![evidence_id.clone()],
        )
        .await
        .expect("Arena ambiguity admission");
        assert_eq!(
            ambiguity.resolver,
            crate::product_os::AmbiguityResolver::Owner
        );
        assert!(adopt_owner_decision(
            db.clone(),
            "m05b-real-dogfood".to_string(),
            "a-real".to_string(),
            "wrong-question".to_string(),
            "proceed".to_string(),
        )
        .await
        .is_err());
        adopt_owner_decision(
            db.clone(),
            "m05b-real-dogfood".to_string(),
            "a-real".to_string(),
            "q-real".to_string(),
            "proceed".to_string(),
        )
        .await
        .expect("current owner decision");

        let verifier = create_fact_verifier_work_order(
            db.clone(),
            "m05b-real-dogfood".to_string(),
            evidence_id.clone(),
            source.to_string(),
        )
        .await
        .expect("verifier work order admission");
        run_fact_verifier_work_order(
            db.clone(),
            runtime,
            verifier.work_order_id,
            source.to_string(),
        )
        .await
        .expect("real independent verification");

        drop(db);
        let reopened = TranscriptStore::open(path.to_string_lossy().as_ref())
            .expect("Product OS store should reopen");
        let reopened_db = Arc::new(Mutex::new(reopened));
        let reopened_snapshot = snapshot(
            reopened_db,
            Arc::new(SessionRuntime::new()),
            "m05b-real-dogfood".to_string(),
        )
        .await
        .expect("reopened research snapshot")
        .expect("reopened project");
        let verified = reopened_snapshot
            .records
            .evidence
            .iter()
            .find(|item| item.evidence_id.ends_with(":evidence"))
            .expect("reopened verified evidence");
        assert_eq!(
            verified.verification,
            Some(EvidenceVerification::IndependentlyVerified)
        );
        assert!(verified.verifier_work_order_id.is_some());
        assert!(reopened_snapshot
            .work_orders
            .iter()
            .any(|order| order.role == ProductWorkOrderRole::FactVerifier
                && order.status == ProductWorkOrderStatus::Completed));
        assert_eq!(
            reopened_snapshot.records.ambiguities[0].status,
            crate::evidence_gates::AmbiguityStatus::Resolved
        );
        assert_eq!(
            reopened_snapshot.records.owner_decisions[0].status,
            crate::product_os::DecisionStatus::Adopted
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn cancellation_marks_work_order_and_rejects_late_result() {
        let path =
            std::env::temp_dir().join(format!("arena-m05b-cancel-{}.db", uuid::Uuid::new_v4()));
        let db = Arc::new(Mutex::new(
            TranscriptStore::open(path.to_string_lossy().as_ref()).expect("temporary store"),
        ));
        let runtime = Arc::new(SessionRuntime::new());
        let order = create_research_work_order(
            db.clone(),
            "m05b-cancel".to_string(),
            "bounded cancellation test".to_string(),
            "https://api.github.com/repos/github/github-mcp-server".to_string(),
        )
        .await
        .expect("admit research work order");
        let cancelled = cancel_product_work_order(
            db.clone(),
            runtime.clone(),
            order.work_order_id.clone(),
            "owner stopped research".to_string(),
        )
        .await
        .expect("cancel work order");
        assert_eq!(cancelled.status, ProductWorkOrderStatus::Cancelled);
        assert!(run_research_work_order(
            db,
            runtime,
            order.work_order_id,
            "https://api.github.com/repos/github/github-mcp-server".to_string(),
        )
        .await
        .is_err());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn fake_wrong_role_and_cancelled_verifier_cannot_finalize() {
        let path =
            std::env::temp_dir().join(format!("arena-m05b-verifier-{}.db", uuid::Uuid::new_v4()));
        let db = Arc::new(Mutex::new(
            TranscriptStore::open(path.to_string_lossy().as_ref()).expect("temporary store"),
        ));
        let runtime = Arc::new(SessionRuntime::new());
        let source = "https://api.github.com/repos/github/github-mcp-server";
        let researcher = create_research_work_order(
            db.clone(),
            "m05b-verifier-authority".to_string(),
            "verify the official repository metadata".to_string(),
            source.to_string(),
        )
        .await
        .expect("admit researcher");
        let researcher_id = researcher.work_order_id.clone();
        let completed = run_research_work_order(
            db.clone(),
            runtime.clone(),
            researcher.work_order_id,
            source.to_string(),
        )
        .await
        .expect("run researcher");
        let evidence_id = completed.evidence_id.expect("research evidence");

        assert!(
            run_fact_verifier_work_order(
                db.clone(),
                runtime.clone(),
                researcher_id,
                source.to_string(),
            )
            .await
            .is_err(),
            "researcher identity must not be accepted as a verifier"
        );

        let verifier = create_fact_verifier_work_order(
            db.clone(),
            "m05b-verifier-authority".to_string(),
            evidence_id,
            source.to_string(),
        )
        .await
        .expect("admit verifier");
        let cancelled = cancel_product_work_order(
            db.clone(),
            runtime.clone(),
            verifier.work_order_id.clone(),
            "owner cancelled verification".to_string(),
        )
        .await
        .expect("cancel verifier");
        assert_eq!(cancelled.status, ProductWorkOrderStatus::Cancelled);
        assert!(
            run_fact_verifier_work_order(db, runtime, verifier.work_order_id, source.to_string(),)
                .await
                .is_err(),
            "cancelled verifier must not finalize evidence"
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn stale_verifier_result_is_rejected_and_recorded_failed() {
        let path =
            std::env::temp_dir().join(format!("arena-m05b-stale-{}.db", uuid::Uuid::new_v4()));
        let db = Arc::new(Mutex::new(
            TranscriptStore::open(path.to_string_lossy().as_ref()).expect("temporary store"),
        ));
        let runtime = Arc::new(SessionRuntime::new());
        let source = "https://api.github.com/repos/github/github-mcp-server";
        let researcher = create_research_work_order(
            db.clone(),
            "m05b-stale-verifier".to_string(),
            "verify the official repository metadata".to_string(),
            source.to_string(),
        )
        .await
        .expect("admit researcher");
        let researcher_id = researcher.work_order_id.clone();
        let completed = run_research_work_order(
            db.clone(),
            runtime.clone(),
            researcher.work_order_id,
            source.to_string(),
        )
        .await
        .expect("run researcher");
        let evidence_id = completed.evidence_id.expect("research evidence");
        let verifier = create_fact_verifier_work_order(
            db.clone(),
            "m05b-stale-verifier".to_string(),
            evidence_id.clone(),
            source.to_string(),
        )
        .await
        .expect("admit verifier");

        admit_ambiguity(
            db.clone(),
            "m05b-stale-verifier".to_string(),
            researcher_id,
            "stale-ambiguity".to_string(),
            "stale-question".to_string(),
            "stale commitment".to_string(),
            "stale evidence".to_string(),
            AmbiguitySeverity::Low,
            vec![evidence_id.clone()],
        )
        .await
        .expect("advance authority after verifier admission");

        assert!(
            run_fact_verifier_work_order(
                db.clone(),
                runtime,
                verifier.work_order_id.clone(),
                source.to_string(),
            )
            .await
            .is_err(),
            "verifier admitted against an old project revision must fail closed"
        );
        let saved = db
            .lock()
            .expect("store lock")
            .get_product_work_order(&verifier.work_order_id)
            .expect("load verifier")
            .expect("verifier record");
        assert_eq!(saved.status, ProductWorkOrderStatus::Failed);
        let records = db
            .lock()
            .expect("store lock")
            .get_product_authority("m05b-stale-verifier")
            .expect("load authority")
            .expect("authority record");
        let records: ProductAuthorityRecords =
            serde_json::from_str(&records).expect("parse authority");
        assert_eq!(
            records
                .evidence
                .iter()
                .find(|item| item.evidence_id == evidence_id)
                .and_then(|item| item.verification),
            Some(EvidenceVerification::Unverified)
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn reopen_reconciles_pending_work_without_fabricating_completion() {
        let path =
            std::env::temp_dir().join(format!("arena-m05b-reconcile-{}.db", uuid::Uuid::new_v4()));
        let db = Arc::new(Mutex::new(
            TranscriptStore::open(path.to_string_lossy().as_ref()).expect("temporary store"),
        ));
        let order = create_research_work_order(
            db.clone(),
            "m05b-reconcile".to_string(),
            "bounded restart reconciliation".to_string(),
            "https://api.github.com/repos/github/github-mcp-server".to_string(),
        )
        .await
        .expect("admit work order");
        {
            let mut store = db.lock().expect("store lock");
            let mut running = store
                .get_product_work_order(&order.work_order_id)
                .expect("load order")
                .expect("order record");
            running.status = ProductWorkOrderStatus::Running;
            running.run_generation = 7;
            store.save_product_work_order(&running).expect("save order");
        }
        drop(db);

        let reopened = TranscriptStore::open(path.to_string_lossy().as_ref())
            .expect("reopen Product OS store");
        let snapshot = snapshot(
            Arc::new(Mutex::new(reopened)),
            Arc::new(SessionRuntime::new()),
            "m05b-reconcile".to_string(),
        )
        .await
        .expect("reconcile snapshot")
        .expect("reconciled project");
        assert_eq!(
            snapshot.work_orders[0].status,
            ProductWorkOrderStatus::ReconciliationRequired
        );
        assert!(snapshot.records.evidence.is_empty());
        let _ = std::fs::remove_file(path);
    }

    async fn admit_review_for_test(
        db: Arc<Mutex<TranscriptStore>>,
        project_id: &str,
        subject: &str,
        evidence_id: &str,
        kind: EvidenceKind,
        claim: &str,
    ) -> ProductWorkOrder {
        let order = create_product_director_work_order(
            db.clone(),
            project_id.to_string(),
            subject.to_string(),
        )
        .await
        .expect("admit Product Director work order");
        admit_product_review(
            db,
            project_id.to_string(),
            order.work_order_id,
            ProductReviewAdmission {
                evidence_id: evidence_id.to_string(),
                kind,
                claim: claim.to_string(),
                summary: format!("bounded internal validation review: {subject}"),
                source_reference: format!("arena://internal-validation/{evidence_id}"),
                decision_impact: true,
            },
        )
        .await
        .expect("admit typed Product Director evidence")
    }

    #[tokio::test]
    async fn real_verified_research_reopens_into_current_build_package_gates() {
        let path =
            std::env::temp_dir().join(format!("arena-m05c-handoff-{}.db", uuid::Uuid::new_v4()));
        let source = "https://api.github.com/repos/github/github-mcp-server";
        let project_id = "m05c-internal-github-metadata";
        let runtime = Arc::new(SessionRuntime::new());
        let db = Arc::new(Mutex::new(
            TranscriptStore::open(path.to_string_lossy().as_ref()).expect("temporary store"),
        ));

        let research = create_research_work_order(
            db.clone(),
            project_id.to_string(),
            "Can a bounded read-only utility retrieve a GitHub repository default branch from the official endpoint?"
                .to_string(),
            source.to_string(),
        )
        .await
        .expect("research admission");
        let research_id = research.work_order_id.clone();
        let research = run_research_work_order(
            db.clone(),
            runtime.clone(),
            research.work_order_id,
            source.to_string(),
        )
        .await
        .expect("real primary-source research");
        let evidence_id = research.evidence_id.clone().expect("research evidence");
        let verifier = create_fact_verifier_work_order(
            db.clone(),
            project_id.to_string(),
            evidence_id.clone(),
            source.to_string(),
        )
        .await
        .expect("independent verifier admission");
        let verifier_id = verifier.work_order_id.clone();
        run_fact_verifier_work_order(
            db.clone(),
            runtime.clone(),
            verifier.work_order_id,
            source.to_string(),
        )
        .await
        .expect("real independent source verification");

        drop(db);
        let reopened_db = Arc::new(Mutex::new(
            TranscriptStore::open(path.to_string_lossy().as_ref()).expect("reopen authority store"),
        ));
        let after_reopen = snapshot(
            reopened_db.clone(),
            Arc::new(SessionRuntime::new()),
            project_id.to_string(),
        )
        .await
        .expect("reopened snapshot")
        .expect("reopened project");
        assert_eq!(
            after_reopen
                .records
                .evidence
                .iter()
                .find(|item| item.evidence_id == evidence_id)
                .and_then(|item| item.verification),
            Some(EvidenceVerification::IndependentlyVerified)
        );

        let scope_order = create_product_director_work_order(
            reopened_db.clone(),
            project_id.to_string(),
            "Review bounded internal validation scope".to_string(),
        )
        .await
        .expect("scope reviewer");
        admit_product_scope_from_review(
            reopened_db.clone(),
            project_id.to_string(),
            scope_order.work_order_id,
            ProductScopeAdmission {
                objective: "Internal validation: report selected metadata for a public GitHub repository through a bounded read-only utility.".to_string(),
                target_user: "Arena engineering maintainer validating the Product OS handoff".to_string(),
                requirements: vec![
                    "Accept an explicitly selected public GitHub repository.".to_string(),
                    "Retrieve and report its identity and default branch from the official GitHub endpoint.".to_string(),
                    "Return bounded failure information without repository writes or authentication.".to_string(),
                ],
                constraints: vec![
                    "Use the official GitHub repository endpoint only.".to_string(),
                    "No write operation, authentication requirement, crawler, or background polling.".to_string(),
                ],
                non_goals: vec![
                    "This is internal Product OS validation, not market validation.".to_string(),
                    "Do not build a general GitHub client or research platform.".to_string(),
                ],
                interfaces: vec![
                    "A bounded read-only request/response boundary for a selected repository.".to_string(),
                ],
                risks: vec![
                    "GitHub endpoint failures, rate limits, malformed metadata, and stale source observations.".to_string(),
                ],
                acceptance_scenarios: vec![
                    "A valid selected repository reports identity and default branch.".to_string(),
                    "An invalid or unavailable endpoint returns bounded failure without a write.".to_string(),
                ],
                reviewer_restatement: crate::evidence_gates::ReviewerRestatement {
                    intended_outcome: "A tiny read-only metadata utility validates the research-to-build handoff.".to_string(),
                    success_condition: "The bounded utility can be admitted with source-backed architecture and safe failure behavior.".to_string(),
                    invented_behaviors: Vec::new(),
                },
            },
        )
        .await
        .expect("reviewed scope admission");

        let proposal_a = admit_review_for_test(
            reopened_db.clone(),
            project_id,
            "Independent architecture proposal A",
            "architecture-direct-http",
            EvidenceKind::ArchitectureProposal,
            "Use a small direct bounded HTTPS client for the official GitHub endpoint.",
        )
        .await;
        let proposal_b = admit_review_for_test(
            reopened_db.clone(),
            project_id,
            "Independent architecture proposal B",
            "architecture-existing-retrieval",
            EvidenceKind::ArchitectureProposal,
            "Wrap Arena's existing bounded official GitHub metadata retrieval instead of adding a client.",
        )
        .await;
        let reuse_review = admit_review_for_test(
            reopened_db.clone(),
            project_id,
            "Reuse classification review",
            "reuse-existing-retrieval",
            EvidenceKind::ReuseReview,
            "REUSE the existing official GitHub metadata retrieval boundary; do not build a crawler.",
        )
        .await;
        let constraints_review = admit_review_for_test(
            reopened_db.clone(),
            project_id,
            "Hard constraints review",
            "constraints-bounded-read-only",
            EvidenceKind::ConstraintsReview,
            "Keep the path read-only, bounded, unauthenticated, and explicit about failures.",
        )
        .await;
        let red_team = admit_review_for_test(
            reopened_db.clone(),
            project_id,
            "Red-team review",
            "red-team-github-metadata",
            EvidenceKind::RedTeamReview,
            "Reject query-bearing URLs; bound response size and timeout; surface stale metadata risk.",
        )
        .await;
        let dissent = admit_review_for_test(
            reopened_db.clone(),
            project_id,
            "Dissent review",
            "dissent-no-general-client",
            EvidenceKind::Dissent,
            "Do not generalize this internal slice into a GitHub client or market feature.",
        )
        .await;

        admit_ambiguity(
            reopened_db.clone(),
            project_id.to_string(),
            dissent.work_order_id.clone(),
            "owner-bounded-repository-choice".to_string(),
            "owner-bounded-repository-choice-question".to_string(),
            "Should the internal validation remain limited to an explicitly selected public repository?"
                .to_string(),
            "pre-implementation scope".to_string(),
            AmbiguitySeverity::Medium,
            vec![dissent.evidence_id.clone().expect("dissent evidence")],
        )
        .await
        .expect("owner-required ambiguity");
        adopt_owner_decision(
            reopened_db.clone(),
            project_id.to_string(),
            "owner-bounded-repository-choice".to_string(),
            "owner-bounded-repository-choice-question".to_string(),
            "remain bounded to a selected public repository".to_string(),
        )
        .await
        .expect("current owner decision");

        let spike_order = create_product_director_work_order(
            reopened_db.clone(),
            project_id.to_string(),
            "Risk spike: bounded official GitHub metadata retrieval".to_string(),
        )
        .await
        .expect("risk spike work order");
        let spike = run_product_github_risk_spike(
            reopened_db.clone(),
            runtime,
            spike_order.work_order_id,
            source.to_string(),
        )
        .await
        .expect("real bounded risk spike");
        let risk_evidence_id = spike.evidence_id.clone().expect("risk evidence");

        adopt_product_reuse_decision(
            reopened_db.clone(),
            project_id.to_string(),
            reuse_review.work_order_id.clone(),
            "official GitHub metadata retrieval".to_string(),
            ReuseClassification::Reuse,
            vec![reuse_review.evidence_id.clone().expect("reuse evidence")],
        )
        .await
        .expect("reuse decision");
        adopt_product_architecture(
            reopened_db.clone(),
            project_id.to_string(),
            ArchitectureAdmission {
                proposal_a_evidence_id: proposal_a.evidence_id.expect("proposal A evidence"),
                proposal_b_evidence_id: proposal_b.evidence_id.expect("proposal B evidence"),
                reuse_review_evidence_id: reuse_review.evidence_id.expect("reuse evidence"),
                constraints_review_evidence_id: constraints_review
                    .evidence_id
                    .expect("constraints evidence"),
                risk_experiment_evidence_ids: vec![risk_evidence_id],
                red_team_evidence_id: red_team.evidence_id.expect("red-team evidence"),
                dissent_evidence_id: dissent.evidence_id.expect("dissent evidence"),
                unresolved_high_blocker_evidence_ids: Vec::new(),
            },
        )
        .await
        .expect("architecture admission");
        adopt_narrow_build_direction(reopened_db.clone(), project_id.to_string())
            .await
            .expect("owner-adopted NarrowBuild direction");

        drop(reopened_db);
        let final_db = Arc::new(Mutex::new(
            TranscriptStore::open(path.to_string_lossy().as_ref()).expect("final reopen"),
        ));
        let assembled = assemble_current_build_package(final_db.clone(), project_id.to_string())
            .await
            .expect("assemble Build Package from reopened authority");
        let evaluation =
            evaluate_current_preimplementation_gates(final_db.clone(), project_id.to_string())
                .await
                .expect("current Build Package gate evaluation");
        assert_eq!(assembled.package_id, evaluation.package.package_id);
        assert!(
            evaluation
                .decisions
                .iter()
                .all(|decision| decision.status == crate::evidence_gates::GateStatus::Pass),
            "all pre-implementation gates must pass from reopened authority"
        );
        let package = evaluation.package.clone();
        println!(
            "M05C runtime IDs: project={project_id} research={research_id} verifier={verifier_id} package={} revision={} fingerprint={}",
            package.package_id, package.package_revision, package.authority_fingerprint
        );

        let stale_scope_order = create_product_director_work_order(
            final_db.clone(),
            project_id.to_string(),
            "Material scope change adversary".to_string(),
        )
        .await
        .expect("stale scope review");
        admit_product_scope_from_review(
            final_db.clone(),
            project_id.to_string(),
            stale_scope_order.work_order_id,
            ProductScopeAdmission {
                objective: "Changed internal validation objective".to_string(),
                target_user: "Arena engineering maintainer validating the Product OS handoff"
                    .to_string(),
                requirements: vec!["A materially changed requirement.".to_string()],
                constraints: vec!["No writes.".to_string()],
                non_goals: vec!["No market claim.".to_string()],
                interfaces: vec!["The existing bounded request boundary.".to_string()],
                risks: vec!["Changed scope requires a new owner direction.".to_string()],
                acceptance_scenarios: vec![
                    "Changed scope remains blocked pending direction.".to_string()
                ],
                reviewer_restatement: crate::evidence_gates::ReviewerRestatement {
                    intended_outcome: "Changed internal validation objective".to_string(),
                    success_condition: "Old package is stale.".to_string(),
                    invented_behaviors: Vec::new(),
                },
            },
        )
        .await
        .expect("material scope update");
        let changed_records = snapshot(
            final_db,
            Arc::new(SessionRuntime::new()),
            project_id.to_string(),
        )
        .await
        .expect("changed snapshot")
        .expect("changed project")
        .records;
        assert!(!package
            .is_current_for(&changed_records)
            .expect("stale package"));
        assert_eq!(
            package
                .evaluate_current(&changed_records, GateId::BuildReadiness)
                .expect("stale gate evaluation")
                .status,
            crate::evidence_gates::GateStatus::Stale
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn source_path_rejects_non_official_or_secret_bearing_urls() {
        assert!(normalize_github_url("https://example.com/repos/x").is_err());
        assert!(normalize_github_url("https://api.github.com/repos/x/y?token=secret").is_err());
    }
}
