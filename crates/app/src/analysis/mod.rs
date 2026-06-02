pub mod detectors;
pub mod engine;
pub mod formatter;
pub mod llm;
pub mod patch;
pub mod scoring;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub detector_id: String,
    pub title: String,
    pub severity: Severity,
    pub path: String,
    pub line_hint: Option<u32>,
    pub snippet: String,
    pub details: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub risk_score: f32,
    pub risk_level: RiskLevel,
    pub findings: Vec<Finding>,
    pub files_scanned: usize,
    pub rust_files_scanned: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}
