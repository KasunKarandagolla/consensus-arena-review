//! Production consultation transaction orchestration.
//!
//! The runtime composes durable Arena authority with a mechanical transport.
//! It never retries or fails over after Armed.

use crate::consultation_broker::{
    self, ConsultationProvider, ConsultationReason, ConsultationResult,
    ConsultationTransactionState, ConsultationTransportKind, ConsultationWorkOrder,
    ConversationAnchor, ConversationAnchorUpdate, ConversationAvailability,
    ConversationEstablishment,
};
use crate::db_helpers;
use crate::errors::AgentError;
use crate::external_browser;
use crate::transcript_store::TranscriptStore;
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum ConsultationExecutionOutcome {
    Complete(ConsultationResult),
    UnknownOutcome(ConsultationWorkOrder),
    PreSendBlocked(ConsultationWorkOrder),
}

fn now() -> i64 {
    Utc::now().timestamp()
}

async fn load_anchor(
    db: Arc<Mutex<TranscriptStore>>,
    anchor_id: String,
) -> Result<ConversationAnchor, String> {
    db_helpers::run_blocking(move || {
        let store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        store
            .get_conversation_anchor(&anchor_id)?
            .ok_or_else(|| AgentError::DatabaseError("ConversationAnchor is missing".to_string()))
    })
    .await
    .map_err(|error| error.to_string())
}

async fn load_order(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
) -> Result<ConsultationWorkOrder, String> {
    db_helpers::run_blocking(move || {
        let store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        store
            .get_consultation_work_order(&request_id)?
            .ok_or_else(|| AgentError::DatabaseError("consultation request is missing".to_string()))
    })
    .await
    .map_err(|error| error.to_string())
}

async fn decision_already_consulted(
    db: Arc<Mutex<TranscriptStore>>,
    project_id: String,
    decision_id: String,
) -> Result<bool, String> {
    db_helpers::run_blocking(move || {
        let store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        Ok(store
            .find_consultation_for_decision(&project_id, &decision_id)?
            .is_some())
    })
    .await
    .map_err(|error| error.to_string())
}

fn stable_profile_id(provider: ConsultationProvider) -> &'static str {
    match provider {
        ConsultationProvider::ChatGpt => "arena-consult-chatgpt",
        ConsultationProvider::Qwen => "arena-consult-qwen",
    }
}

fn stable_provider_config_id(provider: ConsultationProvider) -> &'static str {
    match provider {
        ConsultationProvider::ChatGpt => "consumer-web-chatgpt-v1",
        ConsultationProvider::Qwen => "consumer-web-qwen-v1",
    }
}

async fn persist_pre_send_failure(
    db: Arc<Mutex<TranscriptStore>>,
    request_id: String,
    diagnostic: String,
) -> Result<ConsultationWorkOrder, String> {
    db_helpers::run_blocking(move || {
        let mut store = db.lock().map_err(|_| {
            AgentError::DatabaseError("consultation store lock poisoned".to_string())
        })?;
        let mut order = store
            .get_consultation_work_order(&request_id)?
            .ok_or_else(|| AgentError::DatabaseError("consultation request is missing".to_string()))?;
        if order.state.is_post_arm() {
            return Err(AgentError::DatabaseError(
                "pre-send failure cannot rewrite a post-Armed request".to_string(),
            ));
        }
        order.failure = Some(diagnostic.chars().take(512).collect());
        order.updated_at = now();
        store.save_consultation_work_order(&order)?;
        Ok(order)
    })
    .await
    .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_external_browser_consultation(
    db: Arc<Mutex<TranscriptStore>>,
    repository: PathBuf,
    profile_root: PathBuf,
    project_id: String,
    originating_run_id: String,
    decision_id: String,
    reason: ConsultationReason,
    provider: ConsultationProvider,
    execution_epoch: u64,
    authority_revision: u64,
    prompt: String,
    disclosure_summary: String,
) -> Result<ConsultationExecutionOutcome, String> {
    if decision_already_consulted(
        db.clone(),
        project_id.clone(),
        decision_id.clone(),
    )
    .await?
    {
        return Err("this Product OS decision already has a consultation round".to_string());
    }
    if !repository.is_dir() || !profile_root.is_absolute() {
        return Err("consultation repository/profile roots are invalid".to_string());
    }
    let order = ConsultationWorkOrder::new(
        project_id,
        originating_run_id,
        decision_id,
        reason,
        provider,
        stable_provider_config_id(provider).to_string(),
        stable_profile_id(provider).to_string(),
        ConsultationTransportKind::ExternalBrowserAgent,
        execution_epoch,
        authority_revision,
        &prompt,
        &disclosure_summary,
        now() + 15 * 60,
        1,
    )?;
    let order = consultation_broker::create_request(
        db.clone(),
        order,
        external_browser::AGENT_BROWSER_ADAPTER_VERSION,
    )
    .await?;
    let anchor = load_anchor(db.clone(), order.anchor_id.clone()).await?;
    let anchor_url = if anchor.establishment == ConversationEstablishment::Established
        && anchor.availability == ConversationAvailability::Available
    {
        anchor.canonical_url.as_deref()
    } else {
        None
    };
    let staged = match external_browser::stage(
        &repository,
        &profile_root,
        &order,
        &prompt,
        anchor_url,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            let blocked = persist_pre_send_failure(
                db,
                order.request_id,
                format!("external browser staging blocked before Armed: {error}"),
            )
            .await?;
            return Ok(ConsultationExecutionOutcome::PreSendBlocked(blocked));
        }
    };

    let current_anchor = load_anchor(db.clone(), order.anchor_id.clone()).await?;
    let staged_established = current_anchor.establishment == ConversationEstablishment::Established;
    let _ = consultation_broker::update_anchor(
        db.clone(),
        order.anchor_id.clone(),
        ConversationAnchorUpdate {
            expected_revision: current_anchor.revision,
            availability: ConversationAvailability::Available,
            established: staged_established,
            canonical_url: if staged_established {
                current_anchor.canonical_url.clone()
            } else {
                Some(staged.current_url.clone())
            },
            provider_conversation_id: current_anchor.provider_conversation_id.clone(),
            provider_branch_id: current_anchor.provider_branch_id.clone(),
            pending_request_id: Some(order.request_id.clone()),
            last_confirmed_user_turn_digest: current_anchor
                .last_confirmed_user_turn_digest
                .clone(),
            last_confirmed_assistant_turn_digest: current_anchor
                .last_confirmed_assistant_turn_digest
                .clone(),
            adapter_version: external_browser::AGENT_BROWSER_ADAPTER_VERSION.to_string(),
        },
    )
    .await?;

    let staged_order = consultation_broker::stage_request(
        db.clone(),
        order.request_id.clone(),
    )
    .await?;
    let (armed_order, permit) = consultation_broker::arm_for_send(
        db.clone(),
        staged_order.request_id.clone(),
    )
    .await?;
    let effect = external_browser::submit_once(&repository, permit, &staged).await;
    let mut current =
        consultation_broker::record_submission_outcome(db.clone(), effect).await?;
    if current.state == ConsultationTransactionState::Submitted {
        current = consultation_broker::mark_observing(
            db.clone(),
            armed_order.request_id.clone(),
        )
        .await?;
    }

    for _ in 0..8 {
        match external_browser::observe_once(&repository, &current, &staged).await {
            Ok(Some(observation)) => {
                let anchor = load_anchor(db.clone(), current.anchor_id.clone()).await?;
                let updated = consultation_broker::update_anchor(
                    db.clone(),
                    current.anchor_id.clone(),
                    ConversationAnchorUpdate {
                        expected_revision: anchor.revision,
                        availability: ConversationAvailability::Available,
                        established: true,
                        canonical_url: Some(observation.canonical_url.clone()),
                        provider_conversation_id: observation
                            .provider_conversation_id
                            .clone(),
                        provider_branch_id: observation.provider_branch_id.clone(),
                        pending_request_id: Some(current.request_id.clone()),
                        last_confirmed_user_turn_digest: Some(
                            observation.user_turn_digest.clone(),
                        ),
                        last_confirmed_assistant_turn_digest: Some(
                            observation.assistant_turn_digest.clone(),
                        ),
                        adapter_version:
                            external_browser::AGENT_BROWSER_ADAPTER_VERSION.to_string(),
                    },
                )
                .await?;
                let result = consultation_broker::admit_observation(
                    db.clone(),
                    observation,
                )
                .await?;
                consultation_broker::commit_result(
                    db.clone(),
                    current.request_id.clone(),
                )
                .await?;
                let _ = consultation_broker::update_anchor(
                    db.clone(),
                    updated.anchor_id.clone(),
                    ConversationAnchorUpdate {
                        expected_revision: updated.revision,
                        availability: ConversationAvailability::Available,
                        established: true,
                        canonical_url: updated.canonical_url.clone(),
                        provider_conversation_id: updated.provider_conversation_id.clone(),
                        provider_branch_id: updated.provider_branch_id.clone(),
                        pending_request_id: None,
                        last_confirmed_user_turn_digest: updated
                            .last_confirmed_user_turn_digest
                            .clone(),
                        last_confirmed_assistant_turn_digest: updated
                            .last_confirmed_assistant_turn_digest
                            .clone(),
                        adapter_version:
                            external_browser::AGENT_BROWSER_ADAPTER_VERSION.to_string(),
                    },
                )
                .await;
                external_browser::close_owned_session(&repository, &staged).await;
                return Ok(ConsultationExecutionOutcome::Complete(result));
            }
            Ok(None) => {
                current = load_order(db.clone(), current.request_id.clone()).await?;
            }
            Err(error) => {
                let state = load_order(db.clone(), current.request_id.clone()).await?;
                current = if state.state == ConsultationTransactionState::UnknownOutcome {
                    state
                } else {
                    consultation_broker::mark_unknown_outcome(
                        db.clone(),
                        state.request_id.clone(),
                        format!("post-Armed observation could not correlate a result: {error}"),
                    )
                    .await?
                };
                break;
            }
        }
    }

    if current.state != ConsultationTransactionState::UnknownOutcome {
        current = consultation_broker::mark_unknown_outcome(
            db.clone(),
            current.request_id.clone(),
            "post-Armed observation window ended without correlated response; resend is forbidden"
                .to_string(),
        )
        .await?;
    }
    external_browser::close_owned_session(&repository, &staged).await;
    Ok(ConsultationExecutionOutcome::UnknownOutcome(current))
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_owned_external_browser_consultation(
    runtime: Arc<crate::session_runtime::SessionRuntime>,
    db: Arc<Mutex<TranscriptStore>>,
    repository: PathBuf,
    profile_root: PathBuf,
    project_id: String,
    originating_run_id: String,
    decision_id: String,
    reason: ConsultationReason,
    provider: ConsultationProvider,
    execution_epoch: u64,
    authority_revision: u64,
    prompt: String,
    disclosure_summary: String,
) -> Result<ConsultationExecutionOutcome, String> {
    let runtime_session_id = format!("consultation:{project_id}:{decision_id}");
    let permit = runtime.try_acquire_start(runtime_session_id)?;
    let owner = permit.owner();
    let task_owner = owner.clone();
    let task_runtime = runtime.clone();
    let (activate_tx, activate_rx) = tokio::sync::oneshot::channel();
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let result = if activate_rx.await.is_ok() {
            execute_external_browser_consultation(
                db,
                repository,
                profile_root,
                project_id,
                originating_run_id,
                decision_id,
                reason,
                provider,
                execution_epoch,
                authority_revision,
                prompt,
                disclosure_summary,
            )
            .await
        } else {
            Err("consultation runtime was not activated".to_string())
        };
        task_runtime.mark_completed(&task_owner);
        let _ = result_tx.send(result);
    });
    permit.commit(task, activate_tx)?;
    result_rx
        .await
        .map_err(|_| "consultation runtime stopped before returning".to_string())?
}

pub async fn recover_external_browser_consultation(
    db: Arc<Mutex<TranscriptStore>>,
    repository: PathBuf,
    profile_root: PathBuf,
    request_id: String,
) -> Result<ConsultationExecutionOutcome, String> {
    let mut order = load_order(db.clone(), request_id).await?;
    if order.state == ConsultationTransactionState::UnknownOutcome {
        order = consultation_broker::begin_owner_recovery(
            db.clone(),
            order.request_id.clone(),
        )
        .await?;
    }
    if !matches!(
        order.state,
        ConsultationTransactionState::OwnerRecovery
            | ConsultationTransactionState::Observing
    ) {
        return Err(
            "consultation recovery is observation-only and requires UnknownOutcome/OwnerRecovery/Observing"
                .to_string(),
        );
    }
    let anchor = load_anchor(db.clone(), order.anchor_id.clone()).await?;
    let observation = match external_browser::recover_observe_once(
        &repository,
        &profile_root,
        &order,
        &anchor,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return Err(format!(
                "consultation recovery remains pending without resend: {error}"
            ));
        }
    };
    let Some(observation) = observation else {
        return Ok(ConsultationExecutionOutcome::UnknownOutcome(order));
    };

    let current_anchor = load_anchor(db.clone(), order.anchor_id.clone()).await?;
    let updated = consultation_broker::update_anchor(
        db.clone(),
        order.anchor_id.clone(),
        ConversationAnchorUpdate {
            expected_revision: current_anchor.revision,
            availability: ConversationAvailability::Available,
            established: true,
            canonical_url: Some(observation.canonical_url.clone()),
            provider_conversation_id: observation.provider_conversation_id.clone(),
            provider_branch_id: observation.provider_branch_id.clone(),
            pending_request_id: Some(order.request_id.clone()),
            last_confirmed_user_turn_digest: Some(observation.user_turn_digest.clone()),
            last_confirmed_assistant_turn_digest: Some(
                observation.assistant_turn_digest.clone(),
            ),
            adapter_version: external_browser::AGENT_BROWSER_ADAPTER_VERSION.to_string(),
        },
    )
    .await?;
    let result = consultation_broker::admit_observation(db.clone(), observation).await?;
    consultation_broker::commit_result(db.clone(), order.request_id.clone()).await?;
    let _ = consultation_broker::update_anchor(
        db,
        updated.anchor_id.clone(),
        ConversationAnchorUpdate {
            expected_revision: updated.revision,
            availability: ConversationAvailability::Available,
            established: true,
            canonical_url: updated.canonical_url.clone(),
            provider_conversation_id: updated.provider_conversation_id.clone(),
            provider_branch_id: updated.provider_branch_id.clone(),
            pending_request_id: None,
            last_confirmed_user_turn_digest: updated.last_confirmed_user_turn_digest.clone(),
            last_confirmed_assistant_turn_digest: updated
                .last_confirmed_assistant_turn_digest
                .clone(),
            adapter_version: external_browser::AGENT_BROWSER_ADAPTER_VERSION.to_string(),
        },
    )
    .await;
    Ok(ConsultationExecutionOutcome::Complete(result))
}

pub async fn recover_owned_external_browser_consultation(
    runtime: Arc<crate::session_runtime::SessionRuntime>,
    db: Arc<Mutex<TranscriptStore>>,
    repository: PathBuf,
    profile_root: PathBuf,
    project_id: String,
    request_id: String,
) -> Result<ConsultationExecutionOutcome, String> {
    let runtime_session_id = format!("consultation:{project_id}:recovery:{request_id}");
    let permit = runtime.try_acquire_start(runtime_session_id)?;
    let owner = permit.owner();
    let task_owner = owner.clone();
    let task_runtime = runtime.clone();
    let (activate_tx, activate_rx) = tokio::sync::oneshot::channel();
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let result = if activate_rx.await.is_ok() {
            recover_external_browser_consultation(
                db,
                repository,
                profile_root,
                request_id,
            )
            .await
        } else {
            Err("consultation recovery runtime was not activated".to_string())
        };
        task_runtime.mark_completed(&task_owner);
        let _ = result_tx.send(result);
    });
    permit.commit(task, activate_tx)?;
    result_rx
        .await
        .map_err(|_| "consultation recovery stopped before returning".to_string())?
}

pub async fn reconcile_after_restart(
    db: Arc<Mutex<TranscriptStore>>,
) -> Result<Vec<ConsultationWorkOrder>, String> {
    consultation_broker::reconcile_after_restart(db).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_and_provider_bindings_are_stable_and_separate() {
        assert_ne!(
            stable_profile_id(ConsultationProvider::ChatGpt),
            stable_profile_id(ConsultationProvider::Qwen)
        );
        assert_ne!(
            stable_provider_config_id(ConsultationProvider::ChatGpt),
            stable_provider_config_id(ConsultationProvider::Qwen)
        );
    }
}
