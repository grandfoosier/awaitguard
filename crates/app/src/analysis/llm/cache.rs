use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub struct ExplainCache {
    pool: PgPool,
}

impl ExplainCache {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn make_key(detector_id: &str, snippet: &str) -> String {
        let mut h = Sha256::new();
        h.update(detector_id.as_bytes());
        h.update(b"|");
        h.update(snippet.as_bytes());
        hex::encode(h.finalize())
    }

    /// Returns (why, fix) if cached.
    pub async fn get(&self, key: &str) -> Result<Option<(String, String)>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT explanation_md FROM llm_explain_cache WHERE key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(v,)| {
            let parsed: serde_json::Value = serde_json::from_str(&v).ok()?;
            let why = parsed["why"].as_str().unwrap_or("").to_owned();
            let fix = parsed["fix"].as_str().unwrap_or("").to_owned();
            Some((why, fix))
        }))
    }

    pub async fn set(&self, key: &str, why: &str, fix: &str) -> Result<()> {
        let json = serde_json::json!({"why": why, "fix": fix}).to_string();
        sqlx::query(
            r#"
            INSERT INTO llm_explain_cache (key, explanation_md)
            VALUES ($1, $2)
            ON CONFLICT (key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
