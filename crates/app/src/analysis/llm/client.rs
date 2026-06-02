use anyhow::Result;
use serde_json::json;

pub struct LlmClient {
    http: reqwest::Client,
    model: String,
    api_key: String,
    timeout_secs: u64,
}

impl LlmClient {
    pub fn new(model: String, api_key: String, timeout_secs: u64) -> Self {
        Self {
            http: reqwest::Client::new(),
            model,
            api_key,
            timeout_secs,
        }
    }

    /// Returns (why, fix) strings. Falls back to empty strings on any error.
    pub async fn explain(
        &self,
        detector_id: &str,
        title: &str,
        snippet: &str,
    ) -> Result<(String, String)> {
        let prompt = super::prompt::explain_prompt(detector_id, title, snippet);

        let resp: serde_json::Value = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 200,
                "temperature": 0.2,
            }))
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_owned();

        let why = extract_field(&content, "why").unwrap_or_default();
        let fix = extract_field(&content, "fix").unwrap_or_default();

        Ok((why, fix))
    }
}

fn extract_field(text: &str, field: &str) -> Option<String> {
    text.lines()
        .find(|l| l.to_lowercase().starts_with(&format!("{field}:")))
        .map(|l| l[field.len() + 1..].trim().to_owned())
}
