use anyhow::Result;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Serialize)]
struct AppClaims {
    iat: i64,
    exp: i64,
    iss: String,
}

pub fn create_app_jwt(app_id: u64, pem: &str) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = AppClaims {
        iat: now - 60,
        exp: now + 60 * 9,
        iss: app_id.to_string(),
    };
    let key = EncodingKey::from_rsa_pem(pem.as_bytes())?;
    let token = encode(&Header::new(Algorithm::RS256), &claims, &key)?;
    Ok(token)
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
    permissions: Option<serde_json::Value>,
    repository_selection: Option<String>,
}

pub async fn get_installation_token(jwt: &str, installation_id: i64) -> Result<String> {
    let url = format!(
        "https://api.github.com/app/installations/{installation_id}/access_tokens"
    );
    let resp: TokenResponse = reqwest::Client::new()
        .post(&url)
        .bearer_auth(jwt)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "awaitguard/0.1")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    info!(
        permissions = %resp.permissions.as_ref().map(|p| p.to_string()).unwrap_or_default(),
        repository_selection = %resp.repository_selection.as_deref().unwrap_or("unknown"),
        "Installation token obtained"
    );
    Ok(resp.token)
}
