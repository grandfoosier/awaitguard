use regex::Regex;
use std::sync::OnceLock;

static HUNK_RE: OnceLock<Regex> = OnceLock::new();

/// Returns a vec parallel to `patch.lines()` mapping each line to its 1-based
/// source file line number in the new version of the file, or `None` for diff
/// headers, hunk markers, and removed lines.
///
/// Requires at least one `@@ -X,Y +A,B @@` hunk header to produce `Some` values;
/// synthetic patches without headers (e.g. in tests) will produce all `None`.
pub fn source_line_map(patch: &str) -> Vec<Option<u32>> {
    let hunk_re = HUNK_RE.get_or_init(|| {
        Regex::new(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@").unwrap()
    });

    let mut result = Vec::new();
    let mut new_line: u32 = 0;

    for line in patch.lines() {
        if let Some(caps) = hunk_re.captures(line) {
            new_line = caps[1].parse().unwrap_or(1);
            result.push(None);
        } else if line.starts_with("---") || line.starts_with("+++") {
            result.push(None);
        } else if line.starts_with('-') {
            // Removed line: no new-file line number, don't advance counter
            result.push(None);
        } else if line.starts_with('+') {
            let n = (new_line > 0).then_some(new_line);
            result.push(n);
            if new_line > 0 {
                new_line += 1;
            }
        } else {
            // Context line (leading space or bare line before first hunk)
            let n = (new_line > 0).then_some(new_line);
            result.push(n);
            if new_line > 0 {
                new_line += 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_added_and_context_lines() {
        let patch = "@@ -10,3 +20,4 @@\n context\n+added_a\n-removed\n context\n+added_b";
        let map = source_line_map(patch);
        // hunk header → None
        assert_eq!(map[0], None);
        // " context" → new file line 20
        assert_eq!(map[1], Some(20));
        // "+added_a" → new file line 21
        assert_eq!(map[2], Some(21));
        // "-removed" → None
        assert_eq!(map[3], None);
        // " context" → new file line 22
        assert_eq!(map[4], Some(22));
        // "+added_b" → new file line 23
        assert_eq!(map[5], Some(23));
    }

    #[test]
    fn no_hunk_header_returns_none() {
        let patch = "+added line\n context";
        let map = source_line_map(patch);
        assert!(map.iter().all(|n| n.is_none()));
    }
}
