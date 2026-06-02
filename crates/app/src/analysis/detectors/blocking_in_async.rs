use regex::Regex;

use crate::{
    analysis::{Finding, Severity},
    github::models::ChangedFile,
};
use super::Detector;

pub struct BlockingInAsync;

struct Pattern {
    re: &'static str,
    title: &'static str,
    severity: Severity,
    suggestion: &'static str,
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        re: r"\bstd::thread::sleep\b",
        title: "std::thread::sleep in async context",
        severity: Severity::High,
        suggestion: "Use tokio::time::sleep instead.",
    },
    Pattern {
        re: r"\bthread::sleep\b",
        title: "thread::sleep in async context",
        severity: Severity::High,
        suggestion: "Use tokio::time::sleep instead.",
    },
    Pattern {
        re: r"\bstd::fs::",
        title: "std::fs blocking I/O in async context",
        severity: Severity::High,
        suggestion: "Use tokio::fs for async file I/O, or spawn_blocking for unavoidable blocking calls.",
    },
    Pattern {
        re: r"\breqwest::blocking::",
        title: "reqwest::blocking in async context",
        severity: Severity::High,
        suggestion: "Use the async reqwest client instead.",
    },
    Pattern {
        re: r"\bstd::sync::Mutex\b",
        title: "std::sync::Mutex in async context",
        severity: Severity::Medium,
        suggestion: "Use tokio::sync::Mutex in async contexts to avoid blocking the executor.",
    },
    Pattern {
        re: r"\bparking_lot::Mutex\b",
        title: "parking_lot::Mutex in async context",
        severity: Severity::Medium,
        suggestion: "Use tokio::sync::Mutex or ensure the critical section is very short.",
    },
    Pattern {
        re: r"\bblock_in_place\b",
        title: "tokio::task::block_in_place usage",
        severity: Severity::Medium,
        suggestion: "Verify this is intentional; prefer spawn_blocking for most blocking work.",
    },
];

impl Detector for BlockingInAsync {
    fn id(&self) -> &'static str {
        "blocking_in_async"
    }

    fn analyze_patch(&self, file: &ChangedFile) -> Vec<Finding> {
        let patch = match &file.patch {
            Some(p) => p,
            None => return vec![],
        };

        let compiled: Vec<Regex> = PATTERNS.iter().map(|p| Regex::new(p.re).unwrap()).collect();
        let mut findings = Vec::new();

        for (line_num, line) in patch.lines().enumerate() {
            if !line.starts_with('+') || line.starts_with("+++") {
                continue;
            }
            let content = line.trim_start_matches('+').trim();
            if content.starts_with("//") {
                continue;
            }

            for (i, re) in compiled.iter().enumerate() {
                if re.is_match(content) {
                    let p = &PATTERNS[i];
                    findings.push(Finding {
                        detector_id: self.id().to_owned(),
                        title: p.title.to_owned(),
                        severity: p.severity,
                        path: file.path.clone(),
                        line_hint: Some(line_num as u32 + 1),
                        snippet: content.to_owned(),
                        details: None,
                        suggestion: Some(p.suggestion.to_owned()),
                    });
                    break;
                }
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::models::ChangedFile;

    fn file(patch: &str) -> ChangedFile {
        ChangedFile {
            path: "src/lib.rs".into(),
            patch: Some(patch.into()),
            is_rust: true,
        }
    }

    #[test]
    fn detects_thread_sleep() {
        let findings = BlockingInAsync.analyze_patch(&file("+    std::thread::sleep(dur);"));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn ignores_commented_lines() {
        let findings = BlockingInAsync.analyze_patch(&file("+    // std::thread::sleep(dur);"));
        assert!(findings.is_empty());
    }
}
