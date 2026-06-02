use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, warn};

use crate::{config::Config, store::jobs::JobStore};

pub struct WebhookState {
    pub config: Arc<Config>,
    pub job_store: Arc<JobStore>,
    pub pool: PgPool,
}

pub async fn handle_github_webhook(
    State(state): State<Arc<WebhookState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Err(e) = verify_signature(&state.config.github_webhook_secret, &headers, &body) {
        warn!("Webhook signature rejected: {e}");
        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
    }

    let event_type = match headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
    {
        Some(t) => t.to_owned(),
        None => return (StatusCode::BAD_REQUEST, "missing X-GitHub-Event").into_response(),
    };

    match event_type.as_str() {
        "pull_request" => handle_pull_request(state, body).await,
        "ping" => (StatusCode::OK, "pong").into_response(),
        _ => (StatusCode::OK, "ignored").into_response(),
    }
}

async fn handle_pull_request(
    state: Arc<WebhookState>,
    body: Bytes,
) -> axum::response::Response {
    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            warn!("Failed to parse PR payload: {e}");
            return (StatusCode::BAD_REQUEST, "invalid json").into_response();
        }
    };

    let action = payload["action"].as_str().unwrap_or("");
    if !matches!(action, "opened" | "synchronize" | "reopened") {
        return (StatusCode::OK, "ignored action").into_response();
    }

    let repo_id = payload["repository"]["id"].as_i64().unwrap_or(0);
    let repo_full_name = payload["repository"]["full_name"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let pr_number = payload["pull_request"]["number"].as_i64().unwrap_or(0) as i32;
    let installation_id = payload["installation"]["id"].as_i64().unwrap_or(0);
    let head_sha = payload["pull_request"]["head"]["sha"]
        .as_str()
        .unwrap_or("")
        .to_owned();

    if repo_id == 0 || pr_number == 0 || head_sha.is_empty() {
        warn!("PR payload missing required fields");
        return (StatusCode::BAD_REQUEST, "missing required fields").into_response();
    }

    info!(
        repo = %repo_full_name,
        pr = pr_number,
        sha = %head_sha,
        action,
        "Received PR event"
    );

    if let Err(e) = state
        .job_store
        .upsert_pr_and_enqueue(repo_id, &repo_full_name, pr_number, installation_id, &head_sha)
        .await
    {
        warn!("Failed to enqueue job: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "enqueue failed").into_response();
    }

    (StatusCode::ACCEPTED, "queued").into_response()
}

fn verify_signature(secret: &str, headers: &HeaderMap, body: &Bytes) -> anyhow::Result<()> {
    let sig_header = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("missing X-Hub-Signature-256 header"))?;

    let sig_hex = sig_header
        .strip_prefix("sha256=")
        .ok_or_else(|| anyhow::anyhow!("unexpected signature format"))?;

    let sig_bytes = hex::decode(sig_hex)?;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    mac.update(body);
    mac.verify_slice(&sig_bytes)
        .map_err(|_| anyhow::anyhow!("signature mismatch"))?;

    Ok(())
}
