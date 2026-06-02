use std::sync::Arc;
use tokio::{sync::Semaphore, task::JoinSet, time};
use tracing::{error, info, warn};

use crate::{config::Config, store::jobs::JobStore};
use super::{processor, sweeper};

pub async fn run(config: Arc<Config>, pool: sqlx::PgPool) -> anyhow::Result<()> {
    let job_store = Arc::new(JobStore::new(pool.clone()));
    let semaphore = Arc::new(Semaphore::new(config.worker_concurrency));
    let mut join_set: JoinSet<()> = JoinSet::new();

    // Sweeper runs independently; reset jobs stuck in `running`
    let sweep_pool = pool.clone();
    let lock_timeout = config.job_lock_timeout_secs;
    tokio::spawn(async move {
        sweeper::run(sweep_pool, lock_timeout).await;
    });

    info!(concurrency = config.worker_concurrency, "Worker loop started");

    let mut interval = time::interval(time::Duration::from_secs(2));
    loop {
        interval.tick().await;

        // Reap finished tasks before checking available permits
        while join_set.try_join_next().is_some() {}

        let available = semaphore.available_permits();
        if available == 0 {
            continue;
        }

        let jobs = match job_store.dequeue_batch(available as i64).await {
            Ok(j) => j,
            Err(e) => {
                error!("Dequeue error: {e}");
                continue;
            }
        };

        for job in jobs {
            let permit = match Arc::clone(&semaphore).acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };

            let config = Arc::clone(&config);
            let job_store = Arc::clone(&job_store);
            let pool = pool.clone();
            let job_id = job.id;

            join_set.spawn(async move {
                let _permit = permit;
                if let Err(e) =
                    processor::process(job, Arc::clone(&config), pool, Arc::clone(&job_store))
                        .await
                {
                    warn!(job_id, err = %e, "Job failed, scheduling retry");
                    let _ = job_store
                        .mark_failed(job_id, &e.to_string(), config.job_max_attempts)
                        .await;
                }
            });
        }
    }
}
