//! Qualified external-browser mechanics for consultation.
//!
//! This module never owns consultation authority. The broker must persist
//! Staged/Armed and provide an in-memory ArmedSendPermit before submit_once
//! may perform the single physical Send gesture.

use crate::consultation_broker::{
    ArmedSendPermit, ConsultationObservation, ConsultationProvider, ConsultationTransportKind,
    ConsultationWorkOrder, ConversationAvailability, TransportSubmissionOutcome,
    validate_application_url,
};
use crate::dsh_worker;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const AGENT_BROWSER_VERSION: &str = "0.38.1";
pub const AGENT_BROWSER_ADAPTER_VERSION: &str = "arena-agent-browser-v1";
const COMMAND_TIMEOUT_SECONDS: u64 = 30;
const MAX_BROWSER_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_PROMPT_BYTES: usize = 48 * 1024;
const MAX_OBSERVATION_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBrowserRuntimeStatus {
    pub executable: String,
    pub expected_version: String,
    pub observed_version: Option<String>,
    pub compatible: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendGesture {
    ClickRef,
    PressEnter,
}

#[derive(Debug, Clone)]
pub struct StagedBrowserSubmission {
    pub request_id: String,
    pub session_id: String,
    pub profile_path: PathBuf,
    pub current_url: String,
    pub provider_conversation_id: Option<String>,
    prompt_marker: String,
    send_gesture: SendGesture,
    send_ref: Option<String>,
}

#[derive(Debug, Clone)]
struct BrowserCommandOutput {
    stdout: String,
    stderr: String,
}

fn executable() -> PathBuf {
    std::env::var_os("ARENA_AGENT_BROWSER_EXECUTABLE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("agent-browser"))
}

fn bounded_text(value: String) -> Result<String, String> {
    if value.len() > MAX_BROWSER_OUTPUT_BYTES {
        return Err("agent-browser output exceeded the Arena bound".to_string());
    }
    Ok(value)
}

async fn command(
    cwd: &Path,
    session_id: &str,
    profile_path: &Path,
    args: &[&str],
) -> Result<BrowserCommandOutput, String> {
    let mut full = vec![
        OsString::from("--session"),
        OsString::from(session_id),
        OsString::from("--profile"),
        profile_path.as_os_str().to_os_string(),
    ];
    full.extend(args.iter().map(OsString::from));
    let result = dsh_worker::run_contained_command(
        &executable(),
        &full,
        cwd,
        Duration::from_secs(COMMAND_TIMEOUT_SECONDS),
    )
    .await?;
    if result.timed_out {
        return Err("agent-browser command timed out".to_string());
    }
    if result.exit_code != Some(0) {
        let diagnostic = result.stderr.chars().take(512).collect::<String>();
        return Err(format!("agent-browser command failed: {diagnostic}"));
    }
    Ok(BrowserCommandOutput {
        stdout: bounded_text(result.stdout)?,
        stderr: bounded_text(result.stderr)?,
    })
}

pub async fn runtime_status(cwd: &Path) -> AgentBrowserRuntimeStatus {
    let result = dsh_worker::run_contained_command(
        &executable(),
        &[OsString::from("--version")],
        cwd,
        Duration::from_secs(10),
    )
    .await;
    match result {
        Ok(output) if !output.timed_out && output.exit_code == Some(0) => {
            let combined = format!("{}\n{}", output.stdout, output.stderr);
            let observed = combined
                .split_whitespace()
                .map(|token| token.trim_start_matches('v'))
                .find(|token| token.chars().next().is_some_and(|value| value.is_ascii_digit()))
                .map(ToString::to_string);
            let compatible = observed.as_deref() == Some(AGENT_BROWSER_VERSION);
            AgentBrowserRuntimeStatus {
                executable: executable().to_string_lossy().into_owned(),
                expected_version: AGENT_BROWSER_VERSION.to_string(),
                observed_version: observed.clone(),
                compatible,
                message: if compatible {
                    format!("qualified agent-browser {AGENT_BROWSER_VERSION} is available")
                } else {
                    format!(
                        "agent-browser version mismatch; expected {AGENT_BROWSER_VERSION}, observed {:?}",
                        observed
                    )
                },
            }
        }
        Ok(_) => AgentBrowserRuntimeStatus {
            executable: executable().to_string_lossy().into_owned(),
            expected_version: AGENT_BROWSER_VERSION.to_string(),
            observed_version: None,
            compatible: false,
            message: "agent-browser version probe did not complete successfully".to_string(),
        },
        Err(error) => AgentBrowserRuntimeStatus {
            executable: executable().to_string_lossy().into_owned(),
            expected_version: AGENT_BROWSER_VERSION.to_string(),
            observed_version: None,
            compatible: false,
            message: format!("agent-browser is unavailable: {error}"),
        },
    }
}

fn safe_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(80)
        .collect()
}

fn request_marker(request_id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(request_id.as_bytes()));
    format!("[Arena request {}]", &digest[..16])
}

fn page_classification(url: &str, snapshot: &str) -> ConversationAvailability {
    let combined = format!("{url}\n{snapshot}").to_ascii_lowercase();
    if combined.contains("captcha")
        || combined.contains("challenge")
        || combined.contains("verify you are human")
    {
        ConversationAvailability::Challenge
    } else if combined.contains("/login")
        || combined.contains("/auth")
        || combined.contains("log in")
        || combined.contains("sign in")
    {
        ConversationAvailability::NeedsAuth
    } else {
        ConversationAvailability::Available
    }
}

fn element_ref(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|token| {
            token.starts_with("@e")
                && token
                    .get(2..)
                    .is_some_and(|value| value.chars().all(|c| c.is_ascii_digit()))
        })
        .map(ToString::to_string)
}

fn composer_ref(snapshot: &str) -> Option<String> {
    snapshot
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("[textbox")
                || lower.contains("[textarea")
                || lower.contains("contenteditable")
        })
        .filter_map(element_ref)
        .last()
}

fn send_button_ref(snapshot: &str) -> Option<String> {
    snapshot.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        (lower.contains("[button") && (lower.contains("send") || lower.contains("submit")))
            .then(|| element_ref(line))
            .flatten()
    })
}

fn provider_conversation_id(provider: ConsultationProvider, url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if parsed.host_str()? != provider.expected_host() {
        return None;
    }
    let segments = parsed
        .path_segments()?
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    match provider {
        ConsultationProvider::ChatGpt => {
            (segments.len() >= 2 && segments[0] == "c").then(|| segments[1].to_string())
        }
        ConsultationProvider::Qwen => {
            (segments.len() >= 2 && segments[0] == "chat").then(|| segments[1].to_string())
        }
    }
}

fn bounded_profile_path(profile_root: &Path, order: &ConsultationWorkOrder) -> Result<PathBuf, String> {
    if !profile_root.is_absolute() {
        return Err("Arena consultation profile root must be absolute".to_string());
    }
    let provider = match order.provider {
        ConsultationProvider::ChatGpt => "chatgpt",
        ConsultationProvider::Qwen => "qwen",
    };
    Ok(profile_root
        .join(provider)
        .join(safe_component(&order.profile_id)))
}

pub async fn stage(
    cwd: &Path,
    profile_root: &Path,
    order: &ConsultationWorkOrder,
    prompt: &str,
    anchor_url: Option<&str>,
) -> Result<StagedBrowserSubmission, String> {
    if order.transport != ConsultationTransportKind::ExternalBrowserAgent {
        return Err("external browser cannot stage a different transport".to_string());
    }
    if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT_BYTES {
        return Err("consultation prompt is empty or exceeds the browser bound".to_string());
    }
    let status = runtime_status(cwd).await;
    if !status.compatible {
        return Err(status.message);
    }
    let profile_path = bounded_profile_path(profile_root, order)?;
    std::fs::create_dir_all(&profile_path)
        .map_err(|error| format!("create Arena consultation browser profile: {error}"))?;
    let session_id = format!("arena-consult-{}", safe_component(&order.request_id));
    let target = match anchor_url {
        Some(url) => validate_application_url(order.provider, url, true)?,
        None => order.provider.home_url().to_string(),
    };
    command(cwd, &session_id, &profile_path, &["open", target.as_str()]).await?;
    let current_url = command(cwd, &session_id, &profile_path, &["get", "url"])
        .await?
        .stdout
        .trim()
        .to_string();
    let snapshot = command(cwd, &session_id, &profile_path, &["snapshot", "-i"])
        .await?
        .stdout;
    match page_classification(&current_url, &snapshot) {
        ConversationAvailability::NeedsAuth => {
            return Err("consultation browser requires manual authentication".to_string())
        }
        ConversationAvailability::Challenge => {
            return Err("consultation browser encountered a provider challenge".to_string())
        }
        _ => {}
    }
    let composer = composer_ref(&snapshot)
        .ok_or_else(|| "provider composer was not found in the interactive snapshot".to_string())?;
    let marker = request_marker(&order.request_id);
    let staged_prompt = format!("{prompt}\n\n{marker}");
    command(
        cwd,
        &session_id,
        &profile_path,
        &["fill", composer.as_str(), staged_prompt.as_str()],
    )
    .await?;
    let staged_snapshot = command(cwd, &session_id, &profile_path, &["snapshot", "-i"])
        .await?
        .stdout;
    let send_ref = send_button_ref(&staged_snapshot);
    let send_gesture = if send_ref.is_some() {
        SendGesture::ClickRef
    } else {
        SendGesture::PressEnter
    };
    Ok(StagedBrowserSubmission {
        request_id: order.request_id.clone(),
        session_id,
        profile_path,
        current_url,
        provider_conversation_id: provider_conversation_id(order.provider, &target),
        prompt_marker: marker,
        send_gesture,
        send_ref,
    })
}

pub async fn submit_once(
    cwd: &Path,
    permit: &ArmedSendPermit,
    staged: &StagedBrowserSubmission,
) -> TransportSubmissionOutcome {
    if permit.transport() != ConsultationTransportKind::ExternalBrowserAgent
        || permit.request_id() != staged.request_id
    {
        return TransportSubmissionOutcome::UnknownOutcome {
            diagnostic: "external browser send permit did not match the staged request".to_string(),
        };
    }
    let result = match staged.send_gesture {
        SendGesture::ClickRef => match staged.send_ref.as_deref() {
            Some(reference) => {
                command(cwd, &staged.session_id, &staged.profile_path, &["click", reference]).await
            }
            None => Err("staged click gesture lost its send reference".to_string()),
        },
        SendGesture::PressEnter => {
            command(cwd, &staged.session_id, &staged.profile_path, &["press", "Enter"]).await
        }
    };
    match result {
        Ok(_) => {
            let url = command(cwd, &staged.session_id, &staged.profile_path, &["get", "url"])
                .await
                .ok()
                .map(|value| value.stdout.trim().to_string())
                .filter(|value| !value.is_empty());
            TransportSubmissionOutcome::Submitted { canonical_url: url }
        }
        Err(error) => TransportSubmissionOutcome::UnknownOutcome {
            diagnostic: format!(
                "the single physical Send gesture returned an uncertain outcome: {}",
                error.chars().take(384).collect::<String>()
            ),
        },
    }
}

fn extract_advisory_after_marker(rendered: &str, marker: &str) -> Option<String> {
    let start = rendered.rfind(marker)? + marker.len();
    let suffix = rendered[start..].trim();
    if suffix.len() < 8 {
        return None;
    }
    let bounded = suffix.chars().take(MAX_OBSERVATION_BYTES).collect::<String>();
    (!bounded.trim().is_empty()).then_some(bounded)
}

pub async fn observe_once(
    cwd: &Path,
    order: &ConsultationWorkOrder,
    staged: &StagedBrowserSubmission,
) -> Result<Option<ConsultationObservation>, String> {
    if order.request_id != staged.request_id
        || !order.state.is_post_arm()
        || order.transport != ConsultationTransportKind::ExternalBrowserAgent
    {
        return Err("consultation observation is not admitted for this transaction".to_string());
    }
    let _ = command(cwd, &staged.session_id, &staged.profile_path, &["wait", "1500"]).await;
    let current_url = command(cwd, &staged.session_id, &staged.profile_path, &["get", "url"])
        .await?
        .stdout
        .trim()
        .to_string();
    let rendered = command(cwd, &staged.session_id, &staged.profile_path, &["read"])
        .await?
        .stdout;
    match page_classification(&current_url, &rendered) {
        ConversationAvailability::NeedsAuth => {
            return Err("consultation observation lost provider authentication".to_string())
        }
        ConversationAvailability::Challenge => {
            return Err("consultation observation encountered a provider challenge".to_string())
        }
        _ => {}
    }
    let Some(advisory_text) = extract_advisory_after_marker(&rendered, &staged.prompt_marker) else {
        return Ok(None);
    };
    let canonical_url = validate_application_url(order.provider, &current_url, true)?;
    let assistant_turn_digest = format!("sha256:{:x}", Sha256::digest(advisory_text.as_bytes()));
    Ok(Some(ConsultationObservation {
        request_id: order.request_id.clone(),
        execution_epoch: order.execution_epoch,
        provider: order.provider,
        provider_config_id: order.provider_config_id.clone(),
        profile_id: order.profile_id.clone(),
        canonical_url,
        user_turn_digest: order.prompt_digest.clone(),
        assistant_turn_digest,
        advisory_text,
        provider_conversation_id: provider_conversation_id(order.provider, &current_url)
            .or_else(|| staged.provider_conversation_id.clone()),
        provider_branch_id: None,
    }))
}

pub async fn close_owned_session(cwd: &Path, staged: &StagedBrowserSubmission) {
    let _ = command(cwd, &staged.session_id, &staged.profile_path, &["close"]).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_origins_and_conversation_ids_are_exact() {
        assert_eq!(
            provider_conversation_id(
                ConsultationProvider::ChatGpt,
                "https://chatgpt.com/c/abc-123"
            )
            .as_deref(),
            Some("abc-123")
        );
        assert_eq!(
            provider_conversation_id(
                ConsultationProvider::Qwen,
                "https://chat.qwen.ai/chat/qwen-thread"
            )
            .as_deref(),
            Some("qwen-thread")
        );
        assert!(provider_conversation_id(
            ConsultationProvider::ChatGpt,
            "https://chatgpt.com.evil.example/c/abc"
        )
        .is_none());
    }

    #[test]
    fn snapshot_parser_prefers_last_composer_and_explicit_send_button() {
        let snapshot = "@e1 [textbox] Search\n@e2 [textbox] Message\n@e3 [button] \"Send\"";
        assert_eq!(composer_ref(snapshot).as_deref(), Some("@e2"));
        assert_eq!(send_button_ref(snapshot).as_deref(), Some("@e3"));
    }

    #[test]
    fn marker_binds_rendered_observation() {
        let rendered = "User question\n[Arena request abc]\nAssistant answer";
        assert_eq!(
            extract_advisory_after_marker(rendered, "[Arena request abc]").as_deref(),
            Some("Assistant answer")
        );
        assert!(extract_advisory_after_marker("unrelated", "[Arena request abc]").is_none());
    }
}
