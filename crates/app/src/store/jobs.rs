use anyhow::Result;
use sqlx::PgPool;
use tracing::info;

#[derive(Debug, sqlx::FromRow)]
pub struct Job {
    pub id: i64,
    pub repo_id: i64,
    pub repo_full_name: String,
    pub pr_number: i32,
    pub installation_id: i64,
    pub head_sha: String,
    pub attempt: i32,
    pub status: String,
}

pub struct JobStore {
    pub pool: PgPool,
}

impl JobStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upsert the PR row and enqueue a job only if the head SHA changed.
    pub async fn upsert_pr_and_enqueue(
        &self,
        repo_id: i64,
        repo_full_name: &str,
        pr_number: i32,
        installation_id: i64,
        head_sha: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT head_sha FROM prs WHERE repo_id = $1 AND pr_number = $2",
        )
        .bind(repo_id)
        .bind(pr_number)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((existing_sha,)) = existing {
            if existing_sha == head_sha {
                return Ok(());
            }
        }

        sqlx::query(
            r#"
            INSERT INTO prs (repo_id, pr_number, installation_id, head_sha, last_status, updated_at)
            VALUES ($1, $2, $3, $4, 'new', now())
            ON CONFLICT (repo_id, pr_number) DO UPDATE SET
                head_sha    = EXCLUDED.head_sha,
                last_status = 'new',
                updated_at  = now()
            "#,
        )
        .bind(repo_id)
        .bind(pr_number)
        .bind(installation_id)
        .bind(head_sha)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO jobs (kind, repo_id, repo_full_name, pr_number, installation_id, head_sha)
            VALUES ('analyze_pr', $1, $2, $3, $4, $5)
            "#,
        )
        .bind(repo_id)
        .bind(repo_full_name)
        .bind(pr_number)
        .bind(installation_id)
        .bind(head_sha)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        info!(repo = %repo_full_name, pr = pr_number, sha = %head_sha, "Job enqueued");
        Ok(())
    }

    /// Claim up to `limit` queued jobs atomically using SKIP LOCKED.
    pub async fn dequeue_batch(&self, limit: i64) -> Result<Vec<Job>> {
        let jobs = sqlx::query_as::<_, Job>(
            r#"
            UPDATE jobs SET status = 'running', updated_at = now()
            WHERE id IN (
                SELECT id FROM jobs
                WHERE status = 'queued' AND run_after <= now()
                ORDER BY id
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, repo_id, repo_full_name, pr_number, installation_id, head_sha, attempt, status
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(jobs)
    }

    pub async fn mark_done(&self, job_id: i64) -> Result<()> {
        sqlx::query("UPDATE jobs SET status = 'done', updated_at = now() WHERE id = $1")
            .bind(job_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_failed(&self, job_id: i64, error: &str, max_attempts: i32) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs SET
                attempt    = attempt + 1,
                last_error = $2,
                status     = CASE WHEN attempt + 1 >= $3 THEN 'dead' ELSE 'queued' END,
                run_after  = CASE
                    WHEN attempt + 1 >= $3 THEN run_after
                    ELSE now() + (interval '1 second' * LEAST(POWER(2.0, (attempt + 1)::float)::int, 600))
                END,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(error)
        .bind(max_attempts)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
