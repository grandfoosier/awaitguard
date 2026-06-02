use sqlx::PgPool;
use tokio::time;
use tracing::{info, warn};

/// Periodically resets jobs that got stuck in `running` state (crashed worker,
/// timed-out task) back to `queued` so they can be retried.
pub async fn run(pool: PgPool, lock_timeout_secs: u64) {
    let mut interval = time::interval(time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        match reset_stuck(&pool, lock_timeout_secs).await {
            Ok(0) => {}
            Ok(n) => info!(n, "Sweeper reset stuck jobs"),
            Err(e) => warn!("Sweeper error: {e}"),
        }
    }
}

async fn reset_stuck(pool: &PgPool, timeout_secs: u64) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE jobs
           SET status = 'queued', updated_at = now()
         WHERE status = 'running'
           AND updated_at < now() - ($1::bigint * interval '1 second')
        "#,
    )
    .bind(timeout_secs as i64)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
