use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub github_app_id: u64,
    pub github_private_key_pem: String,
    pub github_webhook_secret: String,
    pub database_url: String,
    pub http_bind: String,
    pub mode: String,
    pub worker_concurrency: usize,
    pub job_max_attempts: i32,
    pub job_lock_timeout_secs: u64,
    pub max_files: usize,
    pub max_patch_bytes: usize,
    pub max_findings: usize,
    pub max_llm_explains: usize,
    pub llm_enabled: bool,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_api_key: Option<String>,
    pub llm_timeout_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            github_app_id: env_req("GITHUB_APP_ID")?
                .parse()
                .context("GITHUB_APP_ID must be a number")?,
            github_private_key_pem: env_req("GITHUB_PRIVATE_KEY_PEM")?.replace("\\n", "\n"),
            github_webhook_secret: env_req("GITHUB_WEBHOOK_SECRET")?,
            database_url: env_req("DATABASE_URL")?,
            http_bind: env_opt("HTTP_BIND", "0.0.0.0:8080"),
            mode: env_opt("MODE", "all"),
            worker_concurrency: env_opt("WORKER_CONCURRENCY", "4")
                .parse()
                .context("WORKER_CONCURRENCY must be a number")?,
            job_max_attempts: env_opt("JOB_MAX_ATTEMPTS", "5")
                .parse()
                .context("JOB_MAX_ATTEMPTS must be a number")?,
            job_lock_timeout_secs: env_opt("JOB_LOCK_TIMEOUT_SECS", "300")
                .parse()
                .context("JOB_LOCK_TIMEOUT_SECS must be a number")?,
            max_files: env_opt("MAX_FILES", "40")
                .parse()
                .context("MAX_FILES must be a number")?,
            max_patch_bytes: env_opt("MAX_PATCH_BYTES", "200000")
                .parse()
                .context("MAX_PATCH_BYTES must be a number")?,
            max_findings: env_opt("MAX_FINDINGS", "20")
                .parse()
                .context("MAX_FINDINGS must be a number")?,
            max_llm_explains: env_opt("MAX_LLM_EXPLAINS", "3")
                .parse()
                .context("MAX_LLM_EXPLAINS must be a number")?,
            llm_enabled: env_opt("LLM_ENABLED", "false")
                .parse()
                .context("LLM_ENABLED must be true or false")?,
            llm_provider: env_opt("LLM_PROVIDER", "openai"),
            llm_model: env_opt("LLM_MODEL", "gpt-4.1-mini"),
            llm_api_key: std::env::var("LLM_API_KEY").ok(),
            llm_timeout_secs: env_opt("LLM_TIMEOUT_SECS", "20")
                .parse()
                .context("LLM_TIMEOUT_SECS must be a number")?,
        })
    }
}

fn env_req(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

fn env_opt(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}
