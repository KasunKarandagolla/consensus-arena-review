//! Bounded TikTok research adapter for tamnd/tiktok-cli (`tt`).
//!
//! TikTok is intentionally a capability, not a Product OS authority. Exit
//! status 4 is unavailable/inconclusive and is never converted to an empty
//! evidence result.

use crate::dsh_worker;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_QUERY_BYTES: usize = 2_000;
const MAX_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_RESULTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiktokOperation {
    Search,
    Video,
    User,
    Posts,
    Comments,
    Replies,
    Hashtag,
    Sound,
    Trending,
    Discover,
}

impl TiktokOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Video => "video",
            Self::User => "user",
            Self::Posts => "posts",
            Self::Comments => "comments",
            Self::Replies => "replies",
            Self::Hashtag => "hashtag",
            Self::Sound => "sound",
            Self::Trending => "trending",
            Self::Discover => "discover",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TiktokRequest {
    pub operation: TiktokOperation,
    pub query: String,
    /// Secondary positional identity used by operations such as replies
    /// (comment ID). Kept explicit so Arena does not hide multi-argument
    /// commands inside an opaque query string.
    #[serde(default)]
    pub secondary: Option<String>,
    pub max_results: usize,
    pub timeout_seconds: u64,
}

impl TiktokRequest {
    pub fn validate(&self) -> Result<(), String> {
        let query_required = !matches!(
            self.operation,
            TiktokOperation::Trending | TiktokOperation::Discover
        );
        if (query_required && self.query.trim().is_empty())
            || self.query.len() > MAX_QUERY_BYTES
            || self.query.chars().any(char::is_control)
        {
            return Err("TikTok query is empty, oversized, or contains control text".to_string());
        }
        if let Some(secondary) = self.secondary.as_deref() {
            if secondary.trim().is_empty()
                || secondary.len() > MAX_QUERY_BYTES
                || secondary.chars().any(char::is_control)
            {
                return Err("TikTok secondary identity is invalid or oversized".to_string());
            }
        }
        if self.operation == TiktokOperation::Replies && self.secondary.is_none() {
            return Err("TikTok replies requires a comment ID".to_string());
        }
        if self.operation != TiktokOperation::Replies && self.secondary.is_some() {
            return Err("TikTok secondary identity is only valid for replies".to_string());
        }
        if self.max_results == 0 || self.max_results > MAX_RESULTS {
            return Err("TikTok result count is outside the bounded range".to_string());
        }
        if self.timeout_seconds == 0 || self.timeout_seconds > 120 {
            return Err("TikTok timeout is outside the bounded range".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiktokResultStatus {
    Success,
    Empty,
    Unavailable,
    NotFound,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TiktokEvidenceRecord {
    pub identity: String,
    pub source_url: Option<String>,
    pub title: Option<String>,
    pub operation: TiktokOperation,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TiktokResult {
    pub status: TiktokResultStatus,
    pub exit_code: Option<i32>,
    pub records: Vec<TiktokEvidenceRecord>,
    pub bounded_output_bytes: usize,
    pub error_classification: Option<String>,
}

pub fn executable() -> PathBuf {
    std::env::var_os("ARENA_TIKTOK_EXECUTABLE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tt"))
}

pub async fn runtime_status(cwd: &Path) -> Result<bool, String> {
    let output = dsh_worker::run_contained_command(
        &executable(),
        &[OsString::from("version")],
        cwd,
        Duration::from_secs(15),
    )
    .await?;
    Ok(output.exit_code == Some(0) && !output.timed_out)
}

pub async fn run(cwd: &Path, request: TiktokRequest) -> Result<TiktokResult, String> {
    request.validate()?;
    let mut args = vec![OsString::from(request.operation.as_str())];
    match request.operation {
        TiktokOperation::Discover => {
            if request.query.trim().is_empty() {
                args.push(OsString::from("--trending"));
            } else {
                args.push(OsString::from("--seed-search"));
                args.push(OsString::from(request.query.clone()));
            }
        }
        TiktokOperation::Replies => {
            args.push(OsString::from(request.query.clone()));
            args.push(OsString::from(request.secondary.as_deref().ok_or_else(
                || "TikTok replies comment ID disappeared".to_string(),
            )?));
        }
        _ => {
            if !request.query.trim().is_empty() {
                args.push(OsString::from(request.query.clone()));
            }
        }
    }
    args.extend([
        OsString::from("--output"),
        OsString::from("jsonl"),
        OsString::from("--limit"),
        OsString::from(request.max_results.to_string()),
        OsString::from("--timeout"),
        OsString::from(request.timeout_seconds.to_string()),
    ]);
    let output = dsh_worker::run_contained_command(
        &executable(),
        &args,
        cwd,
        Duration::from_secs(request.timeout_seconds),
    )
    .await?;
    let bounded_output_bytes = output.stdout.len().saturating_add(output.stderr.len());
    if output.timed_out {
        return Ok(TiktokResult {
            status: TiktokResultStatus::Unavailable,
            exit_code: output.exit_code,
            records: Vec::new(),
            bounded_output_bytes: bounded_output_bytes.min(MAX_OUTPUT_BYTES),
            error_classification: Some("timeout".to_string()),
        });
    }
    if bounded_output_bytes > MAX_OUTPUT_BYTES {
        return Ok(TiktokResult {
            status: TiktokResultStatus::Failed,
            exit_code: output.exit_code,
            records: Vec::new(),
            bounded_output_bytes: MAX_OUTPUT_BYTES,
            error_classification: Some("output_limit".to_string()),
        });
    }
    let status = match output.exit_code {
        Some(0) => TiktokResultStatus::Success,
        Some(3) => TiktokResultStatus::Empty,
        Some(4) => TiktokResultStatus::Unavailable,
        Some(6) => TiktokResultStatus::NotFound,
        _ => TiktokResultStatus::Failed,
    };
    let records = if status == TiktokResultStatus::Success {
        parse_records(&output.stdout, request.operation)
    } else {
        Vec::new()
    };
    let status = if status == TiktokResultStatus::Success && records.is_empty() {
        // Exit 3 is the CLI's explicit valid-empty contract. Exit 0 with no
        // parseable records is therefore malformed/unsupported output, not an
        // evidence-bearing success.
        TiktokResultStatus::Failed
    } else {
        status
    };
    Ok(TiktokResult {
        status,
        exit_code: output.exit_code,
        records,
        bounded_output_bytes,
        error_classification: match status {
            TiktokResultStatus::Unavailable => Some("walled_or_unavailable".to_string()),
            TiktokResultStatus::NotFound => Some("not_found".to_string()),
            TiktokResultStatus::Failed => Some("execution_failure".to_string()),
            TiktokResultStatus::Success | TiktokResultStatus::Empty => None,
        },
    })
}

fn parse_records(raw: &str, operation: TiktokOperation) -> Vec<TiktokEvidenceRecord> {
    raw.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .take(MAX_RESULTS)
        .filter_map(|value| {
            let identity = value
                .get("id")
                .or_else(|| value.get("video_id"))
                .or_else(|| value.get("aweme_id"))
                .and_then(|id| {
                    id.as_str()
                        .map(ToString::to_string)
                        .or_else(|| id.as_i64().map(|number| number.to_string()))
                        .or_else(|| id.as_u64().map(|number| number.to_string()))
                })?;
            Some(TiktokEvidenceRecord {
                identity,
                source_url: value
                    .get("web_url")
                    .or_else(|| value.get("url"))
                    .or_else(|| value.get("share_url"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                title: value
                    .get("title")
                    .or_else(|| value.get("desc"))
                    .and_then(Value::as_str)
                    .map(|value| value.chars().take(400).collect()),
                operation,
                provenance: "tt-jsonl".to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_status_mapping_preserves_unavailable_semantics() {
        assert_eq!(
            match 4 {
                0 => TiktokResultStatus::Success,
                3 => TiktokResultStatus::Empty,
                4 => TiktokResultStatus::Unavailable,
                6 => TiktokResultStatus::NotFound,
                _ => TiktokResultStatus::Failed,
            },
            TiktokResultStatus::Unavailable
        );
    }

    #[test]
    fn request_and_output_are_bounded() {
        assert!(
            TiktokRequest {
                operation: TiktokOperation::Search,
                query: "arena".to_string(),
                secondary: None,
                max_results: 3,
                timeout_seconds: 20,
            }
            .validate()
            .is_ok()
        );
        assert!(
            TiktokRequest {
                operation: TiktokOperation::Search,
                query: "arena".to_string(),
                secondary: None,
                max_results: 21,
                timeout_seconds: 20,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn feed_operations_may_omit_a_query() {
        assert!(
            TiktokRequest {
                operation: TiktokOperation::Trending,
                query: String::new(),
                secondary: None,
                max_results: 3,
                timeout_seconds: 20,
            }
            .validate()
            .is_ok()
        );
        assert!(
            TiktokRequest {
                operation: TiktokOperation::Search,
                query: String::new(),
                secondary: None,
                max_results: 3,
                timeout_seconds: 20,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn replies_requires_comment_id_and_discover_maps_to_a_bounded_seed() {
        assert!(
            TiktokRequest {
                operation: TiktokOperation::Replies,
                query: "https://www.tiktok.com/@a/video/v1".to_string(),
                secondary: None,
                max_results: 3,
                timeout_seconds: 20,
            }
            .validate()
            .is_err()
        );
        assert!(
            TiktokRequest {
                operation: TiktokOperation::Replies,
                query: "https://www.tiktok.com/@a/video/v1".to_string(),
                secondary: Some("comment-1".to_string()),
                max_results: 3,
                timeout_seconds: 20,
            }
            .validate()
            .is_ok()
        );
        assert!(
            TiktokRequest {
                operation: TiktokOperation::Discover,
                query: "tauri agents".to_string(),
                secondary: None,
                max_results: 3,
                timeout_seconds: 20,
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn numeric_record_identity_is_preserved() {
        let records = parse_records(
            r#"{"id":7106594312292453675,"web_url":"https://www.tiktok.com/@a/video/7106594312292453675"}"#,
            TiktokOperation::Video,
        );
        assert_eq!(records[0].identity, "7106594312292453675");
    }

    #[test]
    fn successful_jsonl_records_keep_identity_and_url() {
        let records = parse_records(
            r#"{"id":"v1","web_url":"https://www.tiktok.com/@a/video/v1","desc":"hello"}"#,
            TiktokOperation::Search,
        );
        assert_eq!(records[0].identity, "v1");
        assert_eq!(
            records[0].source_url.as_deref(),
            Some("https://www.tiktok.com/@a/video/v1")
        );
    }
}
