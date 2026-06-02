use anyhow::Result;
use serde_json::json;

#[derive(Debug, Clone, Copy)]
enum Provider {
    OpenAi,
    Anthropic,
}

pub struct LlmClient {
    http: reqwest::Client,
    provider: Provider,
    model: String,
    api_key: String,
    timeout_secs: u64,
}

impl LlmClient {
    pub fn new(provider: &str, model: String, api_key: String, timeout_secs: u64) -> Self {
        let provider = match provider {
            "anthropic" => Provider::Anthropic,
            _ => Provider::OpenAi,
        };
        Self {
            http: reqwest::Client::new(),
            provider,
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
        match self.provider {
            Provider::OpenAi => self.explain_openai(&prompt).await,
            Provider::Anthropic => self.explain_anthropic(&prompt).await,
        }
    }

    async fn explain_openai(&self, prompt: &str) -> Result<(String, String)> {
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

        Ok(parse_response(&content))
    }

    async fn explain_anthropic(&self, prompt: &str) -> Result<(String, String)> {
        let resp: serde_json::Value = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 200,
            }))
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let content = resp["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_owned();

        Ok(parse_response(&content))
    }
}

fn parse_response(text: &str) -> (String, String) {
    let why = extract_field(text, "why").unwrap_or_default();
    let fix = extract_field(text, "fix").unwrap_or_default();
    (why, fix)
}

fn extract_field(text: &str, field: &str) -> Option<String> {
    text.lines()
        .find(|l| l.to_lowercase().starts_with(&format!("{field}:")))
        .map(|l| l[field.len() + 1..].trim().to_owned())
}
