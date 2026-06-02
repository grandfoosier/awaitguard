pub mod github_webhook;
pub mod routes;

use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use crate::{config::Config, store::jobs::JobStore};
use github_webhook::WebhookState;

pub async fn serve(config: Arc<Config>, pool: sqlx::PgPool) -> Result<()> {
    let state = Arc::new(WebhookState {
        config: Arc::clone(&config),
        job_store: Arc::new(JobStore::new(pool.clone())),
        pool: pool.clone(),
    });

    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(&config.http_bind).await?;
    info!(bind = %config.http_bind, "HTTP server listening");
    axum::serve(listener, app).await?;
    Ok(())
}
