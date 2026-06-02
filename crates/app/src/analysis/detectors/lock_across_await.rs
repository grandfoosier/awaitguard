use regex::Regex;

use crate::{
    analysis::{Finding, Severity},
    github::models::ChangedFile,
};
use super::Detector;

pub struct LockAcrossAwait;

impl Detector for LockAcrossAwait {
    fn id(&self) -> &'static str {
        "lock_across_await"
    }

    fn analyze_patch(&self, file: &ChangedFile) -> Vec<Finding> {
        let patch = match &file.patch {
            Some(p) => p,
            None => return vec![],
        };

        let lock_re =
            Regex::new(r"let\s+(?:mut\s+)?(\w+)\s*=.*\.(lock|read|write)\(\)\.await").unwrap();
        let await_re = Regex::new(r"\.await").unwrap();

        struct Guard {
            name: String,
            lock_line: String,
            line_hint: u32,
            depth: i32,
            is_added: bool,
        }

        let mut active_guards: Vec<Guard> = Vec::new();
        let mut depth: i32 = 0;
        let mut patch_line: u32 = 0;
        let mut findings = Vec::new();

        for line in patch.lines() {
            // Skip diff headers and removed lines
            if line.starts_with("+++")
                || line.starts_with("---")
                || line.starts_with("@@")
                || line.starts_with('-')
            {
                continue;
            }

            let is_added = line.starts_with('+');
            let content = line.get(1..).unwrap_or("");
            let trimmed = content.trim();

            patch_line += 1;

            // Remove explicitly dropped guards before checking for awaits
            active_guards.retain(|g| {
                !trimmed.contains(&format!("drop({})", g.name))
                    && !trimmed.contains(&format!("drop(&{})", g.name))
            });

            // Fire when a guard is active and this line has .await, provided at least
            // one of (lock acquisition, await line) is new — both being context means
            // the bug pre-existed this PR and we leave it alone.
            // Check BEFORE recording a new guard so the lock().await line itself
            // doesn't trigger against the guard it just acquired.
            if await_re.is_match(trimmed) {
                if let Some(guard) = active_guards.last() {
                    if is_added || guard.is_added {
                    findings.push(Finding {
                        detector_id: self.id().to_owned(),
                        title: "Lock held across await".to_owned(),
                        severity: Severity::High,
                        path: file.path.clone(),
                        line_hint: Some(guard.line_hint),
                        snippet: format!("{}\n...\n{}", guard.lock_line, trimmed),
                        details: None,
                        suggestion: Some(
                            "Drop the guard before awaiting. \
                            Extract needed data first, or scope the guard with braces."
                                .to_owned(),
                        ),
                    });
                    }
                }
            }

            // Record a new guard from this line (context or added)
            if let Some(caps) = lock_re.captures(content) {
                let name = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                active_guards.push(Guard {
                    name,
                    lock_line: trimmed.to_owned(),
                    line_hint: patch_line,
                    depth,
                    is_added,
                });
            }

            // Update brace depth and evict guards that went out of scope
            let opens = content.chars().filter(|&c| c == '{').count() as i32;
            let closes = content.chars().filter(|&c| c == '}').count() as i32;
            depth += opens - closes;
            active_guards.retain(|g| depth >= g.depth);
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
    fn detects_lock_across_await() {
        let patch = "+    let mut guard = state.lock().await;\n\
                     +    let result = fetch().await;\n\
                     +    guard.push(result);";
        let findings = LockAcrossAwait.analyze_patch(&file(patch));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].detector_id, "lock_across_await");
    }

    #[test]
    fn detects_context_lock_added_await() {
        // Lock was already present (context line); new await is the added line.
        let patch = [
            " async fn process(&self) {",
            "     let mut map = self.inner.lock().await;",
            "+    tokio::time::sleep(Duration::from_millis(7)).await;",
            " }",
        ].join("\n");
        let findings = LockAcrossAwait.analyze_patch(&file(&patch));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].detector_id, "lock_across_await");
    }

    #[test]
    fn no_false_positive_when_dropped() {
        let patch = "+    let mut guard = state.lock().await;\n\
                     +    let data = guard.clone();\n\
                     +    drop(guard);\n\
                     +    let result = fetch().await;";
        let findings = LockAcrossAwait.analyze_patch(&file(patch));
        assert!(findings.is_empty());
    }

    #[test]
    fn detects_added_lock_context_await() {
        // Await was already present (context line); new lock is the added line.
        let patch = [
            " async fn process(&self) {",
            "+    let mut map = self.inner.lock().await;",
            "     do_something().await;",
            " }",
        ].join("\n");
        let findings = LockAcrossAwait.analyze_patch(&file(&patch));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].detector_id, "lock_across_await");
    }

    #[test]
    fn no_false_positive_both_context() {
        // Both lock and await are unchanged context lines — pre-existing bug, not ours to flag.
        let patch = [
            " async fn process(&self) {",
            "     let mut map = self.inner.lock().await;",
            "     do_something().await;",
            "+    // unrelated change",
            " }",
        ].join("\n");
        let findings = LockAcrossAwait.analyze_patch(&file(&patch));
        assert!(findings.is_empty());
    }

    #[test]
    fn no_false_positive_across_function_boundary() {
        // A lock in one function should not trigger on an await in a sibling function.
        let patch = [
            " fn a() {",
            "     let g = m.lock().await;",
            " }",
            "+fn b() {",
            "+    do_work().await;",
            "+}",
        ].join("\n");
        let findings = LockAcrossAwait.analyze_patch(&file(&patch));
        assert!(findings.is_empty());
    }
}
