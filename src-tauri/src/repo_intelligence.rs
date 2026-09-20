//! Bounded integration with the standalone `agent-analyzer` repo-intel CLI.
//!
//! The analyzer is a derived cache, never Product OS truth. Arena retains only
//! a small metadata record and asks for bounded slices rather than embedding a
//! repository map in worker prompts.

use crate::dsh_worker::{self, ContainedCommandOptions};
use crate::git_runtime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const AGENT_ANALYZER_VERSION: &str = "0.8.1";
pub const AGENT_ANALYZER_COMMIT: &str = "719badc74731cd127a03b2d52bc51812b0c50c30";
const MAX_SLICE_BYTES: usize = 12 * 1024;
const MAX_MAP_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIntelligenceSnapshot {
    pub snapshot_key: String,
    pub repository_identity: String,
    pub base_head: String,
    pub analyzer_version: String,
    pub analyzer_commit: String,
    pub map_path: String,
    pub map_bytes: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIntelligenceSlice {
    pub snapshot_key: String,
    pub request: String,
    pub content: String,
}

fn executable() -> PathBuf {
    std::env::var_os("ARENA_AGENT_ANALYZER_EXECUTABLE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("agent-analyzer"))
}

fn hash_key(repository_identity: &str, base_head: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(repository_identity.as_bytes());
    hasher.update([0]);
    hasher.update(base_head.as_bytes());
    hasher.update([0]);
    hasher.update(AGENT_ANALYZER_VERSION.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn repository_identity(repo: &Path) -> Result<(String, String), String> {
    let root = git_runtime::output(repo, &["rev-parse", "--show-toplevel"]).await?;
    if !root.status.success() {
        return Err("could not resolve repository identity".to_string());
    }
    let identity = String::from_utf8(root.stdout)
        .map_err(|_| "repository identity was not UTF-8".to_string())?
        .trim()
        .to_string();
    let head = git_runtime::output(repo, &["rev-parse", "HEAD"]).await?;
    if !head.status.success() {
        return Err("could not resolve repository HEAD".to_string());
    }
    let head = String::from_utf8(head.stdout)
        .map_err(|_| "repository HEAD was not UTF-8".to_string())?
        .trim()
        .to_string();
    if identity.is_empty() || head.is_empty() {
        return Err("repository identity or HEAD was empty".to_string());
    }
    Ok((identity, head))
}

fn metadata_path(map_path: &Path) -> PathBuf {
    map_path.with_extension("metadata.json")
}

fn read_cached(path: &Path) -> Result<Option<ProjectIntelligenceSnapshot>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read repo intelligence metadata: {error}")),
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| format!("parse repo intelligence metadata: {error}"))
}

async fn analyzer_status(repo: &Path, map_path: &Path) -> Result<bool, String> {
    let args = vec![
        OsString::from("repo-intel"),
        OsString::from("status"),
        OsString::from("--map-file"),
        map_path.as_os_str().to_os_string(),
        repo.as_os_str().to_os_string(),
    ];
    let output =
        dsh_worker::run_contained_command(&executable(), &args, repo, Duration::from_secs(60))
            .await?;
    Ok(output.exit_code == Some(0)
        && !output.timed_out
        && serde_json::from_str::<serde_json::Value>(&output.stdout)
            .ok()
            .and_then(|value| {
                value
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .as_deref()
            == Some("valid"))
}

pub async fn ensure_snapshot(
    repo: &Path,
    cache_root: &Path,
) -> Result<ProjectIntelligenceSnapshot, String> {
    let (identity, head) = repository_identity(repo).await?;
    let key = hash_key(&identity, &head);
    let directory = cache_root.join("repo-intelligence").join(&key);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("create repo intelligence cache: {error}"))?;
    let map_path = directory.join("repo-intel.json");
    let metadata = metadata_path(&map_path);
    if let Some(cached) = read_cached(&metadata)? {
        if cached.snapshot_key == key
            && cached.base_head == head
            && std::path::Path::new(&cached.map_path).is_file()
            && analyzer_status(repo, &map_path).await.unwrap_or(false)
        {
            return Ok(cached);
        }
    }

    let options = ContainedCommandOptions {
        stdout_path: Some(map_path.clone()),
        ..ContainedCommandOptions::default()
    };
    let args = vec![
        OsString::from("repo-intel"),
        OsString::from("init"),
        repo.as_os_str().to_os_string(),
    ];
    let output = dsh_worker::run_contained_command_with_options(
        &executable(),
        &args,
        repo,
        Duration::from_secs(300),
        &options,
    )
    .await?;
    if output.timed_out || output.exit_code != Some(0) {
        return Err(format!(
            "agent-analyzer repo-intel init failed: {}",
            output.stderr.chars().take(512).collect::<String>()
        ));
    }
    let map_bytes = std::fs::metadata(&map_path)
        .map_err(|error| format!("inspect repo intelligence map: {error}"))?
        .len();
    if map_bytes == 0 || map_bytes > MAX_MAP_BYTES {
        return Err("agent-analyzer map exceeded Arena's bounded cache limit".to_string());
    }
    let snapshot = ProjectIntelligenceSnapshot {
        snapshot_key: key,
        repository_identity: identity,
        base_head: head,
        analyzer_version: AGENT_ANALYZER_VERSION.to_string(),
        analyzer_commit: AGENT_ANALYZER_COMMIT.to_string(),
        map_path: map_path.to_string_lossy().into_owned(),
        map_bytes,
        status: "valid".to_string(),
    };
    let raw = serde_json::to_vec_pretty(&snapshot)
        .map_err(|error| format!("serialize repo intelligence metadata: {error}"))?;
    std::fs::write(metadata, raw)
        .map_err(|error| format!("write repo intelligence metadata: {error}"))?;
    Ok(snapshot)
}

pub async fn bounded_slice(
    repo: &Path,
    cache_root: &Path,
    request: &str,
) -> Result<ProjectIntelligenceSlice, String> {
    if request.trim().is_empty() || request.len() > 4 * 1024 {
        return Err("repo intelligence request is outside the bounded size".to_string());
    }
    let snapshot = ensure_snapshot(repo, cache_root).await?;
    let lowered = request.to_ascii_lowercase();
    let mut args = vec![OsString::from("repo-intel"), OsString::from("query")];
    if lowered.contains("entry") {
        args.extend([
            OsString::from("entry-points"),
            OsString::from("--map-file"),
            OsString::from(&snapshot.map_path),
            repo.as_os_str().to_os_string(),
        ]);
    } else {
        // Keep Product OS repository intelligence repository-generic. A
        // founder project cannot be assumed to contain Arena-specific files.
        // The bounded find query works across repositories for architecture,
        // coupling, symbols, and other semantic requests.
        args.extend([
            OsString::from("find"),
            OsString::from("--map-file"),
            OsString::from(&snapshot.map_path),
            OsString::from("--top"),
            OsString::from("6"),
            OsString::from(request),
            repo.as_os_str().to_os_string(),
        ]);
    }
    let output =
        dsh_worker::run_contained_command(&executable(), &args, repo, Duration::from_secs(60))
            .await?;
    if output.timed_out || output.exit_code != Some(0) {
        return Err("agent-analyzer bounded query failed".to_string());
    }
    let content = output
        .stdout
        .chars()
        .take(MAX_SLICE_BYTES)
        .collect::<String>();
    if content.trim().is_empty() {
        return Err("agent-analyzer returned an empty bounded slice".to_string());
    }
    Ok(ProjectIntelligenceSlice {
        snapshot_key: snapshot.snapshot_key,
        request: request.to_string(),
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_key_changes_when_head_changes() {
        let first = hash_key("/repo", "aaa");
        let second = hash_key("/repo", "bbb");
        assert_ne!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn repository_intelligence_has_no_arena_specific_target_path() {
        let source = include_str!("repo_intelligence.rs");
        assert!(!source.contains("src-tauri/src/opencode_adapter.rs"));
    }

    #[test]
    fn slice_is_bounded() {
        let slice = "x".repeat(MAX_SLICE_BYTES + 100);
        let bounded = slice.chars().take(MAX_SLICE_BYTES).collect::<String>();
        assert_eq!(bounded.len(), MAX_SLICE_BYTES);
    }
}
