use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub struct FindingsStore {
    pool: PgPool,
}

impl FindingsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn save(
        &self,
        repo_id: i64,
        pr_number: i32,
        head_sha: &str,
        risk_score: f32,
        risk_level: &str,
        findings: &serde_json::Value,
    ) -> Result<()> {
        let hash = findings_hash(findings);
        sqlx::query(
            r#"
            INSERT INTO analysis_runs
                (repo_id, pr_number, head_sha, risk_score, risk_level, findings_json, findings_hash)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(repo_id)
        .bind(pr_number)
        .bind(head_sha)
        .bind(risk_score as f64)
        .bind(risk_level)
        .bind(findings)
        .bind(&hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn findings_hash(v: &serde_json::Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(v.to_string().as_bytes());
    hex::encode(hasher.finalize())
}
