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

    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT explanation_md FROM llm_explain_cache WHERE key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(v,)| v))
    }

    pub async fn set(&self, key: &str, explanation: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO llm_explain_cache (key, explanation_md)
            VALUES ($1, $2)
            ON CONFLICT (key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(explanation)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
