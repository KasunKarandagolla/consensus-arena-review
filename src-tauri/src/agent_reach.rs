//! Agent Reach research substrate adapter.
//!
//! Arena owns research intent, work orders, budgets, provenance and fact
//! verification. Agent Reach is used only as the platform-routing substrate;
//! this adapter executes a closed set of read-only upstream commands documented
//! by Agent Reach. It intentionally does not run `agent-reach doctor` on a
//! status read because current upstream doctor behavior may touch skill files.

use crate::dsh_worker;
use crate::work_graph::ResearchChannel;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_RESULT_BYTES: usize = 512 * 1024;
const MAX_SOURCES: usize = 12;
const RESEARCH_TIMEOUT_SECONDS: u64 = 180;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentReachRuntimeStatus {
    pub executable: String,
    pub observed_version: Option<String>,
    pub qualified_version: Option<String>,
    pub version_matches_qualification: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelSourceObservation {
    pub url: String,
    pub title: String,
    pub summary: String,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelResearchResult {
    pub channel: ResearchChannel,
    pub backend: String,
    pub sources: Vec<ChannelSourceObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelUnavailable {
    pub channel: ResearchChannel,
    pub reason: String,
}

fn executable() -> PathBuf {
    std::env::var_os("ARENA_AGENT_REACH_EXECUTABLE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("agent-reach"))
}

fn qualified_version() -> Option<String> {
    std::env::var("ARENA_AGENT_REACH_QUALIFIED_VERSION")
        .ok()
        .map(|value| value.trim().trim_start_matches('v').to_string())
        .filter(|value| !value.is_empty())
}

fn safe_query(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() || query.len() > 2_000 || query.chars().any(char::is_control) {
        return Err("research channel query is empty, oversized, or invalid".to_string());
    }
    Ok(query.to_string())
}

async fn contained(
    executable: &Path,
    args: Vec<OsString>,
    cwd: &Path,
) -> Result<crate::dsh_worker::ContainedExecution, String> {
    let result = dsh_worker::run_contained_command(
        executable,
        &args,
        cwd,
        Duration::from_secs(RESEARCH_TIMEOUT_SECONDS),
    )
    .await?;
    if result.timed_out {
        return Err("research channel command timed out".to_string());
    }
    if result.stdout.len().saturating_add(result.stderr.len()) > MAX_RESULT_BYTES {
        return Err("research channel output exceeded the Arena bound".to_string());
    }
    Ok(result)
}

pub async fn runtime_status(cwd: &Path) -> AgentReachRuntimeStatus {
    let expected = qualified_version();
    let result = contained(&executable(), vec![OsString::from("version")], cwd).await;
    match result {
        Ok(output) if output.exit_code == Some(0) => {
            let combined = format!("{} {}", output.stdout, output.stderr);
            let observed = combined
                .split_whitespace()
                .map(|part| part.trim_start_matches('v'))
                .find(|part| {
                    part.chars().next().is_some_and(|character| character.is_ascii_digit())
                })
                .map(ToString::to_string);
            let version_matches_qualification =
                expected.as_deref().is_some_and(|value| observed.as_deref() == Some(value));
            AgentReachRuntimeStatus {
                executable: executable().to_string_lossy().into_owned(),
                observed_version: observed.clone(),
                qualified_version: expected.clone(),
                version_matches_qualification,
                message: match (observed, expected) {
                    (Some(observed), Some(expected)) if observed == expected => {
                        format!("Agent Reach {observed} matches Arena qualification")
                    }
                    (Some(observed), Some(expected)) => format!(
                        "Agent Reach {observed} is installed but Arena qualification is pinned to {expected}"
                    ),
                    (Some(observed), None) => format!(
                        "Agent Reach {observed} is installed; no Arena-qualified version is configured"
                    ),
                    _ => "Agent Reach version output was not recognized".to_string(),
                },
            }
        }
        Ok(output) => AgentReachRuntimeStatus {
            executable: executable().to_string_lossy().into_owned(),
            observed_version: None,
            qualified_version: expected,
            version_matches_qualification: false,
            message: format!("Agent Reach version probe exited {:?}", output.exit_code),
        },
        Err(error) => AgentReachRuntimeStatus {
            executable: executable().to_string_lossy().into_owned(),
            observed_version: None,
            qualified_version: expected,
            version_matches_qualification: false,
            message: format!("Agent Reach is unavailable: {error}"),
        },
    }
}

fn unsupported(channel: ResearchChannel, reason: &str) -> ChannelUnavailable {
    ChannelUnavailable {
        channel,
        reason: reason.to_string(),
    }
}

async fn research_github(cwd: &Path, query: &str) -> Result<ChannelResearchResult, String> {
    let output = contained(
        Path::new("gh"),
        vec![
            OsString::from("search"),
            OsString::from("repos"),
            OsString::from(query),
            OsString::from("--sort"),
            OsString::from("stars"),
            OsString::from("--limit"),
            OsString::from("10"),
            OsString::from("--json"),
            OsString::from("fullName,description,url,stargazersCount,updatedAt"),
        ],
        cwd,
    )
    .await?;
    if output.exit_code != Some(0) {
        return Err("Agent Reach GitHub backend (gh) failed".to_string());
    }
    let values: serde_json::Value = serde_json::from_str(&output.stdout)
        .map_err(|_| "Agent Reach GitHub backend returned malformed JSON".to_string())?;
    let items = values
        .as_array()
        .ok_or_else(|| "Agent Reach GitHub backend returned a non-array result".to_string())?;
    let mut sources = Vec::new();
    for item in items.iter().take(MAX_SOURCES) {
        let Some(url) = item.get("url").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let title = item
            .get("fullName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("GitHub repository");
        let description = item
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let stars = item
            .get("stargazersCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default();
        let updated = item
            .get("updatedAt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        sources.push(ChannelSourceObservation {
            url: url.to_string(),
            title: title.to_string(),
            summary: format!(
                "{}; stars={stars}; updated={updated}",
                description.chars().take(1_000).collect::<String>()
            ),
            source_type: "github_repository_search".to_string(),
        });
    }
    if sources.is_empty() {
        return Err("Agent Reach GitHub backend produced no usable sources".to_string());
    }
    Ok(ChannelResearchResult {
        channel: ResearchChannel::Github,
        backend: "Agent Reach upstream: gh".to_string(),
        sources,
    })
}

async fn research_youtube(cwd: &Path, query: &str) -> Result<ChannelResearchResult, String> {
    let search = format!("ytsearch10:{query}");
    let output = contained(
        Path::new("yt-dlp"),
        vec![
            OsString::from("--dump-json"),
            OsString::from("--skip-download"),
            OsString::from("--no-playlist"),
            OsString::from(search),
        ],
        cwd,
    )
    .await?;
    if output.exit_code != Some(0) {
        return Err("Agent Reach YouTube backend (yt-dlp) failed".to_string());
    }
    let mut sources = Vec::new();
    for line in output.stdout.lines().take(MAX_SOURCES) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(url) = value
            .get("webpage_url")
            .and_then(serde_json::Value::as_str)
            .or_else(|| value.get("original_url").and_then(serde_json::Value::as_str))
        else {
            continue;
        };
        let title = value
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("YouTube video");
        let channel = value
            .get("channel")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown channel");
        let description = value
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let upload_date = value
            .get("upload_date")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        sources.push(ChannelSourceObservation {
            url: url.to_string(),
            title: title.to_string(),
            summary: format!(
                "channel={channel}; upload_date={upload_date}; {}",
                description.chars().take(1_000).collect::<String>()
            ),
            source_type: "youtube_video_search".to_string(),
        });
    }
    if sources.is_empty() {
        return Err("Agent Reach YouTube backend produced no usable sources".to_string());
    }
    Ok(ChannelResearchResult {
        channel: ResearchChannel::Youtube,
        backend: "Agent Reach upstream: yt-dlp".to_string(),
        sources,
    })
}

pub async fn research_channel(
    cwd: &Path,
    channel: ResearchChannel,
    query: &str,
) -> Result<Result<ChannelResearchResult, ChannelUnavailable>, String> {
    let query = safe_query(query)?;
    let reach = runtime_status(cwd).await;
    if reach.observed_version.is_none() {
        return Ok(Err(unsupported(
            channel,
            "Agent Reach is not installed; Arena did not substitute an unqualified platform path",
        )));
    }
    if !reach.version_matches_qualification {
        return Ok(Err(unsupported(
            channel,
            "Agent Reach is installed but its exact version is not qualified by Arena; set ARENA_AGENT_REACH_QUALIFIED_VERSION only after runtime qualification",
        )));
    }
    match channel {
        ResearchChannel::Github => research_github(cwd, &query).await.map(Ok),
        ResearchChannel::Youtube => research_youtube(cwd, &query).await.map(Ok),
        ResearchChannel::Tiktok => Ok(Err(unsupported(
            channel,
            "Global TikTok is not an Arena-qualified Agent Reach channel; Agent Reach documents Douyin separately",
        ))),
        ResearchChannel::Douyin => Ok(Err(unsupported(
            channel,
            "Douyin requires a separately qualified mcporter/MCP backend before Arena may use it",
        ))),
        ResearchChannel::Reddit => Ok(Err(unsupported(
            channel,
            "Reddit requires login/backend qualification before Arena may automate it",
        ))),
        ResearchChannel::X => Ok(Err(unsupported(
            channel,
            "X/Twitter requires credential-safe backend qualification before Arena may automate it",
        ))),
        ResearchChannel::Rss => Ok(Err(unsupported(
            channel,
            "RSS research requires an explicit feed URL rather than a free-text search query",
        ))),
        ResearchChannel::Web | ResearchChannel::ResearchPapers => Ok(Err(unsupported(
            channel,
            "This channel is handled by Arena's existing web-research path, not the Agent Reach adapter",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_unqualified_channels_fail_closed() {
        let unavailable = unsupported(
            ResearchChannel::Tiktok,
            "not qualified",
        );
        assert_eq!(unavailable.channel, ResearchChannel::Tiktok);
        assert!(unavailable.reason.contains("qualified"));
    }

    #[test]
    fn research_query_is_bounded() {
        assert!(safe_query("reusable Tauri orchestration").is_ok());
        assert!(safe_query("").is_err());
        assert!(safe_query("bad\nquery").is_err());
    }
}
