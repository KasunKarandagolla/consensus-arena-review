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
    self, AuthorityAmbiguityRecord, ProductAuthorityRecords, ProductWorkOrder,
    ProductWorkOrderRole, ProductWorkOrderStatus,
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
            "Verified source evidence remains current after reopen".to_string(),
        ],
        decision_outcome: None,
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
        assert!(
            adopt_owner_decision(
                db.clone(),
                "m05b-real-dogfood".to_string(),
                "a-real".to_string(),
                "wrong-question".to_string(),
                "proceed".to_string(),
            )
            .await
            .is_err()
        );
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
        assert!(
            reopened_snapshot
                .work_orders
                .iter()
                .any(|order| order.role == ProductWorkOrderRole::FactVerifier
                    && order.status == ProductWorkOrderStatus::Completed)
        );
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
        assert!(
            run_research_work_order(
                db,
                runtime,
                order.work_order_id,
                "https://api.github.com/repos/github/github-mcp-server".to_string(),
            )
            .await
            .is_err()
        );
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

    #[test]
    fn source_path_rejects_non_official_or_secret_bearing_urls() {
        assert!(normalize_github_url("https://example.com/repos/x").is_err());
        assert!(normalize_github_url("https://api.github.com/repos/x/y?token=secret").is_err());
    }
}
