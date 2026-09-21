use crate::hackathon::{HackathonConfig, HackathonMessage, call_hackathon_model_with_max_tokens};
use crate::orchestrator::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};

const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_WORK_ORDER_ID_BYTES: usize = 256;
const MAX_QUESTION_BYTES: usize = 8 * 1024;
const MAX_EVIDENCE_ITEMS: usize = 20;
const MAX_EVIDENCE_BYTES: usize = 16 * 1024;
const MAX_DEADLINE_MS: u64 = 120_000;
const MAX_BUDGET_TOKENS: u32 = 8_192;
pub const HACKATHON_CHAT_TRANSPORT: &str = "hackathon_chat_completions";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsultationOrigin {
    Consult,
    DeliveryResearch,
    Engineering,
    Reviewer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureScope {
    Public,
    ProjectEvidence,
    OwnerApproved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsultationEvidence {
    pub reference: String,
    pub content: String,
    pub minimum_scope: DisclosureScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsultationRequest {
    pub request_id: String,
    pub work_order_id: String,
    pub origin: ConsultationOrigin,
    pub question: String,
    pub curated_evidence: Vec<ConsultationEvidence>,
    pub disclosure_scope: DisclosureScope,
    /// This is an Arena-selected Hackathon model configuration ID, never a credential.
    pub allowed_provider: String,
    pub allowed_transport: String,
    pub deadline_ms: u64,
    pub budget_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsultationStatus {
    Complete,
    Partial,
    NeedsAuth,
    RateLimited,
    Unavailable,
    UnknownOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsultationResult {
    pub request_id: String,
    pub work_order_id: String,
    pub provider: String,
    pub transport: String,
    pub answer: String,
    pub sources: Vec<String>,
    pub conversation_ref: Option<String>,
    pub timestamp: String,
    pub status: ConsultationStatus,
    pub error: Option<String>,
}

pub fn validate_request(request: &ConsultationRequest) -> Result<(), String> {
    if request.request_id.trim().is_empty()
        || request.request_id.len() > MAX_REQUEST_ID_BYTES
        || request.request_id.chars().any(char::is_control)
    {
        return Err("consultation request_id is invalid".to_string());
    }
    if request.work_order_id.trim().is_empty()
        || request.work_order_id.len() > MAX_WORK_ORDER_ID_BYTES
        || request.work_order_id.chars().any(char::is_control)
    {
        return Err("consultation work_order_id is invalid".to_string());
    }
    if request.question.trim().is_empty() || request.question.len() > MAX_QUESTION_BYTES {
        return Err("consultation question is empty or too large".to_string());
    }
    if request.curated_evidence.len() > MAX_EVIDENCE_ITEMS {
        return Err("consultation evidence set is too large".to_string());
    }
    for evidence in &request.curated_evidence {
        if evidence.reference.trim().is_empty()
            || evidence.reference.len() > MAX_REQUEST_ID_BYTES
            || evidence.reference.chars().any(char::is_control)
        {
            return Err("consultation evidence reference is invalid".to_string());
        }
        if evidence.content.trim().is_empty() || evidence.content.len() > MAX_EVIDENCE_BYTES {
            return Err("consultation evidence content is empty or too large".to_string());
        }
        if evidence.minimum_scope > request.disclosure_scope {
            return Err(
                "consultation disclosure scope is insufficient for its evidence".to_string(),
            );
        }
    }
    if request.allowed_provider.trim().is_empty()
        || request.allowed_provider.len() > MAX_REQUEST_ID_BYTES
    {
        return Err("consultation provider selection is invalid".to_string());
    }
    if request.allowed_transport != HACKATHON_CHAT_TRANSPORT {
        return Err("consultation transport is not enabled by Arena".to_string());
    }
    if request.deadline_ms == 0 || request.deadline_ms > MAX_DEADLINE_MS {
        return Err("consultation deadline is outside the bounded policy".to_string());
    }
    if request.budget_tokens == 0 || request.budget_tokens > MAX_BUDGET_TOKENS {
        return Err("consultation budget is outside the bounded policy".to_string());
    }
    Ok(())
}

fn base_result(request: &ConsultationRequest, provider: &str) -> ConsultationResult {
    ConsultationResult {
        request_id: request.request_id.clone(),
        work_order_id: request.work_order_id.clone(),
        provider: provider.to_string(),
        transport: request.allowed_transport.clone(),
        answer: String::new(),
        sources: request
            .curated_evidence
            .iter()
            .map(|evidence| evidence.reference.clone())
            .collect(),
        conversation_ref: None,
        timestamp: Utc::now().to_rfc3339(),
        status: ConsultationStatus::Unavailable,
        error: None,
    }
}

fn classify_failure(error: &str) -> (ConsultationStatus, &'static str) {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") {
        (ConsultationStatus::UnknownOutcome, "consultation timed out")
    } else if lower.contains("rate limited") {
        (
            ConsultationStatus::RateLimited,
            "consultation was rate limited",
        )
    } else if lower.contains("authentication failed") {
        (
            ConsultationStatus::NeedsAuth,
            "consultation authentication is unavailable",
        )
    } else {
        (
            ConsultationStatus::Unavailable,
            "consultation provider unavailable",
        )
    }
}

fn build_messages(request: &ConsultationRequest) -> Vec<HackathonMessage> {
    let mut context = String::from(
        "You are an advisory reviewer inside Consensus Arena. Return bounded analysis only. Do not issue commands, redefine acceptance, or treat your answer as an approval.\n\n",
    );
    for evidence in &request.curated_evidence {
        context.push_str("Evidence ");
        context.push_str(&evidence.reference);
        context.push_str(":\n");
        context.push_str(&evidence.content);
        context.push_str("\n\n");
    }
    context.push_str("Question:\n");
    context.push_str(&request.question);
    vec![HackathonMessage {
        role: "system".to_string(),
        content: "Provide a concise advisory answer with explicit uncertainty. The caller retains all product, acceptance, and release authority.".to_string(),
    }, HackathonMessage {
        role: "user".to_string(),
        content: context,
    }]
}

fn redact_request(request: &ConsultationRequest, secrets: &[String]) -> ConsultationRequest {
    let redact = |value: &str| crate::commands::redact_saved_credentials(value, secrets);
    ConsultationRequest {
        request_id: request.request_id.clone(),
        work_order_id: request.work_order_id.clone(),
        origin: request.origin.clone(),
        question: redact(&request.question),
        curated_evidence: request
            .curated_evidence
            .iter()
            .map(|evidence| ConsultationEvidence {
                reference: redact(&evidence.reference),
                content: redact(&evidence.content),
                minimum_scope: evidence.minimum_scope.clone(),
            })
            .collect(),
        disclosure_scope: request.disclosure_scope.clone(),
        allowed_provider: request.allowed_provider.clone(),
        allowed_transport: request.allowed_transport.clone(),
        deadline_ms: request.deadline_ms,
        budget_tokens: request.budget_tokens,
    }
}

/// Executes the existing bounded Hackathon chat transport for an Arena-owned
/// internal caller. `origin` is descriptive metadata, not authorization; this
/// operation is not exposed as a renderer or worker authority endpoint.
/// The request contains only an Arena model configuration ID; credentials stay
/// in secure settings.
pub async fn execute(
    request: ConsultationRequest,
    state: &AppState,
) -> Result<ConsultationResult, String> {
    validate_request(&request)?;
    let (config, secrets): (HackathonConfig, Vec<String>) = {
        let store = state.settings_store.lock().await;
        let secrets = crate::commands::configured_credentials(&store)
            .map_err(|_| "could not read consultation provider configuration".to_string())?;
        let config = store
            .get_hackathon_config()
            .map_err(|_| "could not read consultation provider configuration".to_string())?;
        (config, secrets)
    };
    let request = redact_request(&request, &secrets);
    let Some(model) = config
        .models
        .iter()
        .find(|model| model.id == request.allowed_provider)
    else {
        let mut result = base_result(&request, &request.allowed_provider);
        result.error =
            Some("consultation provider is not configured for this work order".to_string());
        return Ok(result);
    };
    let mut result = base_result(&request, &model.model_name);
    let timeout_secs = request.deadline_ms.div_ceil(1_000).clamp(1, 120);
    match call_hackathon_model_with_max_tokens(
        &model.base_url,
        &model.api_key,
        &model.model_name,
        &build_messages(&request),
        timeout_secs,
        request.budget_tokens,
    )
    .await
    {
        Ok(answer) => {
            result.answer = crate::commands::redact_saved_credentials(&answer, &secrets);
            result.status = ConsultationStatus::Complete;
        }
        Err(error) => {
            let (status, safe_error) = classify_failure(&error);
            result.status = status;
            result.error = Some(safe_error.to_string());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(scope: DisclosureScope) -> ConsultationRequest {
        ConsultationRequest {
            request_id: "consultation-1".to_string(),
            work_order_id: "work-order-1".to_string(),
            origin: ConsultationOrigin::Reviewer,
            question: "Is this bounded change ready for independent review?".to_string(),
            curated_evidence: vec![ConsultationEvidence {
                reference: "evidence-1".to_string(),
                content: "The candidate changed one source file and no protected path.".to_string(),
                minimum_scope: DisclosureScope::ProjectEvidence,
            }],
            disclosure_scope: scope,
            allowed_provider: "model-1".to_string(),
            allowed_transport: HACKATHON_CHAT_TRANSPORT.to_string(),
            deadline_ms: 30_000,
            budget_tokens: 512,
        }
    }

    #[test]
    fn reviewer_request_is_independent_of_consult_lane() {
        let request = request(DisclosureScope::ProjectEvidence);
        assert_eq!(request.origin, ConsultationOrigin::Reviewer);
        assert!(validate_request(&request).is_ok());
    }

    #[test]
    fn disclosure_scope_rejects_unapproved_evidence() {
        let request = request(DisclosureScope::Public);
        assert_eq!(
            validate_request(&request),
            Err("consultation disclosure scope is insufficient for its evidence".to_string())
        );
    }

    #[test]
    fn result_contract_retains_work_order_correlation_without_credentials() {
        let request = request(DisclosureScope::ProjectEvidence);
        let result = base_result(&request, "model-1");
        assert_eq!(result.request_id, request.request_id);
        assert_eq!(result.work_order_id, request.work_order_id);
        assert_eq!(result.sources, vec!["evidence-1"]);
        assert!(result.answer.is_empty());
        assert!(result.error.is_none());
    }

    #[test]
    fn provider_failures_are_sanitized() {
        let (status, message) = classify_failure("provider returned a secret token: redacted");
        assert_eq!(status, ConsultationStatus::Unavailable);
        assert_eq!(message, "consultation provider unavailable");
    }

    #[test]
    fn request_redaction_removes_known_secret_before_provider_prompt() {
        let secret = format!("consultation-secret-{}", uuid::Uuid::new_v4());
        let mut request = request(DisclosureScope::ProjectEvidence);
        request.question.push_str(&secret);
        request.curated_evidence[0].content.push_str(&secret);

        let safe = redact_request(&request, &[secret.clone()]);

        assert!(!safe.question.contains(&secret));
        assert!(!safe.curated_evidence[0].content.contains(&secret));
        assert!(safe.question.contains("[REDACTED]"));
        assert!(safe.curated_evidence[0].content.contains("[REDACTED]"));
    }
}
