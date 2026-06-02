use regex::Regex;

use crate::{
    analysis::{Finding, Severity},
    github::models::ChangedFile,
};
use super::Detector;

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

        let spawn_re = Regex::new(r"tokio::spawn\s*\(").unwrap();
        let loop_re = Regex::new(r"\b(for\s+\w+\s+in|while\s+|loop\s*\{)").unwrap();
        let join_all_re = Regex::new(r"\bjoin_all\s*\(").unwrap();
        let futures_unordered_re = Regex::new(r"\bFuturesUnordered::new\s*\(\s*\)").unwrap();

        // All visible lines (context + added, not removed/headers), preserving order.
        // Used for the loop-lookback window so context loop headers are visible.
        let visible: Vec<(u32, bool, &str)> = patch
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
                (i as u32 + 1, is_added, content)
            })
            .collect();

        let mut findings = Vec::new();

        for (idx, (line_num, is_added, content)) in visible.iter().enumerate() {
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
                        line_hint: Some(*line_num),
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
                    line_hint: Some(*line_num),
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
                    line_hint: Some(*line_num),
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
