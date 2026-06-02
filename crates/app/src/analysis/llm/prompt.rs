pub fn explain_prompt(detector_id: &str, title: &str, snippet: &str) -> String {
    format!(
        r#"You are a Rust async concurrency expert. Explain this issue briefly.

Issue: {title}
Detector: {detector_id}
Code snippet:
```rust
{snippet}
```

Respond in exactly this format (no preamble):
why: <one sentence explaining the concurrency hazard>
fix: <one sentence concrete fix specific to the code shown>"#
    )
}
