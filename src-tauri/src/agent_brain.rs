use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::errors::AgentError;

/// HIGH-4: HTTP timeout for agent-brain API calls. Without this, an
/// unresponsive orchestration endpoint stalls the entire autonomous session
/// loop indefinitely — browser navigation already has a 300s timeout
/// (wait_for_response), but this call had no mitigation at all.
const BRAIN_HTTP_TIMEOUT_SECS: u64 = 60;

const DECISION_JSON_CONTRACT: &str = r#"

Return exactly one JSON object and no markdown or explanation. The object must use exactly one
of these actions: route, route_compare, blueprint, continue, complete, ask_user.
Examples:
{"action":"route","target_model":"deepseek","prompt":"Review this proposal for risks and simplifications."}
{"action":"blueprint","section_title":"Initial MVP Blueprint","section_content":"..."}
{"action":"continue"}
{"action":"complete"}
{"action":"route_compare","models":["deepseek","claude"],"prompt":"Compare trade-offs."}
{"action":"ask_user","question":"Which platform?","options":["Web","Mobile"],"allow_custom":true}
Use canonical participant IDs supplied in Context (for example deepseek), not display names.
If the leader asks to consult a selected participant, choose route. If the leader produced a useful
blueprint section and no consultation is needed, choose blueprint.
"#;

// reqwest::Client is cheaply Clone (Arc-backed connection pool).
// All other fields are String / Option<String>.
// Clone is required so session_runner::run_debate can clone the brain out of
// the agent_brain lock before starting the session loop (DEF-001 fix).
#[derive(Clone)]
pub struct AgentBrain {
    api_key: String,
    base_url: String,
    model: String,
    system_prompt: String,
    client: Client,
    // D-038: optional fallback config. Set via with_fallback(). On primary failure,
    // decide() constructs a one-shot fallback client and retries once.
    fallback_api_key: Option<String>,
    fallback_base_url: Option<String>,
    fallback_model: Option<String>,
}

/// rename_all = "snake_case" so RouteCompare → "route_compare" and AskUser → "ask_user",
/// matching the system prompt JSON contract in ARCHITECTURE.md.
/// Single-word variants (route, blueprint, continue, complete) are unaffected.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentDecision {
    Route { target_model: String, prompt: String },
    Blueprint { section_title: String, section_content: String },
    Continue,
    Complete,
    /// D-035: side-by-side comparison — route prompt to each listed model in sequence,
    /// return combined "[X said: …][Y said: …]" block to leader.
    RouteCompare { models: Vec<String>, prompt: String },
    /// D-041: pause session loop, ask user a question, resume with their answer.
    AskUser {
        question: String,
        options: Vec<String>,
        allow_custom: bool,
    },
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

impl AgentBrain {
    pub fn new(
        api_key: String,
        base_url: String,
        model: String,
        system_prompt: String,
    ) -> Result<Self, AgentError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(BRAIN_HTTP_TIMEOUT_SECS))
            .build()
            .map_err(|e| AgentError::NetworkError(
                format!("Failed to create HTTP client: {}", e),
            ))?;

        Ok(AgentBrain {
            api_key,
            base_url,
            model,
            system_prompt,
            client,
            fallback_api_key: None,
            fallback_base_url: None,
            fallback_model: None,
        })
    }

    /// D-038: attach a fallback brain config. Builder-style; call after new().
    /// If the primary API call fails, decide() constructs a fresh client from
    /// these credentials and retries once before returning the original error.
    pub fn with_fallback(
        mut self,
        api_key: String,
        base_url: String,
        model: String,
    ) -> Self {
        self.fallback_api_key = Some(api_key);
        self.fallback_base_url = Some(base_url);
        self.fallback_model = Some(model);
        self
    }

    /// Task 5 (HIGH-3): explicitly clear a previously-attached fallback —
    /// used when the user saves an empty fallback config to remove it,
    /// since `with_fallback("", "", "")` would otherwise leave the brain
    /// thinking a fallback is configured (the `Some(_)` match in `decide()`
    /// doesn't care whether the strings are empty).
    pub fn without_fallback(mut self) -> Self {
        self.fallback_api_key = None;
        self.fallback_base_url = None;
        self.fallback_model = None;
        self
    }

    pub async fn decide(
        &self,
        leader_response: &str,
        context: &str,
        memory_context: Option<&str>,
    ) -> Result<AgentDecision, AgentError> {
        let user_content = format!(
            "Leader response:\n{}\n\nContext:\n{}",
            leader_response, context
        );

        let effective_system_prompt = self.build_effective_system_prompt(memory_context);

        // Primary attempt
        match self
            .call_api_with(
                &self.client,
                &self.base_url,
                &self.api_key,
                &self.model,
                &effective_system_prompt,
                &user_content,
            )
            .await
        {
            Ok(decision) => Ok(decision),

            // D-038: on failure, retry once with the fallback config if present.
            Err(primary_err) => {
                match (
                    &self.fallback_api_key,
                    &self.fallback_base_url,
                    &self.fallback_model,
                ) {
                    (Some(fb_key), Some(fb_url), Some(fb_model)) => {
                        tracing::debug!(
                            "[BRAIN] primary failed ({}); retrying with fallback",
                            primary_err
                        );
                        // HIGH-4: fallback client also gets the timeout — a
                        // hung fallback is exactly as fatal as a hung primary.
                        let fb_client = reqwest::Client::builder()
                            .timeout(Duration::from_secs(BRAIN_HTTP_TIMEOUT_SECS))
                            .build()
                            .map_err(|e| AgentError::NetworkError(
                                format!("Fallback client build failed: {}", e),
                            ))?;
                        self.call_api_with(
                            &fb_client,
                            fb_url,
                            fb_key,
                            fb_model,
                            &effective_system_prompt,
                            &user_content,
                        )
                        .await
                        // On fallback failure, surface the original error.
                        .map_err(|_| primary_err)
                    }
                    _ => Err(primary_err),
                }
            }
        }
    }

    pub fn build_effective_system_prompt(&self, memory_context: Option<&str>) -> String {
        let mut prompt = self.system_prompt.clone();
        prompt.push_str(DECISION_JSON_CONTRACT);
        if let Some(memory) = memory_context {
            if !memory.trim().is_empty() {
                prompt.push_str("\n\n<memory_context>\n");
                prompt.push_str(memory);
                prompt.push_str("\n</memory_context>");
            }
        }
        prompt
    }

    /// Shared HTTP call + parse logic used by both primary and fallback paths.
    async fn call_api_with(
        &self,
        client: &Client,
        base_url: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_content: &str,
    ) -> Result<AgentDecision, AgentError> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_content.to_string(),
                },
            ],
            max_tokens: 1024,
        };

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                AgentError::NetworkError(format!("Agent brain request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::NetworkError(format!(
                "Agent brain API error {}: {}",
                status, body
            )));
        }

        let chat_response: ChatResponse = response.json().await.map_err(|e| {
            AgentError::NetworkError(format!("Failed to parse agent brain response: {}", e))
        })?;

        let content = chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| {
                AgentError::NetworkError("Agent brain returned empty response".to_string())
            })?;

        // D-040 [BRAIN] raw response (truncated to 400 chars for log readability)
        tracing::debug!("[BRAIN] raw ({}) {:.400}", url, content);

        let clean = extract_json_object(&content).ok_or_else(|| {
            AgentError::NetworkError("Agent brain response contained no complete JSON object".to_string())
        })?;

        let decision = serde_json::from_str::<AgentDecision>(clean).map_err(|e| {
            AgentError::NetworkError(format!(
                "Failed to parse agent decision JSON: {} ({} bytes, redacted)",
                e,
                clean.len()
            ))
        })?;

        // D-040 [BRAIN] parsed decision
        tracing::debug!("[BRAIN] parsed {:?}", decision);

        Ok(decision)
    }
}

/// Returns the first balanced JSON object, tolerating code fences and prose
/// around it without logging or duplicating the brain response.
fn extract_json_object(content: &str) -> Option<&str> {
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
            } else if *byte == b'\"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'\"' => in_string = true,
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

#[cfg(test)]
mod tests {
    use super::extract_json_object;

    #[test]
    fn extracts_fenced_json_with_trailing_prose() {
        assert_eq!(
            extract_json_object("Here:\n```json\n{\"action\":\"continue\"}\n```\nThanks"),
            Some("{\"action\":\"continue\"}")
        );
    }
}
