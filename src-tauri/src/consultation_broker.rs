//! Arena-owned durable consultation transaction controller.
//!
//! Browser/API transports are mechanics only. This module owns request
//! identity, durable transaction state, no-resend authority, conversation
//! anchors, result correlation, and restart reconciliation.

use crate::db_helpers;
use crate::errors::AgentError;
use crate::transcript_store::TranscriptStore;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub const CONSULTATION_SCHEMA_VERSION: u32 = 1;
pub const MAX_ADVISORY_BYTES: usize = 64 * 1024;

fn now() -> i64 {
    Utc::now().timestamp()
}

fn digest_text(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsultationProvider {
    ChatGpt,
    Qwen,
}

impl ConsultationProvider {
    pub const fn home_url(self) -> &'static str {
        match self {
            Self::ChatGpt => "https://chatgpt.com/",
            Self::Qwen => "https://chat.qwen.ai/",
        }
    }

    pub const fn expected_host(self) -> &'static str {
        match self {
            Self::ChatGpt => "chatgpt.com",
            Self::Qwen => "chat.qwen.ai",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsultationTransportKind {
    ApiCompatible,
    ExternalBrowserAgent,
    LegacyTauriWebView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsultationReason {
    OwnerRequested,
    ConsequentialAmbiguity,
    ArchitectureConflict,
    EvidenceDisagreement,
    RepeatedRepairBlocker,
    AuthorityReliabilityDissent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsultationTransactionState {
    Preparing,
    Staged,
    Armed,
    Submitted,
    Observing,
    ReadyToCommit,
    Complete,
    UnknownOutcome,
    OwnerRecovery,
    Cancelled,
}

impl ConsultationTransactionState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Cancelled)
    }

    pub const fn is_post_arm(self) -> bool {
        matches!(
            self,
            Self::Armed
                | Self::Submitted
                | Self::Observing
                | Self::ReadyToCommit
                | Self::Complete
                | Self::UnknownOutcome
                | Self::OwnerRecovery
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationEstablishment {
    Unestablished,
    Established,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAvailability {
    Available,
    NeedsAuth,
    Challenge,
    Lost,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsultationWorkOrder {
    pub schema_version: u32,
    pub request_id: String,
    pub project_id: String,
    pub originating_run_id: String,
    pub decision_id: String,
    pub reason: ConsultationReason,
    pub provider: ConsultationProvider,
    pub provider_config_id: String,
    pub profile_id: String,
    pub transport: ConsultationTransportKind,
    pub state: ConsultationTransactionState,
    pub execution_epoch: u64,
    pub authority_revision: u64,
    pub prompt_digest: String,
    pub disclosure_digest: String,
    pub anchor_id: String,
    pub anchor_revision: u64,
    pub submitted_at: Option<i64>,
    pub result_id: Option<String>,
    pub deadline_at: i64,
    pub budget_units: u32,
    pub linked_prior_request_id: Option<String>,
    pub failure: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ConsultationWorkOrder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_id: String,
        originating_run_id: String,
        decision_id: String,
        reason: ConsultationReason,
        provider: ConsultationProvider,
        provider_config_id: String,
        profile_id: String,
        transport: ConsultationTransportKind,
        execution_epoch: u64,
        authority_revision: u64,
        prompt: &str,
        disclosure: &str,
        deadline_at: i64,
        budget_units: u32,
    ) -> Result<Self, String> {
        for (value, field) in [
            (&project_id, "project id"),
            (&originating_run_id, "originating run id"),
            (&decision_id, "decision id"),
            (&provider_config_id, "provider config id"),
            (&profile_id, "profile id"),
        ] {
            if value.trim().is_empty() {
                return Err(format!("consultation request requires {field}"));
            }
        }
        if prompt.trim().is_empty()
            || deadline_at <= now()
            || budget_units == 0
            || execution_epoch == 0
        {
            return Err("consultation request requires prompt, future deadline, budget, and epoch"
                .to_string());
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        Ok(Self {
            schema_version: CONSULTATION_SCHEMA_VERSION,
            anchor_id: format!(
                "anchor:{}:{}:{}",
                project_id,
                match provider {
                    ConsultationProvider::ChatGpt => "chatgpt",
                    ConsultationProvider::Qwen => "qwen",
                },
                profile_id
            ),
            request_id,
            project_id,
            originating_run_id,
            decision_id,
            reason,
            provider,
            provider_config_id,
            profile_id,
            transport,
            state: ConsultationTransactionState::Preparing,
            execution_epoch,
            authority_revision,
            prompt_digest: digest_text(prompt),
            disclosure_digest: digest_text(disclosure),
            anchor_revision: 0,
            submitted_at: None,
            result_id: None,
            deadline_at,
            budget_units,
            linked_prior_request_id: None,
            failure: None,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    pub fn transition(&mut self, next: ConsultationTransactionState) -> Result<(), String> {
        let valid = matches!(
            (self.state, next),
            (
                ConsultationTransactionState::Preparing,
                ConsultationTransactionState::Staged
            ) | (
                ConsultationTransactionState::Staged,
                ConsultationTransactionState::Armed
            ) | (
                ConsultationTransactionState::Armed,
                ConsultationTransactionState::Submitted
            ) | (
                ConsultationTransactionState::Armed,
                ConsultationTransactionState::UnknownOutcome
            ) | (
                ConsultationTransactionState::Submitted,
                ConsultationTransactionState::Observing
            ) | (
                ConsultationTransactionState::Submitted,
                ConsultationTransactionState::UnknownOutcome
            ) | (
                ConsultationTransactionState::Observing,
                ConsultationTransactionState::ReadyToCommit
            ) | (
                ConsultationTransactionState::Observing,
                ConsultationTransactionState::UnknownOutcome
            ) | (
                ConsultationTransactionState::ReadyToCommit,
                ConsultationTransactionState::Complete
            ) | (
                ConsultationTransactionState::UnknownOutcome,
                ConsultationTransactionState::Observing
            ) | (
                ConsultationTransactionState::UnknownOutcome,
                ConsultationTransactionState::OwnerRecovery
            ) | (
                ConsultationTransactionState::OwnerRecovery,
                ConsultationTransactionState::Observing
            )
        ) || (!self.state.is_terminal() && next == ConsultationTransactionState::Cancelled);
        if !valid {
            return Err(format!(
                "invalid consultation transition {:?} -> {:?}",
                self.state, next
            ));
        }
        self.state = next;
        self.updated_at = now();
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationAnchor {
    pub anchor_id: String,
    pub provider: ConsultationProvider,
    pub provider_config_id: String,
    pub profile_id: String,
    pub thread_id: String,
    pub establishment: ConversationEstablishment,
    pub availability: ConversationAvailability,
    pub canonical_url: Option<String>,
    pub provider_conversation_id: Option<String>,
    pub provider_branch_id: Option<String>,
    pub pending_request_id: Option<String>,
    pub last_confirmed_user_turn_digest: Option<String>,
    pub last_confirmed_assistant_turn_digest: Option<String>,
    pub adapter_version: String,
    pub revision: u64,
    pub updated_at: i64,
}

impl ConversationAnchor {
    pub fn initial(order: &ConsultationWorkOrder, adapter_version: &str) -> Self {
        Self {
            anchor_id: order.anchor_id.clone(),
            provider: order.provider,
            provider_config_id: order.provider_config_id.clone(),
            profile_id: order.profile_id.clone(),
            thread_id: order.decision_id.clone(),
            establishment: ConversationEstablishment::Unestablished,
            availability: ConversationAvailability::Unknown,
            canonical_url: None,
            provider_conversation_id: None,
            provider_branch_id: None,
            pending_request_id: Some(order.request_id.clone()),
            last_confirmed_user_turn_digest: None,
            last_confirmed_assistant_turn_digest: None,
            adapter_version: adapter_version.to_string(),
            revision: 1,
            updated_at: now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConversationAnchorUpdate {
    pub expected_revision: u64,
    pub availability: ConversationAvailability,
    pub established: bool,
    pub canonical_url: Option<String>,
    pub provider_conversation_id: Option<String>,
    pub provider_branch_id: Option<String>,
    pub pending_request_id: Option<String>,
    pub last_confirmed_user_turn_digest: Option<String>,
    pub last_confirmed_assistant_turn_digest: Option<String>,
    pub adapter_version: String,
}

#[derive(Debug, Clone)]
pub struct ArmedSendPermit {
    request_id: String,
    execution_epoch: u64,
    transport: ConsultationTransportKind,
    issued_at: i64,
}

impl ArmedSendPermit {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn transport(&self) -> ConsultationTransportKind {
        self.transport
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportSubmissionOutcome {
    Submitted {
        canonical_url: Option<String>,
    },
    UnknownOutcome {
        diagnostic: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsultationObservation {
    pub request_id: String,
    pub execution_epoch: u64,
    pub provider: ConsultationProvider,
    pub provider_config_id: String,
    pub profile_id: String,
    pub canonical_url: String,
    pub user_turn_digest: String,
    pub assistant_turn_digest: String,
    pub advisory_text: String,
    pub provider_conversation_id: Option<String>,
    pub provider_branch_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsultationResult {
    pub result_id: String,
    pub request_id: String,
    pub execution_epoch: u64,
    pub provider: ConsultationProvider,
    pub canonical_url: String,
    pub user_turn_digest: String,
    pub assistant_turn_digest: String,
    pub content_digest: String,
    pub advisory_text: String,
    pub completed_at: i64,
}

pub fn validate_application_url(
    provider: ConsultationProvider,
    raw_url: &str,
    established_identity: bool,
) -> Result<String, String> {
    let mut url =
        reqwest::Url::parse(raw_url).map_err(|_| "consultation application URL is invalid".to_string())?;
    if url.scheme() != "https" || url.host_str() != Some(provider.expected_host()) {
        return Err("consultation application URL has the wrong origin".to_string());
    }
    if url.username() != "" || url.password().is_some() {
        return Err("consultation application URL must not contain credentials".to_string());
    }
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/');
    let lower = path.to_ascii_lowercase();
    let blocked = lower.is_empty()
        || matches!(
            lower.as_str(),
            "/login" | "/auth" | "/authorize" | "/new" | "/chat" | "/"
        )
        || lower.contains("/oauth")
        || lower.contains("/challenge")
        || lower.contains("/captcha")
        || lower.contains("/login/");
    if established_identity && blocked {
        return Err("setup/auth/new-chat URL cannot establish a conversation anchor".to_string());
    }
    Ok(url.to_string())
}

pub fn apply_anchor_update(
    current: &ConversationAnchor,
    update: ConversationAnchorUpdate,
) -> Result<ConversationAnchor, String> {
    if current.revision != update.expected_revision {
        return Err("stale ConversationAnchor revision".to_string());
    }
    let mut next = current.clone();
    next.availability = update.availability;
    next.adapter_version = update.adapter_version;
    next.pending_request_id = update.pending_request_id;
    if update.established {
        let raw_url = update
            .canonical_url
            .as_deref()
            .ok_or_else(|| "established ConversationAnchor requires a canonical URL".to_string())?;
        let canonical = validate_application_url(current.provider, raw_url, true)?;
        if update.provider_conversation_id.as_deref().is_none_or(str::is_empty)
            && canonical == current.provider.home_url()
        {
            return Err("established anchor has no durable conversation identity".to_string());
        }
        next.establishment = ConversationEstablishment::Established;
        next.canonical_url = Some(canonical);
        next.provider_conversation_id = update.provider_conversation_id;
        next.provider_branch_id = update.provider_branch_id;
        next.last_confirmed_user_turn_digest = update.last_confirmed_user_turn_digest;
        next.last_confirmed_assistant_turn_digest = update.last_confirmed_assistant_turn_digest;
    } else if current.establishment == ConversationEstablishment::Unestablished {
        next.canonical_url = update
            .canonical_url
            .as_deref()
            .map(|value| validate_application_url(current.provider, value, false))
            .transpose()?;
    }
    next.revision = next.revision.saturating_add(1);
    next.updated_at = now();
    Ok(next)
}

pub async fn create_request(
    db: Arc<Mutex<TranscriptStore>>,
    order: ConsultationWorkOrder,
    adapter_version: &str,
) -> Result<ConsultationWorkOrder, String> {
    let anchor = ConversationAnchor::initial(&order, adapter_version);
    db_helpers::run_blocking(move || {
        let mut store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        if store.get_consultation_work_order(&order.request_id)?.is_some() {
            return Err(AgentError::DatabaseError(
                "consultation request identity already exists".to_string(),
            ));
        }
        if store
            .get_conversation_anchor(&anchor.anchor_id)?
            .is_none()
        {
            store.save_conversation_anchor(&anchor)?;
        }
        store.save_consultation_work_order(&order)?;
        Ok(order)
    })
    .await
    .map_err(|error| error.to_string())
}

pub async fn stage_request(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
) -> Result<ConsultationWorkOrder, String> {
    mutate_request(db, request_id, |order| {
        order.transition(ConsultationTransactionState::Staged)
    })
    .await
}

pub async fn arm_for_send(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
) -> Result<(ConsultationWorkOrder, ArmedSendPermit), String> {
    let order = mutate_request(db, request_id, |order| {
        order.transition(ConsultationTransactionState::Armed)
    })
    .await?;
    let permit = ArmedSendPermit {
        request_id: order.request_id.clone(),
        execution_epoch: order.execution_epoch,
        transport: order.transport,
        issued_at: now(),
    };
    Ok((order, permit))
}

async fn mutate_request<F>(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
    mutation: F,
) -> Result<ConsultationWorkOrder, String>
where
    F: FnOnce(&mut ConsultationWorkOrder) -> Result<(), String> + Send + 'static,
{
    db_helpers::run_blocking(move || {
        let mut store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        let mut order = store
            .get_consultation_work_order(&request_id)?
            .ok_or_else(|| AgentError::DatabaseError("consultation request is unknown".to_string()))?;
        mutation(&mut order).map_err(AgentError::DatabaseError)?;
        store.save_consultation_work_order(&order)?;
        Ok(order)
    })
    .await
    .map_err(|error| error.to_string())
}

pub async fn record_submission_outcome(
    db: Arc<Mutex<TranscriptStore>>,
    permit: ArmedSendPermit,
    outcome: TransportSubmissionOutcome,
) -> Result<ConsultationWorkOrder, String> {
    if now().saturating_sub(permit.issued_at) > 120 {
        return Err("consultation send permit expired before submission outcome".to_string());
    }
    mutate_request(db, permit.request_id.clone(), move |order| {
        if order.state != ConsultationTransactionState::Armed
            || order.execution_epoch != permit.execution_epoch
            || order.transport != permit.transport
        {
            return Err("consultation send permit is stale or mismatched".to_string());
        }
        match outcome {
            TransportSubmissionOutcome::Submitted { .. } => {
                order.submitted_at = Some(now());
                order.transition(ConsultationTransactionState::Submitted)
            }
            TransportSubmissionOutcome::UnknownOutcome { diagnostic } => {
                order.failure = Some(diagnostic.chars().take(512).collect());
                order.transition(ConsultationTransactionState::UnknownOutcome)
            }
        }
    })
    .await
}

pub async fn mark_observing(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
) -> Result<ConsultationWorkOrder, String> {
    mutate_request(db, request_id, |order| {
        order.transition(ConsultationTransactionState::Observing)
    })
    .await
}

pub async fn mark_unknown_outcome(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
    diagnostic: String,
) -> Result<ConsultationWorkOrder, String> {
    mutate_request(db, request_id, move |order| {
        order.failure = Some(diagnostic.chars().take(512).collect());
        order.transition(ConsultationTransactionState::UnknownOutcome)
    })
    .await
}

pub async fn reconcile_after_restart(
    db: Arc<Mutex<TranscriptStore>>,
) -> Result<Vec<ConsultationWorkOrder>, String> {
    db_helpers::run_blocking(move || {
        let mut store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        let mut changed = Vec::new();
        for mut order in store.list_open_consultation_work_orders()? {
            match order.state {
                ConsultationTransactionState::Armed => {
                    order.state = ConsultationTransactionState::UnknownOutcome;
                    order.failure = Some(
                        "Arena restarted after Armed; automatic resend is forbidden".to_string(),
                    );
                }
                ConsultationTransactionState::Submitted => {
                    order.state = ConsultationTransactionState::Observing;
                }
                _ => continue,
            }
            order.updated_at = now();
            store.save_consultation_work_order(&order)?;
            changed.push(order);
        }
        Ok(changed)
    })
    .await
    .map_err(|error| error.to_string())
}

pub async fn update_anchor(
    db: Arc<Mutex<TranscriptStore>>,
    anchor_id: String,
    update: ConversationAnchorUpdate,
) -> Result<ConversationAnchor, String> {
    db_helpers::run_blocking(move || {
        let mut store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        let current = store
            .get_conversation_anchor(&anchor_id)?
            .ok_or_else(|| AgentError::DatabaseError("ConversationAnchor is unknown".to_string()))?;
        let next = apply_anchor_update(&current, update).map_err(AgentError::DatabaseError)?;
        store.save_conversation_anchor(&next)?;
        Ok(next)
    })
    .await
    .map_err(|error| error.to_string())
}

pub async fn admit_observation(
    db: Arc<Mutex<TranscriptStore>>,
    observation: ConsultationObservation,
) -> Result<ConsultationResult, String> {
    if observation.advisory_text.trim().is_empty()
        || observation.advisory_text.len() > MAX_ADVISORY_BYTES
    {
        return Err("consultation observation has no bounded advisory result".to_string());
    }
    db_helpers::run_blocking(move || {
        let mut store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        let mut order = store
            .get_consultation_work_order(&observation.request_id)?
            .ok_or_else(|| AgentError::DatabaseError("consultation request is unknown".to_string()))?;
        if order.execution_epoch != observation.execution_epoch
            || order.provider != observation.provider
            || order.provider_config_id != observation.provider_config_id
            || order.profile_id != observation.profile_id
            || !matches!(
                order.state,
                ConsultationTransactionState::Submitted
                    | ConsultationTransactionState::Observing
                    | ConsultationTransactionState::UnknownOutcome
            )
        {
            return Err(AgentError::DatabaseError(
                "consultation observation is stale or mismatched".to_string(),
            ));
        }
        if observation.user_turn_digest != order.prompt_digest {
            return Err(AgentError::DatabaseError(
                "consultation observation does not correlate to the submitted request".to_string(),
            ));
        }
        let anchor = store
            .get_conversation_anchor(&order.anchor_id)?
            .ok_or_else(|| AgentError::DatabaseError("ConversationAnchor is missing".to_string()))?;
        if anchor.provider != observation.provider
            || anchor.profile_id != observation.profile_id
            || anchor.establishment != ConversationEstablishment::Established
        {
            return Err(AgentError::DatabaseError(
                "consultation observation does not match the established anchor".to_string(),
            ));
        }
        let canonical = validate_application_url(observation.provider, &observation.canonical_url, true)
            .map_err(AgentError::DatabaseError)?;
        if anchor.canonical_url.as_deref() != Some(canonical.as_str())
            && anchor.provider_conversation_id.as_deref()
                != observation.provider_conversation_id.as_deref()
        {
            return Err(AgentError::DatabaseError(
                "consultation response came from the wrong conversation".to_string(),
            ));
        }
        if observation.assistant_turn_digest.trim().is_empty() {
            return Err(AgentError::DatabaseError(
                "consultation response has no assistant-turn identity".to_string(),
            ));
        }
        let result_id = format!(
            "consultation-result:{}",
            digest_text(&format!(
                "{}:{}:{}",
                order.request_id, observation.user_turn_digest, observation.assistant_turn_digest
            ))
        );
        let result = ConsultationResult {
            result_id: result_id.clone(),
            request_id: order.request_id.clone(),
            execution_epoch: order.execution_epoch,
            provider: order.provider,
            canonical_url: canonical,
            user_turn_digest: observation.user_turn_digest,
            assistant_turn_digest: observation.assistant_turn_digest,
            content_digest: digest_text(&observation.advisory_text),
            advisory_text: observation.advisory_text,
            completed_at: now(),
        };
        store.save_consultation_result(&result)?;
        order.result_id = Some(result_id);
        if order.state == ConsultationTransactionState::UnknownOutcome {
            order.state = ConsultationTransactionState::Observing;
        }
        order.transition(ConsultationTransactionState::ReadyToCommit)
            .map_err(AgentError::DatabaseError)?;
        store.save_consultation_work_order(&order)?;
        Ok(result)
    })
    .await
    .map_err(|error| error.to_string())
}

pub async fn commit_result(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
) -> Result<ConsultationWorkOrder, String> {
    mutate_request(db, request_id, |order| {
        if order.result_id.is_none() {
            return Err("consultation result cannot commit without correlated evidence".to_string());
        }
        order.transition(ConsultationTransactionState::Complete)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> ConsultationWorkOrder {
        ConsultationWorkOrder::new(
            "project".to_string(),
            "run".to_string(),
            "decision".to_string(),
            ConsultationReason::ArchitectureConflict,
            ConsultationProvider::ChatGpt,
            "chatgpt-consumer".to_string(),
            "arena-chatgpt".to_string(),
            ConsultationTransportKind::ExternalBrowserAgent,
            1,
            3,
            "question",
            "bounded disclosure",
            now() + 600,
            1,
        )
        .expect("order")
    }

    #[test]
    fn post_arm_state_cannot_return_to_staged() {
        let mut value = order();
        value.transition(ConsultationTransactionState::Staged).unwrap();
        value.transition(ConsultationTransactionState::Armed).unwrap();
        assert!(value.transition(ConsultationTransactionState::Staged).is_err());
    }

    #[test]
    fn setup_and_auth_urls_cannot_overwrite_established_anchor() {
        let value = order();
        let current = ConversationAnchor {
            establishment: ConversationEstablishment::Established,
            availability: ConversationAvailability::Available,
            canonical_url: Some("https://chatgpt.com/c/123".to_string()),
            provider_conversation_id: Some("123".to_string()),
            revision: 4,
            ..ConversationAnchor::initial(&value, "adapter-v1")
        };
        let update = ConversationAnchorUpdate {
            expected_revision: 4,
            availability: ConversationAvailability::NeedsAuth,
            established: true,
            canonical_url: Some("https://chatgpt.com/auth/login".to_string()),
            provider_conversation_id: None,
            provider_branch_id: None,
            pending_request_id: None,
            last_confirmed_user_turn_digest: None,
            last_confirmed_assistant_turn_digest: None,
            adapter_version: "adapter-v1".to_string(),
        };
        assert!(apply_anchor_update(&current, update).is_err());
        assert_eq!(current.canonical_url.as_deref(), Some("https://chatgpt.com/c/123"));
    }

    #[test]
    fn stale_anchor_revision_is_rejected() {
        let value = order();
        let current = ConversationAnchor::initial(&value, "adapter-v1");
        let update = ConversationAnchorUpdate {
            expected_revision: 0,
            availability: ConversationAvailability::Available,
            established: false,
            canonical_url: Some("https://chatgpt.com/".to_string()),
            provider_conversation_id: None,
            provider_branch_id: None,
            pending_request_id: Some(value.request_id),
            last_confirmed_user_turn_digest: None,
            last_confirmed_assistant_turn_digest: None,
            adapter_version: "adapter-v1".to_string(),
        };
        assert!(apply_anchor_update(&current, update).is_err());
    }

    #[test]
    fn established_provider_urls_are_origin_strict() {
        assert!(validate_application_url(
            ConsultationProvider::ChatGpt,
            "https://chatgpt.com/c/123",
            true
        )
        .is_ok());
        assert!(validate_application_url(
            ConsultationProvider::ChatGpt,
            "https://chatgpt.com.evil.example/c/123",
            true
        )
        .is_err());
        assert!(validate_application_url(
            ConsultationProvider::Qwen,
            "https://chat.qwen.ai/chat/abc",
            true
        )
        .is_ok());
    }
}
