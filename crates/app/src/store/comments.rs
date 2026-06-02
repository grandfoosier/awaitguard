use anyhow::Result;
use sqlx::PgPool;

pub struct CommentStore {
    pool: PgPool,
}

impl CommentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_comment_id(&self, repo_id: i64, pr_number: i32) -> Result<Option<i64>> {
        let row: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT comment_id FROM prs WHERE repo_id = $1 AND pr_number = $2",
        )
        .bind(repo_id)
        .bind(pr_number)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|(id,)| id))
    }

    pub async fn set_comment_id(
        &self,
        repo_id: i64,
        pr_number: i32,
        comment_id: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE prs SET comment_id = $3, updated_at = now() WHERE repo_id = $1 AND pr_number = $2",
        )
        .bind(repo_id)
        .bind(pr_number)
        .bind(comment_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
