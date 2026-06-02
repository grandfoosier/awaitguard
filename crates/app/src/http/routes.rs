use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::http::github_webhook::{handle_github_webhook, WebhookState};

pub fn router(state: Arc<WebhookState>) -> Router {
    Router::new()
        .route("/webhook/github", post(handle_github_webhook))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(state): State<Arc<WebhookState>>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (StatusCode::OK, "ready").into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, format!("db: {e}")).into_response(),
    }
}
