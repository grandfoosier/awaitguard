use anyhow::Result;
use std::collections::HashSet;
use tracing::info;

use crate::{config::Config, github::models::ChangedFile};
use super::{AnalysisResult, detectors, scoring};

pub fn analyze(files: &[ChangedFile], config: &Config) -> Result<AnalysisResult> {
    let detectors = detectors::all();
    let mut all_findings = Vec::new();

    let rust_count = files.iter().filter(|f| f.is_rust).count();
    info!(total_files = files.len(), rust_files = rust_count, "Engine: starting analysis");

    for file in files {
        let patch_len = file.patch.as_deref().unwrap_or("").len();
        info!(
            path = %file.path,
            patch_bytes = patch_len,
            has_patch = file.patch.is_some(),
            "Engine: analyzing file"
        );
        if patch_len > config.max_patch_bytes {
            continue;
        }
        for det in &detectors {
            let found = det.analyze_patch(file);
            info!(detector = det.id(), findings = found.len(), path = %file.path, "Engine: detector result");
            all_findings.extend(found);
        }
    }

    // Deduplicate by (detector_id, path, snippet) — same pattern in different files is distinct.
    // Use retain + HashSet so order is preserved and non-consecutive duplicates are caught.
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    all_findings.retain(|f| seen.insert((f.detector_id.clone(), f.path.clone(), f.snippet.clone())));

    // Sort by severity descending, cap at limit
    all_findings.sort_by(|a, b| b.severity.cmp(&a.severity));
    all_findings.truncate(config.max_findings);

    let (risk_score, risk_level) = scoring::score(&all_findings);

    Ok(AnalysisResult {
        risk_score,
        risk_level,
        findings: all_findings,
        files_scanned: files.len(),
        rust_files_scanned: rust_count,
    })
}
