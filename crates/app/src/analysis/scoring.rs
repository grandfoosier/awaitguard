use super::{Finding, RiskLevel, Severity};

pub fn score(findings: &[Finding]) -> (f32, RiskLevel) {
    let total: f32 = findings
        .iter()
        .map(|f| match f.severity {
            Severity::High => 6.0,
            Severity::Medium => 3.0,
            Severity::Low => 1.0,
        })
        .sum();

    let capped = total.min(15.0);
    let level = match capped as u32 {
        0..=3 => RiskLevel::Low,
        4..=8 => RiskLevel::Medium,
        _ => RiskLevel::High,
    };

    (capped, level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Finding;

    fn finding(severity: Severity) -> Finding {
        Finding {
            detector_id: "test".into(),
            title: "test".into(),
            severity,
            path: "src/lib.rs".into(),
            line_hint: None,
            snippet: "".into(),
            details: None,
            suggestion: None,
        }
    }

    #[test]
    fn low_score() {
        let (score, level) = score(&[finding(Severity::Low)]);
        assert_eq!(level, RiskLevel::Low);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn high_score_caps_at_15() {
        let findings: Vec<_> = (0..5).map(|_| finding(Severity::High)).collect();
        let (score, level) = super::score(&findings);
        assert_eq!(score, 15.0);
        assert_eq!(level, RiskLevel::High);
    }
}
