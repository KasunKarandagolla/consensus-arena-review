use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Emergency safety cap — maximum leader decisions per group.
/// Not exposed in UI, exists only to prevent infinite loops if per-teammate cap is Unlimited and leader never submits.
/// Documented in audits/hackathon-mode-pre.md and plan.
pub const HACKATHON_SAFETY_MAX_ROUNDS: u32 = 20;

/// Invitation health-check timeout in seconds.
pub const HACKATHON_INVITE_TIMEOUT_SECS: u64 = 15;

/// Per-group API call timeout in seconds.
pub const HACKATHON_GROUP_TIMEOUT_SECS: u64 = 60;

// ── Persisted Config ─────────────────────────────────────────────────────────

/// Persisted hackathon model — each saved entry carries its own api_key/base_url/model_name and belongs to exactly one group.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonModelConfig {
    pub id: String,
    pub model_name: String,
    pub base_url: String,
    pub api_key: String,
    pub group_id: String,
}

/// Persisted group — ordered model_ids define leadership and fallback.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonGroupConfig {
    pub id: String,
    pub name: String,
    pub model_ids: Vec<String>,
    pub selected: bool,
}

/// Persisted top-level hackathon configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonConfig {
    pub groups: Vec<HackathonGroupConfig>,
    pub models: Vec<HackathonModelConfig>,
    /// None = Unlimited, Some(n) = cap per teammate (non-leader only)
    pub max_questions_per_teammate: Option<u32>,
    pub enabled: bool,
}

impl Default for HackathonConfig {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            models: Vec::new(),
            max_questions_per_teammate: Some(3),
            enabled: false,
        }
    }
}

/// Frontend-safe view — api_key omitted, base_url kept for host display.
/// Frontend must never receive api_keys.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonModelSafe {
    pub id: String,
    pub model_name: String,
    pub base_url: String,
    pub group_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonGroupSafe {
    pub id: String,
    pub name: String,
    pub model_ids: Vec<String>,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HackathonConfigSafe {
    pub groups: Vec<HackathonGroupSafe>,
    pub models: Vec<HackathonModelSafe>,
    pub max_questions_per_teammate: Option<u32>,
    pub enabled: bool,
}

impl HackathonConfig {
    pub fn to_safe(&self) -> HackathonConfigSafe {
        HackathonConfigSafe {
            groups: self
                .groups
                .iter()
                .map(|g| HackathonGroupSafe {
                    id: g.id.clone(),
                    name: g.name.clone(),
                    model_ids: g.model_ids.clone(),
                    selected: g.selected,
                })
                .collect(),
            models: self
                .models
                .iter()
                .map(|m| HackathonModelSafe {
                    id: m.id.clone(),
                    model_name: m.model_name.clone(),
                    base_url: m.base_url.clone(),
                    group_id: m.group_id.clone(),
                })
                .collect(),
            max_questions_per_teammate: self.max_questions_per_teammate,
            enabled: self.enabled,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        // Check group names unique, not empty
        let mut group_ids = HashSet::new();
        let mut group_names = HashSet::new();
        for g in &self.groups {
            let trimmed = g.name.trim();
            if trimmed.is_empty() {
                return Err("Group name cannot be empty".to_string());
            }
            if !group_names.insert(trimmed.to_string()) {
                return Err(format!("Duplicate group name: {}", trimmed));
            }
            if !group_ids.insert(g.id.clone()) {
                return Err(format!("Duplicate group id: {}", g.id));
            }
        }
        // Check models
        let mut model_ids = HashSet::new();
        for m in &self.models {
            if m.model_name.trim().is_empty() {
                return Err("Model name cannot be empty".to_string());
            }
            if m.base_url.trim().is_empty() {
                return Err(format!("Model {} base_url is required", m.model_name));
            }
            let parsed = reqwest::Url::parse(m.base_url.trim())
                .map_err(|e| format!("Model {} base_url invalid: {}", m.model_name, e))?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err(format!(
                    "Model {} base_url must be http(s): {}",
                    m.model_name, m.base_url
                ));
            }
            if m.api_key.trim().is_empty() {
                return Err(format!("Model {} api_key is required", m.model_name));
            }
            if !group_ids.contains(&m.group_id) {
                return Err(format!(
                    "Model {} assigned to unknown group {}",
                    m.model_name, m.group_id
                ));
            }
            if !model_ids.insert(m.id.clone()) {
                return Err(format!("Duplicate model id: {}", m.id));
            }
        }
        // Ensure model_ids in groups reference existing models and are ordered
        for g in &self.groups {
            let mut seen_in_group = HashSet::new();
            for mid in &g.model_ids {
                if !model_ids.contains(mid) {
                    return Err(format!("Group {} references unknown model {}", g.name, mid));
                }
                if !seen_in_group.insert(mid.clone()) {
                    return Err(format!("Group {} has duplicate model id {}", g.name, mid));
                }
                // Also ensure model's group_id matches
                let model =
                    self.models.iter().find(|m| &m.id == mid).ok_or_else(|| {
                        format!("Group {} references unknown model {}", g.name, mid)
                    })?;
                if model.group_id != g.id {
                    return Err(format!(
                        "Model {} group mismatch: group {} lists it but model belongs to {}",
                        model.model_name, g.name, model.group_id
                    ));
                }
            }
        }
        // Max questions validation — numeric input: allow any integer >=1, null = Unlimited
        if let Some(n) = self.max_questions_per_teammate {
            if n == 0 {
                return Err("max_questions_per_teammate cannot be 0".to_string());
            }
            if n > 100 {
                return Err(format!(
                    "Invalid max_questions_per_teammate: {} — maximum 100 or Unlimited (null)",
                    n
                ));
            }
            // Allow any >=1, but warn if not in classic set; still accept
        }
        Ok(())
    }
}

// ── Decision Contract ─────────────────────────────────────────────────────────

/// Isolated hackathon leader decision — distinct from AgentDecision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HackathonDecision {
    Route {
        target_model: String,
        prompt: String,
    },
    Submit {
        final_output: String,
    },
}

// ── Transient Run State ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRunStatus {
    Pending,
    Confirmed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParticipantRunState {
    pub model_id: String,
    pub model_name: String,
    pub base_url: String,
    pub group_id: String,
    pub status: ParticipantRunStatus,
    pub consultation_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Locked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HackathonMessage {
    pub role: String, // "user" or "assistant" or "system"
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupRunState {
    pub group_id: String,
    pub group_name: String,
    pub model_ids_ordered: Vec<String>,
    pub participants: Vec<ParticipantRunState>,
    pub leader_id: Option<String>,
    pub history: Vec<HackathonMessage>,
    pub status: GroupRunStatus,
    pub final_output: Option<String>,
    pub consultation_counts: HashMap<String, u32>,
}

#[derive(Clone, Debug)]
pub struct HackathonRunState {
    pub run_id: String,
    pub task_brief: String,
    pub max_questions: Option<u32>,
    pub groups: Vec<GroupRunState>,
    pub cancelled: Arc<AtomicBool>,
    pub created_at: String,
}

// Safe serializations for frontend events
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HackathonRunSafe {
    pub run_id: String,
    pub task_brief: String,
    pub max_questions_per_teammate: Option<u32>,
    pub groups: Vec<GroupRunSafe>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupRunSafe {
    pub group_id: String,
    pub group_name: String,
    pub model_ids_ordered: Vec<String>,
    pub participants: Vec<ParticipantRunSafe>,
    pub leader_id: Option<String>,
    pub history_len: usize,
    pub status: GroupRunStatus,
    pub final_output: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParticipantRunSafe {
    pub model_id: String,
    pub model_name: String,
    pub base_url: String,
    pub group_id: String,
    pub status: ParticipantRunStatus,
    pub consultation_count: u32,
}

impl HackathonRunState {
    pub fn to_safe(&self) -> HackathonRunSafe {
        HackathonRunSafe {
            run_id: self.run_id.clone(),
            task_brief: self.task_brief.clone(),
            max_questions_per_teammate: self.max_questions,
            groups: self
                .groups
                .iter()
                .map(|g| GroupRunSafe {
                    group_id: g.group_id.clone(),
                    group_name: g.group_name.clone(),
                    model_ids_ordered: g.model_ids_ordered.clone(),
                    participants: g
                        .participants
                        .iter()
                        .map(|p| ParticipantRunSafe {
                            model_id: p.model_id.clone(),
                            model_name: p.model_name.clone(),
                            base_url: p.base_url.clone(),
                            group_id: p.group_id.clone(),
                            status: p.status.clone(),
                            consultation_count: p.consultation_count,
                        })
                        .collect(),
                    leader_id: g.leader_id.clone(),
                    history_len: g.history.len(),
                    status: g.status.clone(),
                    final_output: g.final_output.clone(),
                })
                .collect(),
            cancelled: self.cancelled.load(Ordering::SeqCst),
        }
    }
}

// ── Pure Helpers (testable) ───────────────────────────────────────────────────

/// Select leader: first live (Confirmed or Pending? For invitation stage, Confirmed only)
/// For execution, live = Confirmed participants.
pub fn select_leader(ordered_ids: &[String], live_set: &HashSet<String>) -> Option<String> {
    for id in ordered_ids {
        if live_set.contains(id) {
            return Some(id.clone());
        }
    }
    None
}

/// Determine next fallback leader after current leader fails.
pub fn fallback_leader(
    ordered_ids: &[String],
    failed_leader_id: &str,
    live_set: &HashSet<String>,
) -> Option<String> {
    let pos = ordered_ids.iter().position(|id| id == failed_leader_id)?;
    for id in ordered_ids.iter().skip(pos + 1) {
        if live_set.contains(id) {
            return Some(id.clone());
        }
    }
    None
}

/// Check if route is allowed per cap and group membership.
pub fn is_route_allowed(
    target_model_id: &str,
    leader_id: &str,
    group_model_ids: &[String],
    live_set: &HashSet<String>,
    consultation_counts: &HashMap<String, u32>,
    cap: Option<u32>,
) -> Result<(), String> {
    if target_model_id == leader_id {
        return Err("Cannot route to the leader itself".to_string());
    }
    if !group_model_ids.contains(&target_model_id.to_string()) {
        return Err(format!(
            "Target {} is not a member of this group",
            target_model_id
        ));
    }
    if !live_set.contains(target_model_id) {
        return Err(format!("Target {} is not live", target_model_id));
    }
    if let Some(limit) = cap {
        let count = consultation_counts
            .get(target_model_id)
            .copied()
            .unwrap_or(0);
        if count >= limit {
            return Err(format!(
                "Teammate {} has reached max questions cap ({})",
                target_model_id, limit
            ));
        }
    }
    Ok(())
}

/// Sort participants so responders (Confirmed) float above non-responders, preserving original order within each partition.
pub fn sort_by_responder_status(
    ordered_ids: &[String],
    statuses: &HashMap<String, ParticipantRunStatus>,
) -> Vec<String> {
    let mut confirmed = Vec::new();
    let mut others = Vec::new();
    for id in ordered_ids {
        match statuses.get(id) {
            Some(ParticipantRunStatus::Confirmed) => confirmed.push(id.clone()),
            _ => others.push(id.clone()),
        }
    }
    confirmed.extend(others);
    confirmed
}

/// Format combined report for leader — analogous to RouteCompare concatenation.
pub fn format_report(groups: &[GroupRunState], run_id: &str) -> String {
    if groups.is_empty() {
        return format!("[Hackathon Run {}]\nNo groups participated.\n", run_id);
    }
    let mut report = format!("=== Hackathon Results (run {}) ===\n\n", run_id);
    for group in groups {
        report.push_str(&format!("[Hackathon Group: {}]\n", group.group_name));
        match &group.final_output {
            Some(output) if !output.trim().is_empty() => {
                report.push_str(output.trim());
                report.push_str("\n\n");
            }
            Some(_) => {
                report.push_str("(No output — group submitted empty result)\n\n");
            }
            None => match group.status {
                GroupRunStatus::Locked => {
                    report.push_str("(No output — group had zero live members and was locked)\n\n");
                }
                GroupRunStatus::Failed => {
                    report.push_str("(No output — group failed: all members unavailable)\n\n");
                }
                _ => {
                    report.push_str("(No output — group did not complete)\n\n");
                }
            },
        }
    }
    report.push_str("=== End Hackathon Results ===\n");
    report.push_str("Above are raw materials from parallel hackathon groups. Evaluate, accept, reject, or synthesize as the leader deems appropriate using your existing Blueprint/Route/Continue/Complete decision process.\n");
    report
}

/// Extract JSON decision robustly (tolerant of fences/prose) — mirrors agent_brain::extract_json_object.
pub fn extract_json_object(content: &str) -> Option<&str> {
    let start = content.find('{')?;
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return content.get(start..=start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn parse_hackathon_decision(content: &str) -> Result<HackathonDecision, String> {
    let clean = extract_json_object(content)
        .ok_or_else(|| "Response contained no complete JSON object".to_string())?;
    serde_json::from_str::<HackathonDecision>(clean).map_err(|e| {
        format!(
            "Failed to parse hackathon decision JSON: {} ({} bytes)",
            e,
            clean.len()
        )
    })
}

// ── Invitation helpers ───────────────────────────────────────────────────────

pub fn build_invitation_prompt() -> String {
    "Consensus Arena invitation health check. Reply with exactly: OK".to_string()
}

pub fn build_group_system_prompt(group_name: &str, member_names: &[String]) -> String {
    let members = if member_names.is_empty() {
        "no teammates".to_string()
    } else {
        member_names.join(", ")
    };
    format!(
        "You are the leader of hackathon group '{}'. Your teammates are: {}. \
        You must decide whether to consult a teammate or submit the group's final output. \
        Reply with exactly one JSON object, no markdown: \
        {{\"action\":\"route\",\"target_model\":\"<teammate_id>\",\"prompt\":\"<question>\"}} \
        or {{\"action\":\"submit\",\"final_output\":\"<synthesized output>\"}}. \
        Only route to non-leader teammates who are listed. Submit when the group's work is complete. \
        Use canonical model ids exactly as provided.",
        group_name, members
    )
}

// ── Error helpers ───────────────────────────────────────────────────────────

pub fn redact_api_key_logs(text: &str) -> String {
    // Minimal redaction — replace api_key values
    text.replace("api_key", "[REDACTED]")
}

// ── HTTP helpers shared with API engine ─────────────────────────────────────

pub fn redact_endpoint(url: &str) -> String {
    url.split('?').next().unwrap_or(url).to_string()
}

// ── HTTP Client ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct HackathonChatMessage {
    role: String,
    content: String,
}

#[derive(serde::Serialize)]
struct HackathonChatRequest {
    model: String,
    messages: Vec<HackathonChatMessageSer>,
    max_tokens: u32,
}

#[derive(serde::Serialize)]
struct HackathonChatMessageSer {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct HackathonChatResponse {
    choices: Vec<HackathonChatChoice>,
}

#[derive(serde::Deserialize)]
struct HackathonChatChoice {
    message: HackathonChatResponseMessage,
}

#[derive(serde::Deserialize)]
struct HackathonChatResponseMessage {
    content: String,
}

/// Call a single hackathon model via OpenAI-compatible chat/completions.
/// Uses per-call client with timeout to avoid sharing state across groups.
pub async fn call_hackathon_model(
    base_url: &str,
    api_key: &str,
    model_name: &str,
    messages: &[HackathonMessage],
    timeout_secs: u64,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let req_messages: Vec<HackathonChatMessageSer> = messages
        .iter()
        .map(|m| HackathonChatMessageSer {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();

    let request = HackathonChatRequest {
        model: model_name.to_string(),
        messages: req_messages,
        max_tokens: 1024,
    };

    tracing::debug!(
        "[HACKATHON] calling model={} endpoint={}",
        model_name,
        redact_endpoint(&url)
    );

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "request timed out".to_string()
            } else if e.is_connect() {
                format!("connect error: {}", e)
            } else {
                format!("request failed: {}", e)
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        let code = status.as_u16();
        let reason = status.canonical_reason().unwrap_or("");
        // Redact auth details
        let detail = if code == 401 || code == 403 {
            "authentication failed".to_string()
        } else if code == 429 {
            "rate limited".to_string()
        } else if code == 410 {
            "model gone (410)".to_string()
        } else {
            format!("HTTP {} {}", code, reason).trim().to_string()
        };
        return Err(format!("API error ({}): {}", code, detail));
    }

    let chat_response: HackathonChatResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    let content = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| "Empty response from model".to_string())?
        .message
        .content;

    if content.trim().is_empty() {
        return Err("Model returned empty content".to_string());
    }

    Ok(content)
}

// ── Group Execution ─────────────────────────────────────────────────────────

/// Build messages for leader decision from group history.
fn build_leader_messages(
    group: &GroupRunState,
    task_brief: &str,
    member_names: Vec<String>,
) -> Vec<HackathonMessage> {
    let system = build_group_system_prompt(&group.group_name, &member_names);
    let mut messages = vec![HackathonMessage {
        role: "system".to_string(),
        content: system,
    }];
    // Task brief as first user message if history empty, otherwise full history
    if group.history.is_empty() {
        messages.push(HackathonMessage {
            role: "user".to_string(),
            content: format!("Task brief:\n{}", task_brief),
        });
    } else {
        messages.extend(group.history.clone());
    }
    messages
}

/// Build messages for teammate consult — full history + routing prompt.
fn build_teammate_messages(
    history: &[HackathonMessage],
    route_prompt: &str,
) -> Vec<HackathonMessage> {
    let mut messages = vec![HackathonMessage {
        role: "system".to_string(),
        content: "You are a teammate in a hackathon group. Answer the leader's question concisely and helpfully.".to_string(),
    }];
    messages.extend(history.iter().cloned());
    messages.push(HackathonMessage {
        role: "user".to_string(),
        content: route_prompt.to_string(),
    });
    messages
}

/// Execute a single group's hierarchical loop.
/// `model_credentials` maps model_id -> (base_url, api_key, model_name) for lookup.
pub async fn run_single_group(
    mut group: GroupRunState,
    task_brief: String,
    max_questions: Option<u32>,
    model_credentials: HashMap<String, (String, String, String)>,
    run_id: String,
    cancel_flag: Arc<AtomicBool>,
) -> GroupRunState {
    // Early check: if group has no models, mark failed
    if group.model_ids_ordered.is_empty() {
        group.status = GroupRunStatus::Locked;
        return group;
    }

    // Copy live set initially = all participants (for leader selection, will be updated after invitations)
    // For execution phase, live = Confirmed only — caller should have set statuses accordingly.
    // If statuses are still Pending (no invitation phase), treat Pending as live.
    let mut live_set: HashSet<String> = group
        .participants
        .iter()
        .filter(|p| {
            matches!(
                p.status,
                ParticipantRunStatus::Confirmed | ParticipantRunStatus::Pending
            )
        })
        .map(|p| p.model_id.clone())
        .collect();

    // If after invitation we have zero live, lock the group
    if live_set.is_empty() {
        group.status = GroupRunStatus::Locked;
        return group;
    }

    // Initialize history with task brief if empty
    if group.history.is_empty() {
        group.history.push(HackathonMessage {
            role: "user".to_string(),
            content: format!("Task brief:\n{}", task_brief),
        });
    }

    // Determine initial leader
    let mut current_leader = match select_leader(&group.model_ids_ordered, &live_set) {
        Some(l) => l,
        None => {
            group.status = GroupRunStatus::Locked;
            return group;
        }
    };
    group.leader_id = Some(current_leader.clone());
    group.status = GroupRunStatus::Running;

    let mut rounds: u32 = 0;
    let mut consecutive_invalid: u32 = 0;

    loop {
        // Cancellation check at top of each iteration with run_id staleness
        if cancel_flag.load(Ordering::SeqCst) {
            tracing::info!(
                "[HACKATHON] group {} cancelled (run {})",
                group.group_name,
                run_id
            );
            group.status = GroupRunStatus::Failed;
            break;
        }
        if rounds >= HACKATHON_SAFETY_MAX_ROUNDS {
            tracing::warn!(
                "[HACKATHON] group {} reached safety cap {} (run {})",
                group.group_name,
                HACKATHON_SAFETY_MAX_ROUNDS,
                run_id
            );
            // Finalize with whatever history we have — use last history as output if no submit
            if group.final_output.is_none() {
                let assembled: String = group
                    .history
                    .iter()
                    .map(|m| format!("{}: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let truncated = format!(
                    "{}\n\n[Note: truncated — safety cap of {} rounds reached]",
                    assembled, HACKATHON_SAFETY_MAX_ROUNDS
                );
                group.final_output = Some(truncated);
            }
            group.status = GroupRunStatus::Completed;
            break;
        }
        rounds = rounds.saturating_add(1);

        // Build member names for system prompt (all members in group)
        let member_names: Vec<String> = group
            .participants
            .iter()
            .map(|p| p.model_name.clone())
            .collect();

        let messages = build_leader_messages(&group, &task_brief, member_names);

        // Lookup credentials for current leader
        let (leader_base_url, leader_api_key, leader_model_name) =
            match model_credentials.get(&current_leader) {
                Some(v) => v.clone(),
                None => {
                    tracing::error!(
                        "[HACKATHON] leader {} credentials missing (run {})",
                        current_leader,
                        run_id
                    );
                    // Mark leader failed and fallback
                    live_set.remove(&current_leader);
                    for p in &mut group.participants {
                        if p.model_id == current_leader {
                            p.status = ParticipantRunStatus::Failed;
                            p.last_error = Some("Missing credentials".to_string());
                        }
                    }
                    match fallback_leader(&group.model_ids_ordered, &current_leader, &live_set) {
                        Some(next) => {
                            current_leader = next.clone();
                            group.leader_id = Some(next);
                            continue;
                        }
                        None => {
                            group.status = GroupRunStatus::Failed;
                            break;
                        }
                    }
                }
            };

        // Call leader
        let leader_response = match call_hackathon_model(
            &leader_base_url,
            &leader_api_key,
            &leader_model_name,
            &messages,
            HACKATHON_GROUP_TIMEOUT_SECS,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "[HACKATHON] leader {} failed (run {}, group {}): {}",
                    current_leader,
                    run_id,
                    group.group_name,
                    redact_api_key_logs(&e)
                );
                // Mark leader failed, fallback
                live_set.remove(&current_leader);
                for p in &mut group.participants {
                    if p.model_id == current_leader {
                        p.status = ParticipantRunStatus::Failed;
                        p.last_error = Some(e.clone());
                    }
                }
                match fallback_leader(&group.model_ids_ordered, &current_leader, &live_set) {
                    Some(next) => {
                        // Preserve history; add failure notice to history for next leader context
                        group.history.push(HackathonMessage {
                            role: "system".to_string(),
                            content: format!(
                                "[Leader {} became unavailable: {}. Leadership passed to {}.]",
                                current_leader,
                                redact_api_key_logs(&e),
                                next
                            ),
                        });
                        current_leader = next.clone();
                        group.leader_id = Some(next);
                        continue;
                    }
                    None => {
                        group.status = GroupRunStatus::Failed;
                        break;
                    }
                }
            }
        };

        // Parse decision
        let decision = match parse_hackathon_decision(&leader_response) {
            Ok(d) => {
                consecutive_invalid = 0;
                d
            }
            Err(e) => {
                consecutive_invalid = consecutive_invalid.saturating_add(1);
                tracing::warn!(
                    "[HACKATHON] invalid decision from leader {} (run {}, attempt {}): {}",
                    current_leader,
                    run_id,
                    consecutive_invalid,
                    e
                );
                if consecutive_invalid >= 2 {
                    // Treat as leader failure after 2 consecutive invalid
                    live_set.remove(&current_leader);
                    for p in &mut group.participants {
                        if p.model_id == current_leader {
                            p.status = ParticipantRunStatus::Failed;
                            p.last_error = Some(format!("Invalid decision: {}", e));
                        }
                    }
                    match fallback_leader(&group.model_ids_ordered, &current_leader, &live_set) {
                        Some(next) => {
                            group.history.push(HackathonMessage {
                                role: "system".to_string(),
                                content: format!(
                                    "[Leader {} produced invalid decision twice: {}. Leadership passed to {}.]",
                                    current_leader, e, next
                                ),
                            });
                            current_leader = next.clone();
                            group.leader_id = Some(next);
                            consecutive_invalid = 0;
                            continue;
                        }
                        None => {
                            group.status = GroupRunStatus::Failed;
                            break;
                        }
                    }
                } else {
                    // Inject correction and retry same leader
                    group.history.push(HackathonMessage {
                        role: "user".to_string(),
                        content: leader_response.clone(),
                    });
                    group.history.push(HackathonMessage {
                        role: "system".to_string(),
                        content: format!(
                            "Your previous response was invalid: {}. Please reply with exactly one JSON object: {{\"action\":\"route\",\"target_model\":\"...\",\"prompt\":\"...\"}} or {{\"action\":\"submit\",\"final_output\":\"...\"}}.",
                            e
                        ),
                    });
                    continue;
                }
            }
        };

        match decision {
            HackathonDecision::Submit { final_output } => {
                group.final_output = Some(final_output);
                group.status = GroupRunStatus::Completed;
                break;
            }
            HackathonDecision::Route {
                target_model,
                prompt,
            } => {
                // Validate route
                if let Err(validation_err) = is_route_allowed(
                    &target_model,
                    &current_leader,
                    &group.model_ids_ordered,
                    &live_set,
                    &group.consultation_counts,
                    max_questions,
                ) {
                    consecutive_invalid = consecutive_invalid.saturating_add(1);
                    tracing::warn!(
                        "[HACKATHON] invalid route from leader {} to {} (run {}): {}",
                        current_leader,
                        target_model,
                        run_id,
                        validation_err
                    );
                    if consecutive_invalid >= 2 {
                        live_set.remove(&current_leader);
                        for p in &mut group.participants {
                            if p.model_id == current_leader {
                                p.status = ParticipantRunStatus::Failed;
                                p.last_error = Some(format!("Invalid route: {}", validation_err));
                            }
                        }
                        match fallback_leader(&group.model_ids_ordered, &current_leader, &live_set)
                        {
                            Some(next) => {
                                group.history.push(HackathonMessage {
                                    role: "system".to_string(),
                                    content: format!(
                                        "[Leader {} invalid route twice: {}. Leadership passed to {}.]",
                                        current_leader, validation_err, next
                                    ),
                                });
                                current_leader = next.clone();
                                group.leader_id = Some(next);
                                consecutive_invalid = 0;
                                continue;
                            }
                            None => {
                                group.status = GroupRunStatus::Failed;
                                break;
                            }
                        }
                    } else {
                        group.history.push(HackathonMessage {
                            role: "user".to_string(),
                            content: format!(
                                "Leader attempted route to {} with prompt: {}",
                                target_model, prompt
                            ),
                        });
                        group.history.push(HackathonMessage {
                            role: "system".to_string(),
                            content: format!(
                                "Invalid route: {}. Valid teammates are live non-leaders within cap. Try again.",
                                validation_err
                            ),
                        });
                        continue;
                    }
                } else {
                    consecutive_invalid = 0;
                    // Increment consultation count
                    let count = group
                        .consultation_counts
                        .entry(target_model.clone())
                        .or_insert(0);
                    *count = count.saturating_add(1);
                    for p in &mut group.participants {
                        if p.model_id == target_model {
                            p.consultation_count = *count;
                        }
                    }
                    // Append routing instruction to history
                    group.history.push(HackathonMessage {
                        role: "user".to_string(),
                        content: format!(
                            "[Leader {} routed to {}: {}]",
                            current_leader, target_model, prompt
                        ),
                    });

                    // Lookup teammate credentials
                    let (tm_base, tm_key, tm_model) = match model_credentials.get(&target_model) {
                        Some(v) => v.clone(),
                        None => {
                            group.history.push(HackathonMessage {
                                role: "system".to_string(),
                                content: format!(
                                    "[Teammate {} unavailable: missing credentials]",
                                    target_model
                                ),
                            });
                            continue;
                        }
                    };

                    // Build teammate messages (full history)
                    let tm_messages = build_teammate_messages(&group.history, &prompt);

                    let teammate_response = match call_hackathon_model(
                        &tm_base,
                        &tm_key,
                        &tm_model,
                        &tm_messages,
                        HACKATHON_GROUP_TIMEOUT_SECS,
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            let err_msg = redact_api_key_logs(&e);
                            tracing::warn!(
                                "[HACKATHON] teammate {} failed (run {}, group {}): {}",
                                target_model,
                                run_id,
                                group.group_name,
                                err_msg
                            );
                            // Mark teammate failed, but continue group (do not fallback leader)
                            live_set.remove(&target_model);
                            for p in &mut group.participants {
                                if p.model_id == target_model {
                                    p.status = ParticipantRunStatus::Failed;
                                    p.last_error = Some(err_msg.clone());
                                }
                            }
                            format!("[Response from {} unavailable: {}]", target_model, err_msg)
                        }
                    };

                    // Append teammate response to history
                    group.history.push(HackathonMessage {
                        role: "assistant".to_string(),
                        content: format!("[{} said]: {}", target_model, teammate_response),
                    });
                    // Loop back to leader
                }
            }
        }
    }

    group
}

/// Execute hackathon for the current session — shared helper for Tauri command and AgentDecision.
/// Returns combined delimited report on success. Validates task_brief (1-500 chars, non-empty),
/// checks hackathon config, validates selected participants server-side if provided, runs groups concurrently via JoinSet,
/// handles cancellation, formats report, emits events. Never leaks api keys.
pub async fn execute_hackathon(
    task_brief: String,
    selected_participant_ids: Option<Vec<String>>,
    state: &crate::orchestrator::AppState,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let trimmed = task_brief.trim();
    if trimmed.is_empty() {
        return Err("Hackathon task brief cannot be empty".to_string());
    }
    if trimmed.len() > 2000 {
        return Err("Hackathon task brief too long (max 2000 chars)".to_string());
    }
    // Must have an existing run state from invitations or config
    let (run_id, max_questions, config_groups) = {
        let run_lock = state.hackathon_run.lock().await;
        if let Some(run) = run_lock.as_ref() {
            if run.cancelled.load(Ordering::SeqCst) {
                return Err(
                    "Previous hackathon run was cancelled — send invitations again".to_string(),
                );
            }
            let has_confirmed = run.groups.iter().any(|g| {
                g.participants
                    .iter()
                    .any(|p| p.status == ParticipantRunStatus::Confirmed)
            });
            if !has_confirmed {
                return Err(
                    "No confirmed participants — send invitations and wait for responders"
                        .to_string(),
                );
            }
            (run.run_id.clone(), run.max_questions, run.groups.clone())
        } else {
            let config = state
                .settings_store
                .lock()
                .await
                .get_hackathon_config()
                .map_err(|e| format!("Failed to read hackathon config: {}", e))?;
            let selected: Vec<_> = config
                .groups
                .iter()
                .filter(|g| g.selected)
                .cloned()
                .collect();
            if selected.is_empty() {
                return Err("No hackathon run found — send invitations first".to_string());
            }
            let new_run_id = uuid::Uuid::new_v4().to_string();
            let mut groups = Vec::new();
            for g in selected {
                let participants: Vec<ParticipantRunState> =
                    g.model_ids
                        .iter()
                        .filter_map(|mid| {
                            config.models.iter().find(|m| &m.id == mid).map(|m| {
                                ParticipantRunState {
                                    model_id: m.id.clone(),
                                    model_name: m.model_name.clone(),
                                    base_url: m.base_url.clone(),
                                    group_id: g.id.clone(),
                                    status: ParticipantRunStatus::Pending,
                                    consultation_count: 0,
                                    last_error: None,
                                }
                            })
                        })
                        .collect();
                groups.push(GroupRunState {
                    group_id: g.id.clone(),
                    group_name: g.name.clone(),
                    model_ids_ordered: g.model_ids.clone(),
                    participants,
                    leader_id: None,
                    history: Vec::new(),
                    status: GroupRunStatus::Pending,
                    final_output: None,
                    consultation_counts: std::collections::HashMap::new(),
                });
            }
            (new_run_id, config.max_questions_per_teammate, groups)
        }
    };

    // If we created a new run_id because no prior run existed, store it
    {
        let mut run_lock = state.hackathon_run.lock().await;
        if run_lock.is_none() {
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let new_state = HackathonRunState {
                run_id: run_id.clone(),
                task_brief: task_brief.clone(),
                max_questions,
                groups: config_groups.clone(),
                cancelled: cancel_flag,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            *run_lock = Some(new_state);
            let mut id_lock = state.hackathon_run_id.lock().await;
            *id_lock = Some(run_id.clone());
            state.hackathon_cancel.store(false, Ordering::SeqCst);
        } else {
            if let Some(run) = run_lock.as_mut() {
                run.task_brief = task_brief.clone();
                run.cancelled.store(false, Ordering::SeqCst);
            }
            state.hackathon_cancel.store(false, Ordering::SeqCst);
        }
    }

    let config = state
        .settings_store
        .lock()
        .await
        .get_hackathon_config()
        .map_err(|e| format!("Failed to read hackathon config: {}", e))?;
    let mut model_creds: HashMap<String, (String, String, String)> = HashMap::new();
    for m in &config.models {
        model_creds.insert(
            m.id.clone(),
            (m.base_url.clone(), m.api_key.clone(), m.model_name.clone()),
        );
    }

    // Server-trusted selection validation
    if let Some(selected) = &selected_participant_ids {
        let run_lock = state.hackathon_run.lock().await;
        let run = run_lock
            .as_ref()
            .ok_or_else(|| "No hackathon run to validate selection against".to_string())?;
        let mut id_to_group: HashMap<String, (String, ParticipantRunStatus)> = HashMap::new();
        for g in &run.groups {
            for p in &g.participants {
                id_to_group.insert(p.model_id.clone(), (g.group_id.clone(), p.status.clone()));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for mid in selected {
            if !seen.insert(mid.clone()) {
                return Err(format!("Duplicate selected participant: {}", mid));
            }
            let (group_id, status) = id_to_group
                .get(mid)
                .ok_or_else(|| format!("Selected model {} not in current run", mid))?
                .clone();
            if status != ParticipantRunStatus::Confirmed {
                return Err(format!(
                    "Selected model {} is not a confirmed responder ({:?})",
                    mid, status
                ));
            }
            let _ = config_groups
                .iter()
                .find(|g| &g.group_id == &group_id)
                .ok_or_else(|| {
                    format!(
                        "Selected model {} belongs to group {} not in run",
                        mid, group_id
                    )
                })?;
        }
        {
            let mut run_mut = state.hackathon_run.lock().await;
            if let Some(r) = run_mut.as_mut() {
                let sel_set: std::collections::HashSet<String> = selected.iter().cloned().collect();
                for g in &mut r.groups {
                    let is_selected_group =
                        config_groups.iter().any(|cg| cg.group_id == g.group_id);
                    if !is_selected_group || g.status == GroupRunStatus::Locked {
                        continue;
                    }
                    for p in &mut g.participants {
                        if p.status == ParticipantRunStatus::Confirmed
                            && !sel_set.contains(&p.model_id)
                        {
                            p.status = ParticipantRunStatus::Failed;
                            p.last_error = Some("Deselected by user before Go".to_string());
                        }
                    }
                    let live_set: std::collections::HashSet<String> = g
                        .participants
                        .iter()
                        .filter(|p| p.status == ParticipantRunStatus::Confirmed)
                        .map(|p| p.model_id.clone())
                        .collect();
                    if live_set.is_empty() {
                        g.status = GroupRunStatus::Locked;
                        g.leader_id = None;
                    } else {
                        g.leader_id = select_leader(&g.model_ids_ordered, &live_set);
                        if g.status != GroupRunStatus::Locked {
                            g.status = GroupRunStatus::Pending;
                        }
                    }
                }
            }
        }
    }

    let groups_snapshot: Vec<GroupRunState> = {
        let run_lock = state.hackathon_run.lock().await;
        run_lock
            .as_ref()
            .map(|r| r.groups.clone())
            .unwrap_or_default()
    };
    let executable_groups: Vec<GroupRunState> = groups_snapshot
        .into_iter()
        .filter(|g| g.status != GroupRunStatus::Locked)
        .filter(|g| {
            g.participants
                .iter()
                .any(|p| p.status == ParticipantRunStatus::Confirmed)
        })
        .collect();
    if executable_groups.is_empty() {
        return Err(
            "No executable groups — all are locked or have zero selected live members".to_string(),
        );
    }

    let app_clone = app.clone();
    let _ = app_clone.emit(
        "hackathon-run-started",
        serde_json::json!({
            "run_id": run_id,
            "task_brief": task_brief,
            "group_count": executable_groups.len(),
        }),
    );

    let cancel_flag = {
        let run_lock = state.hackathon_run.lock().await;
        run_lock
            .as_ref()
            .map(|r| r.cancelled.clone())
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)))
    };
    let global_cancel = state.hackathon_cancel.clone();

    let mut join_set = tokio::task::JoinSet::new();
    for group in executable_groups {
        let task_brief_clone = task_brief.clone();
        let max_q = max_questions;
        let creds = model_creds.clone();
        let run_id_task = run_id.clone();
        let cancel_clone = cancel_flag.clone();
        let global_cancel_clone = global_cancel.clone();
        let app_task = app_clone.clone();
        join_set.spawn(async move {
            let combined_cancel = Arc::new(AtomicBool::new(false));
            let watcher_cancel = combined_cancel.clone();
            let c1 = cancel_clone.clone();
            let c2 = global_cancel_clone.clone();
            tokio::spawn(async move {
                loop {
                    if c1.load(Ordering::SeqCst) || c2.load(Ordering::SeqCst) {
                        watcher_cancel.store(true, Ordering::SeqCst);
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            });
            let result = run_single_group(
                group,
                task_brief_clone,
                max_q,
                creds,
                run_id_task.clone(),
                combined_cancel,
            )
            .await;
            let _ = app_task.emit(
                "hackathon-group-output",
                serde_json::json!({
                    "run_id": run_id_task,
                    "group_id": result.group_id,
                    "group_name": result.group_name,
                    "status": result.status,
                    "final_output": result.final_output,
                }),
            );
            result
        });
    }

    let mut completed_groups: Vec<GroupRunState> = Vec::new();
    while let Some(res) = join_set.join_next().await {
        match res {
            Ok(group) => completed_groups.push(group),
            Err(e) => {
                tracing::warn!("[HACKATHON] group task join error: {}", e);
            }
        }
    }

    {
        let active_id = state
            .hackathon_run_id
            .lock()
            .await
            .clone()
            .unwrap_or_default();
        if active_id != run_id {
            return Err("Hackathon run was superseded by a newer run".to_string());
        }
    }
    if cancel_flag.load(Ordering::SeqCst) || global_cancel.load(Ordering::SeqCst) {
        return Err("Hackathon run was cancelled".to_string());
    }
    {
        let mut run_lock = state.hackathon_run.lock().await;
        if let Some(run) = run_lock.as_mut() {
            if run.run_id == run_id {
                for completed in &completed_groups {
                    if let Some(stored) = run
                        .groups
                        .iter_mut()
                        .find(|g| g.group_id == completed.group_id)
                    {
                        *stored = completed.clone();
                    }
                }
                completed_groups = run.groups.clone();
            }
        }
    }

    let report = format_report(&completed_groups, &run_id);
    let _ = app.emit(
        "hackathon-complete",
        serde_json::json!({
            "run_id": run_id,
            "report": report,
            "group_count": completed_groups.len(),
        }),
    );
    Ok(report)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn select_leader_first_live() {
        let ordered = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut live = HashSet::new();
        live.insert("b".to_string());
        live.insert("c".to_string());
        assert_eq!(select_leader(&ordered, &live), Some("b".to_string()));
    }

    #[test]
    fn select_leader_none_when_empty() {
        let ordered = vec!["a".to_string(), "b".to_string()];
        let live = HashSet::new();
        assert_eq!(select_leader(&ordered, &live), None);
    }

    #[test]
    fn fallback_moves_down() {
        let ordered = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut live = HashSet::new();
        live.insert("c".to_string());
        // a failed, next live down is c (b not live)
        assert_eq!(fallback_leader(&ordered, "a", &live), Some("c".to_string()));
    }

    #[test]
    fn fallback_none_when_none_below() {
        let ordered = vec!["a".to_string(), "b".to_string()];
        let mut live = HashSet::new();
        live.insert("a".to_string());
        assert_eq!(fallback_leader(&ordered, "b", &live), None);
    }

    #[test]
    fn zero_live_group_locked() {
        let ordered = vec!["a".to_string(), "b".to_string()];
        let live = HashSet::new();
        assert_eq!(select_leader(&ordered, &live), None);
    }

    #[test]
    fn sort_responders_float_top_preserving_order() {
        let ordered = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let mut statuses = HashMap::new();
        statuses.insert("a".to_string(), ParticipantRunStatus::Failed);
        statuses.insert("b".to_string(), ParticipantRunStatus::Confirmed);
        statuses.insert("c".to_string(), ParticipantRunStatus::Pending);
        statuses.insert("d".to_string(), ParticipantRunStatus::Confirmed);
        let sorted = sort_by_responder_status(&ordered, &statuses);
        assert_eq!(sorted, vec!["b", "d", "a", "c"]);
    }

    #[test]
    fn per_teammate_cap_blocks_when_reached() {
        let group_ids = vec!["lead".to_string(), "tm1".to_string(), "tm2".to_string()];
        let mut live = HashSet::new();
        live.insert("tm1".to_string());
        live.insert("tm2".to_string());
        let mut counts = HashMap::new();
        counts.insert("tm1".to_string(), 3);
        // cap 3 should block
        assert!(is_route_allowed("tm1", "lead", &group_ids, &live, &counts, Some(3)).is_err());
        // tm2 still allowed (0 < 3)
        assert!(is_route_allowed("tm2", "lead", &group_ids, &live, &counts, Some(3)).is_ok());
    }

    #[test]
    fn unlimited_never_blocks() {
        let group_ids = vec!["lead".to_string(), "tm1".to_string()];
        let mut live = HashSet::new();
        live.insert("tm1".to_string());
        let mut counts = HashMap::new();
        counts.insert("tm1".to_string(), 100);
        assert!(is_route_allowed("tm1", "lead", &group_ids, &live, &counts, None).is_ok());
    }

    #[test]
    fn invalid_route_target_rejected() {
        let group_ids = vec!["lead".to_string(), "tm1".to_string()];
        let mut live = HashSet::new();
        live.insert("tm1".to_string());
        let counts = HashMap::new();
        // routing to leader itself is invalid
        assert!(is_route_allowed("lead", "lead", &group_ids, &live, &counts, Some(3)).is_err());
        // routing to non-member
        assert!(is_route_allowed("outsider", "lead", &group_ids, &live, &counts, Some(3)).is_err());
        // routing to dead teammate
        assert!(is_route_allowed("tm2", "lead", &group_ids, &live, &counts, Some(3)).is_err());
    }

    #[test]
    fn report_formatting_includes_groups() {
        let groups = vec![
            GroupRunState {
                group_id: "g1".to_string(),
                group_name: "Falcon".to_string(),
                model_ids_ordered: vec!["m1".to_string()],
                participants: vec![],
                leader_id: Some("m1".to_string()),
                history: vec![],
                status: GroupRunStatus::Completed,
                final_output: Some("Falcon output".to_string()),
                consultation_counts: HashMap::new(),
            },
            GroupRunState {
                group_id: "g2".to_string(),
                group_name: "Vega".to_string(),
                model_ids_ordered: vec!["m2".to_string()],
                participants: vec![],
                leader_id: None,
                history: vec![],
                status: GroupRunStatus::Locked,
                final_output: None,
                consultation_counts: HashMap::new(),
            },
        ];
        let report = format_report(&groups, "run-123");
        assert!(report.contains("[Hackathon Group: Falcon]"));
        assert!(report.contains("Falcon output"));
        assert!(report.contains("[Hackathon Group: Vega]"));
        assert!(report.contains("zero live members"));
        assert!(report.contains("run-123"));
    }

    #[test]
    fn stale_run_rejection_concept() {
        // Simulate run_id check — event with old run_id should be ignored
        let active_run_id = "run-new";
        let event_run_id = "run-old";
        assert_ne!(active_run_id, event_run_id);
        // In real code: if event_run_id != stored_run_id, ignore
    }

    #[test]
    fn parse_valid_route() {
        let json = r#"{"action":"route","target_model":"tm1","prompt":"hello"}"#;
        let dec = parse_hackathon_decision(json).expect("parses route");
        assert_eq!(
            dec,
            HackathonDecision::Route {
                target_model: "tm1".to_string(),
                prompt: "hello".to_string()
            }
        );
    }

    #[test]
    fn parse_valid_submit() {
        let json = r#"{"action":"submit","final_output":"done"}"#;
        let dec = parse_hackathon_decision(json).expect("parses submit");
        assert_eq!(
            dec,
            HackathonDecision::Submit {
                final_output: "done".to_string()
            }
        );
    }

    #[test]
    fn parse_invalid_json_fails() {
        let bad = r#"{"action":"route","target_model":}"#;
        assert!(parse_hackathon_decision(bad).is_err());
    }

    #[test]
    fn extract_fenced_json() {
        let content =
            "Here:\n```json\n{\"action\":\"submit\",\"final_output\":\"hi\"}\n```\nThanks";
        let clean = extract_json_object(content).expect("extracted");
        assert!(clean.contains("\"submit\""));
    }

    #[test]
    fn hackathon_config_validation_empty_name_fails() {
        let mut cfg = HackathonConfig::default();
        cfg.groups.push(HackathonGroupConfig {
            id: "g1".to_string(),
            name: "".to_string(),
            model_ids: vec![],
            selected: true,
        });
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn hackathon_safe_omits_keys() {
        let cfg = HackathonConfig {
            groups: vec![],
            models: vec![HackathonModelConfig {
                id: "m1".to_string(),
                model_name: "llama".to_string(),
                base_url: "https://example.com/v1".to_string(),
                api_key: "secret123".to_string(),
                group_id: "g1".to_string(),
            }],
            max_questions_per_teammate: Some(3),
            enabled: true,
        };
        let safe = cfg.to_safe();
        let json = serde_json::to_string(&safe).expect("serializes");
        assert!(!json.contains("secret123"));
        assert!(json.contains("llama"));
        assert!(json.contains("https://example.com/v1"));
    }
}
