use super::{AnalysisResult, RiskLevel, Severity};

pub fn format_comment(
    owner: &str,
    repo: &str,
    pr_number: i32,
    head_sha: &str,
    result: &AnalysisResult,
) -> String {
    let (level_emoji, level_str) = match result.risk_level {
        RiskLevel::Low => ("🟢", "Low"),
        RiskLevel::Medium => ("🟡", "Medium"),
        RiskLevel::High => ("🔴", "High"),
    };

    let short_sha = &head_sha[..head_sha.len().min(7)];
    let mut out = format!(
        "## {level_emoji} Concurrency Risk: **{level_str}** ({:.0}/15)\n\n",
        result.risk_score
    );

    out.push_str(&format!(
        "Analyzed `{owner}/{repo}` PR #{pr_number} at `{short_sha}`.\n"
    ));
    out.push_str(&format!(
        "Files scanned: {} (Rust: {}). Findings: {}.\n",
        result.files_scanned, result.rust_files_scanned, result.findings.len()
    ));

    if result.findings.is_empty() {
        out.push_str("\nNo concurrency issues detected.\n");
    } else {
        out.push_str("\n### Findings\n\n");
        for (i, f) in result.findings.iter().enumerate() {
            let sev_label = match f.severity {
                Severity::High => "🔴 High",
                Severity::Medium => "🟡 Medium",
                Severity::Low => "🔵 Low",
            };
            let line_str = f
                .line_hint
                .map(|l| format!(" (near line {l})"))
                .unwrap_or_default();

            out.push_str(&format!(
                "{}. **{}** — `{}`{} [{sev_label}]\n",
                i + 1,
                f.title,
                f.path,
                line_str,
            ));
            out.push_str(&format!(
                "   ```rust\n   {}\n   ```\n",
                f.snippet.replace('\n', "\n   ")
            ));
            if let Some(details) = &f.details {
                out.push_str(&format!("   - **Why it matters:** {details}\n"));
            }
            if let Some(suggestion) = &f.suggestion {
                out.push_str(&format!("   - **Suggested fix:** {suggestion}\n"));
            }
            out.push('\n');
        }
    }

    out.push_str(
        "<sub>To suppress: add label `concurrency-ok`. \
        Powered by [awaitguard](https://github.com/awaitguard/awaitguard).</sub>\n",
    );
    out
}
