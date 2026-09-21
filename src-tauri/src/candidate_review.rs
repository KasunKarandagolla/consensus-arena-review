//! Arena-owned, advisory semantic review receipts.
//!
//! This module contains only bounded review data and deterministic receipt
//! rules. It does not own acceptance, verification, ProductAuthority, or
//! Apply decisions.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MAX_CONTEXT_BYTES: usize = 48 * 1024;
pub const MAX_REVIEW_TEXT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLens {
    TestQuality,
    ErrorHandling,
    TypeApiDesign,
    Maintainability,
    CommentDocsAccuracy,
    Simplification,
}

impl ReviewLens {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TestQuality => "test_quality",
            Self::ErrorHandling => "error_handling",
            Self::TypeApiDesign => "type_api_design",
            Self::Maintainability => "maintainability",
            Self::CommentDocsAccuracy => "comment_docs_accuracy",
            Self::Simplification => "simplification",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLensStatus {
    Complete,
    Unavailable,
    Failed,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewLensState {
    pub reviewer_type: ReviewLens,
    pub status: ReviewLensStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffCoverageStatus {
    Included,
    Excerpted,
    Omitted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateDiffFile {
    pub path: String,
    pub status: DiffCoverageStatus,
    pub excerpt_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateDiffManifest {
    pub files: Vec<CandidateDiffFile>,
    pub total_changed_files: usize,
    pub omitted_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingDisposition {
    BlockingRepair,
    NonblockingWarning,
    FalseUnsupported,
    AlreadyCovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticReviewFinding {
    pub finding_id: String,
    pub reviewer_type: ReviewLens,
    pub severity: String,
    pub confidence: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub finding: String,
    pub evidence: String,
    pub recommended_disposition: FindingDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticReviewReceipt {
    pub receipt_id: String,
    pub candidate_sha: String,
    pub acceptance_commit: String,
    pub reviewer_type: ReviewLens,
    pub findings: Vec<SemanticReviewFinding>,
    pub source_session_id: String,
    pub advisory_only: bool,
}

impl SemanticReviewReceipt {
    pub fn has_blocking_finding(&self) -> bool {
        self.findings.iter().any(|finding| {
            matches!(
                finding.recommended_disposition,
                FindingDisposition::BlockingRepair
            )
        })
    }

    pub fn is_current_for(&self, candidate_sha: &str, acceptance_commit: &str) -> bool {
        self.advisory_only
            && self.candidate_sha == candidate_sha
            && self.acceptance_commit == acceptance_commit
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateReviewSummary {
    pub candidate_sha: String,
    pub acceptance_commit: String,
    pub receipts: Vec<SemanticReviewReceipt>,
    pub deduplicated_findings: Vec<SemanticReviewFinding>,
    pub lens_states: Vec<ReviewLensState>,
    pub diff_manifest: CandidateDiffManifest,
    pub review_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateReviewContext {
    pub candidate_sha: String,
    pub acceptance_commit: String,
    pub diff: String,
    pub diff_manifest: CandidateDiffManifest,
    pub acceptance_summary: String,
    pub repo_intel: Option<String>,
}

impl CandidateReviewContext {
    pub fn bounded_prompt(&self, lens: ReviewLens) -> String {
        let repo_intel = self
            .repo_intel
            .as_deref()
            .unwrap_or("No repo-intelligence slice was available; use only the bounded diff.");
        let manifest = match serde_json::to_string(&self.diff_manifest) {
            Ok(value) => value,
            Err(_) => "{\"error\":\"diff manifest unavailable\"}".to_string(),
        };
        format!(
            "Review exact candidate SHA {candidate} against acceptance commit {acceptance}.\n\n\
You are the {lens} reviewer. This is advisory analysis only. You cannot mark the candidate\n\
Verified, PASS, accepted, or ready to Apply, and you cannot change files. Return only one\n\
JSON object with this shape: {{\"findings\":[{{\"finding_id\":\"stable-id\",\"severity\":\"low|medium|high|critical\",\"confidence\":\"low|medium|high\",\"file\":\"path or null\",\"line\":0,\"finding\":\"...\",\"evidence\":\"...\",\"recommended_disposition\":\"blocking_repair|nonblocking_warning|false_unsupported|already_covered\"}}]}}.\n\
Use an empty array when no material issue is supported by the supplied evidence.\n\n\
Frozen acceptance summary:\n{summary}\n\n\
Changed-file coverage manifest (omissions are explicit and must not be inferred as reviewed):\n{manifest}\n\n\
Bounded repository-intelligence slice:\n{repo_intel}\n\n\
Bounded per-file candidate excerpts:\n{diff}",
            candidate = self.candidate_sha,
            acceptance = self.acceptance_commit,
            lens = lens.as_str(),
            summary = self.acceptance_summary,
            manifest = manifest,
            repo_intel = repo_intel,
            diff = self.diff,
        )
    }
}

pub fn receipt_id(candidate_sha: &str, lens: ReviewLens, session_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(candidate_sha.as_bytes());
    hasher.update([0]);
    hasher.update(lens.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(session_id.as_bytes());
    format!("semantic-{:x}", hasher.finalize())
}

pub fn parse_receipt(
    candidate_sha: &str,
    acceptance_commit: &str,
    lens: ReviewLens,
    session_id: &str,
    text: &str,
) -> Result<SemanticReviewReceipt, String> {
    if text.len() > MAX_REVIEW_TEXT_BYTES {
        return Err("semantic review result exceeded the bounded size".to_string());
    }
    let object = text
        .find('{')
        .and_then(|start| {
            text[start..]
                .rfind('}')
                .map(|end| &text[start..=start + end])
        })
        .ok_or_else(|| "semantic review returned no JSON object".to_string())?;
    #[derive(Deserialize)]
    struct ReviewPayload {
        #[serde(default)]
        findings: Vec<SemanticReviewFindingPayload>,
    }
    #[derive(Deserialize)]
    struct SemanticReviewFindingPayload {
        #[serde(default)]
        finding_id: String,
        #[serde(default)]
        severity: String,
        #[serde(default)]
        confidence: String,
        file: Option<String>,
        line: Option<u32>,
        #[serde(default)]
        finding: String,
        #[serde(default)]
        evidence: String,
        recommended_disposition: FindingDisposition,
    }
    let payload: ReviewPayload = serde_json::from_str(object)
        .map_err(|error| format!("semantic review JSON was invalid: {error}"))?;
    let findings = payload
        .findings
        .into_iter()
        .enumerate()
        .map(|(index, finding)| SemanticReviewFinding {
            finding_id: if finding.finding_id.trim().is_empty() {
                format!("{}-{}", lens.as_str(), index + 1)
            } else {
                finding.finding_id
            },
            reviewer_type: lens,
            severity: finding.severity,
            confidence: finding.confidence,
            file: finding.file,
            line: finding.line,
            finding: finding.finding,
            evidence: finding.evidence,
            recommended_disposition: finding.recommended_disposition,
        })
        .collect();
    Ok(SemanticReviewReceipt {
        receipt_id: receipt_id(candidate_sha, lens, session_id),
        candidate_sha: candidate_sha.to_string(),
        acceptance_commit: acceptance_commit.to_string(),
        reviewer_type: lens,
        findings,
        source_session_id: session_id.to_string(),
        advisory_only: true,
    })
}

pub fn deduplicate(receipts: &[SemanticReviewReceipt]) -> Vec<SemanticReviewFinding> {
    let mut seen = HashSet::new();
    let mut findings = Vec::new();
    for receipt in receipts {
        for finding in &receipt.findings {
            let key = format!(
                "{}|{}|{}|{}",
                finding.file.as_deref().unwrap_or(""),
                finding.line.unwrap_or_default(),
                finding.finding.trim().to_ascii_lowercase(),
                finding.recommended_disposition.as_str()
            );
            if seen.insert(key) {
                findings.push(finding.clone());
            }
        }
    }
    findings
}

impl FindingDisposition {
    fn as_str(&self) -> &'static str {
        match self {
            Self::BlockingRepair => "blocking_repair",
            Self::NonblockingWarning => "nonblocking_warning",
            Self::FalseUnsupported => "false_unsupported",
            Self::AlreadyCovered => "already_covered",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(disposition: &str) -> String {
        format!(
            "{{\"findings\":[{{\"finding_id\":\"f1\",\"severity\":\"high\",\"confidence\":\"high\",\"file\":\"src/lib.rs\",\"line\":4,\"finding\":\"error is swallowed\",\"evidence\":\"return path\",\"recommended_disposition\":\"{disposition}\"}}]}}"
        )
    }

    #[test]
    fn receipt_is_stale_when_candidate_changes() {
        let receipt = parse_receipt(
            "new",
            "acceptance",
            ReviewLens::ErrorHandling,
            "session",
            &json("blocking_repair"),
        )
        .expect("receipt should parse");
        assert!(receipt.is_current_for("new", "acceptance"));
        assert!(!receipt.is_current_for("old", "acceptance"));
        assert!(receipt.has_blocking_finding());
    }

    #[test]
    fn duplicate_findings_are_collapsed_without_scoring() {
        let first = parse_receipt(
            "candidate",
            "acceptance",
            ReviewLens::TestQuality,
            "one",
            &json("nonblocking_warning"),
        )
        .expect("first receipt should parse");
        let second = parse_receipt(
            "candidate",
            "acceptance",
            ReviewLens::Maintainability,
            "two",
            &json("nonblocking_warning"),
        )
        .expect("second receipt should parse");
        assert_eq!(deduplicate(&[first, second]).len(), 1);
    }

    #[test]
    fn review_prompt_carries_exact_candidate_identity_and_is_bounded() {
        let context = CandidateReviewContext {
            candidate_sha: "candidate".to_string(),
            acceptance_commit: "acceptance".to_string(),
            diff: "bounded diff".to_string(),
            diff_manifest: CandidateDiffManifest {
                files: vec![CandidateDiffFile {
                    path: "src/lib.rs".to_string(),
                    status: DiffCoverageStatus::Included,
                    excerpt_bytes: 12,
                }],
                total_changed_files: 1,
                omitted_files: 0,
            },
            acceptance_summary: "frozen check".to_string(),
            repo_intel: None,
        };
        let prompt = context.bounded_prompt(ReviewLens::TestQuality);
        assert!(prompt.contains("candidate"));
        assert!(prompt.contains("frozen check"));
        assert!(prompt.contains("advisory analysis only"));
        assert!(
            prompt.contains("changed-file coverage manifest")
                || prompt.contains("Changed-file coverage manifest")
        );
        assert!(prompt.contains("src/lib.rs"));
    }

    #[test]
    fn manifest_makes_omitted_files_explicit() {
        let manifest = CandidateDiffManifest {
            files: vec![
                CandidateDiffFile {
                    path: "src/large.rs".to_string(),
                    status: DiffCoverageStatus::Omitted,
                    excerpt_bytes: 0,
                },
                CandidateDiffFile {
                    path: "src/small.rs".to_string(),
                    status: DiffCoverageStatus::Included,
                    excerpt_bytes: 40,
                },
            ],
            total_changed_files: 2,
            omitted_files: 1,
        };
        assert_eq!(manifest.omitted_files, 1);
        assert_eq!(manifest.files[0].status, DiffCoverageStatus::Omitted);
    }

    #[test]
    fn reviewer_cannot_promote_its_extra_verified_field() {
        let receipt = parse_receipt(
            "candidate",
            "acceptance",
            ReviewLens::Maintainability,
            "session",
            r#"{"verified":true,"findings":[]}"#,
        )
        .expect("receipt should parse");
        assert!(receipt.advisory_only);
        assert!(!receipt.has_blocking_finding());
    }
}
