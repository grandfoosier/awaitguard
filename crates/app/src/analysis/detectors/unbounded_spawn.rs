use regex::Regex;
use std::sync::OnceLock;

use crate::{
    analysis::{patch::source_line_map, Finding, Severity},
    github::models::ChangedFile,
};
use super::Detector;

static SPAWN_RE: OnceLock<Regex> = OnceLock::new();
static LOOP_RE: OnceLock<Regex> = OnceLock::new();
static JOIN_ALL_RE: OnceLock<Regex> = OnceLock::new();
static FUTURES_UNORDERED_RE: OnceLock<Regex> = OnceLock::new();

fn spawn_re() -> &'static Regex {
    SPAWN_RE.get_or_init(|| Regex::new(r"tokio::spawn\s*\(").unwrap())
}
fn loop_re() -> &'static Regex {
    LOOP_RE.get_or_init(|| Regex::new(r"\b(for\s+\w+\s+in|while\s+|loop\s*\{)").unwrap())
}
fn join_all_re() -> &'static Regex {
    JOIN_ALL_RE.get_or_init(|| Regex::new(r"\bjoin_all\s*\(").unwrap())
}
fn futures_unordered_re() -> &'static Regex {
    FUTURES_UNORDERED_RE.get_or_init(|| Regex::new(r"\bFuturesUnordered::new\s*\(\s*\)").unwrap())
}

pub struct UnboundedSpawn;

impl Detector for UnboundedSpawn {
    fn id(&self) -> &'static str {
        "unbounded_spawn"
    }

    fn analyze_patch(&self, file: &ChangedFile) -> Vec<Finding> {
        let patch = match &file.patch {
            Some(p) => p,
            None => return vec![],
        };

        let spawn_re = spawn_re();
        let loop_re = loop_re();
        let join_all_re = join_all_re();
        let futures_unordered_re = futures_unordered_re();

        // All visible lines (context + added, not removed/headers), preserving order.
        // Used for the loop-lookback window so context loop headers are visible.
        let line_map = source_line_map(patch);
        let visible: Vec<(Option<u32>, bool, &str)> = patch
            .lines()
            .enumerate()
            .filter(|(_, l)| {
                !l.starts_with("+++")
                    && !l.starts_with("---")
                    && !l.starts_with("@@")
                    && !l.starts_with('-')
            })
            .map(|(i, l)| {
                let is_added = l.starts_with('+');
                let content = l.get(1..).unwrap_or("").trim();
                let source_line = line_map.get(i).copied().flatten();
                (source_line, is_added, content)
            })
            .collect();

        let mut findings = Vec::new();

        for (idx, (source_line, is_added, content)) in visible.iter().enumerate() {
            if *is_added && spawn_re.is_match(content) {
                // Look back up to 8 visible lines for a loop construct
                let start = idx.saturating_sub(8);
                let in_loop = visible[start..idx]
                    .iter()
                    .any(|(_, _, prev)| loop_re.is_match(prev));

                if in_loop {
                    findings.push(Finding {
                        detector_id: self.id().to_owned(),
                        title: "Unbounded tokio::spawn inside loop".to_owned(),
                        severity: Severity::High,
                        path: file.path.clone(),
                        line_hint: *source_line,
                        snippet: content.to_string(),
                        details: None,
                        suggestion: Some(
                            "Use a Semaphore or StreamExt::buffer_unordered(n) to bound concurrency."
                                .to_owned(),
                        ),
                    });
                    continue;
                }
            }

            if !is_added {
                continue;
            }

            if join_all_re.is_match(content) {
                findings.push(Finding {
                    detector_id: self.id().to_owned(),
                    title: "join_all on potentially unbounded collection".to_owned(),
                    severity: Severity::Medium,
                    path: file.path.clone(),
                    line_hint: *source_line,
                    snippet: content.to_string(),
                    details: None,
                    suggestion: Some(
                        "Consider futures::stream::iter(...).buffer_unordered(n) for bounded concurrency."
                            .to_owned(),
                    ),
                });
            }

            if futures_unordered_re.is_match(content) {
                findings.push(Finding {
                    detector_id: self.id().to_owned(),
                    title: "FuturesUnordered without explicit bound".to_owned(),
                    severity: Severity::Medium,
                    path: file.path.clone(),
                    line_hint: *source_line,
                    snippet: content.to_string(),
                    details: None,
                    suggestion: Some(
                        "Pair FuturesUnordered with a Semaphore or push limit to prevent unbounded growth."
                            .to_owned(),
                    ),
                });
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
    fn detects_spawn_in_loop() {
        let patch = "+    for item in items {\n\
                     +        tokio::spawn(process(item));\n\
                     +    }";
        let findings = UnboundedSpawn.analyze_patch(&file(patch));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn detects_spawn_in_context_loop() {
        // Loop header is a context line; spawn is newly added inside it.
        let patch = [
            " for item in items {",
            "+    tokio::spawn(process(item));",
            " }",
        ].join("\n");
        let findings = UnboundedSpawn.analyze_patch(&file(&patch));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn detects_join_all() {
        let findings = UnboundedSpawn.analyze_patch(&file("+    join_all(futures).await;"));
        assert!(!findings.is_empty());
    }
}
