//! Deterministic, non-mutating documentation drift signals.
//!
//! This deliberately reports only bounded source-path references. A worker
//! may propose a correction, but this scanner never edits or commits docs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_DOCUMENT_BYTES: u64 = 512 * 1024;
const MAX_REPORT_ITEMS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftFinding {
    pub document: String,
    pub reference: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftReport {
    pub candidate_sha: String,
    pub findings: Vec<DriftFinding>,
    pub bounded: bool,
}

fn looks_like_source_path(value: &str) -> bool {
    [".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".html", ".css"]
        .iter()
        .any(|suffix| value.ends_with(suffix))
}

fn backtick_references(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').enumerate().filter_map(|(index, part)| {
        (index % 2 == 1 && looks_like_source_path(part.trim())).then_some(part.trim())
    })
}

pub fn scan_documents(
    repository: &Path,
    candidate_sha: &str,
    documents: &[PathBuf],
) -> Result<DriftReport, String> {
    let mut findings = Vec::new();
    let mut bounded = false;
    for document in documents {
        let metadata = std::fs::metadata(document)
            .map_err(|error| format!("inspect documentation file: {error}"))?;
        if metadata.len() > MAX_DOCUMENT_BYTES {
            bounded = true;
            continue;
        }
        let text = std::fs::read_to_string(document)
            .map_err(|error| format!("read documentation file: {error}"))?;
        for reference in backtick_references(&text) {
            let normalized = reference.trim_start_matches('/');
            if !repository.join(normalized).is_file() {
                findings.push(DriftFinding {
                    document: document.to_string_lossy().into_owned(),
                    reference: reference.to_string(),
                    reason: "documented source path does not exist at candidate HEAD".to_string(),
                });
                if findings.len() >= MAX_REPORT_ITEMS {
                    bounded = true;
                    return Ok(DriftReport {
                        candidate_sha: candidate_sha.to_string(),
                        findings,
                        bounded,
                    });
                }
            }
        }
    }
    Ok(DriftReport {
        candidate_sha: candidate_sha.to_string(),
        findings,
        bounded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_renamed_source_is_reported_without_mutating_docs() {
        let root = std::env::temp_dir().join(format!("arena-doc-drift-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source");
        std::fs::create_dir_all(root.join("docs")).expect("create docs");
        std::fs::write(root.join("src/new.rs"), "fn new_name() {}\n").expect("write source");
        let doc = root.join("docs/current.md");
        std::fs::write(&doc, "The implementation is in `src/old.rs`.\n").expect("write doc");
        let before = std::fs::read_to_string(&doc).expect("read doc");
        let report = scan_documents(&root, "candidate-sha", std::slice::from_ref(&doc))
            .expect("scan should complete");
        assert_eq!(report.candidate_sha, "candidate-sha");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(std::fs::read_to_string(doc).expect("read doc"), before);
        let _ = std::fs::remove_dir_all(root);
    }
}
