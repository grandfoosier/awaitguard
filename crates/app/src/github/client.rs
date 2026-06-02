use anyhow::{Context, Result};
use reqwest::{header, Client};
use serde_json::json;

use crate::{config::Config, github::models::ChangedFile};
use super::auth;

pub struct GitHubClient {
    http: Client,
    token: String,
}

impl GitHubClient {
    pub async fn for_installation(config: &Config, installation_id: i64) -> Result<Self> {
        let jwt = auth::create_app_jwt(config.github_app_id, &config.github_private_key_pem)?;
        let token = auth::get_installation_token(&jwt, installation_id).await?;

        let mut default_headers = header::HeaderMap::new();
        default_headers.insert(
            header::ACCEPT,
            "application/vnd.github+json".parse().unwrap(),
        );
        default_headers.insert(
            "X-GitHub-Api-Version",
            "2022-11-28".parse().unwrap(),
        );

        let http = Client::builder()
            .default_headers(default_headers)
            .user_agent("awaitguard/0.1")
            .build()?;

        Ok(Self { http, token })
    }

    pub async fn get_pr_files(
        &self,
        owner: &str,
        repo: &str,
        pr_number: i32,
    ) -> Result<Vec<ChangedFile>> {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}/files"
        );

        let resp: Vec<serde_json::Value> = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .query(&[("per_page", "100")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let files = resp
            .into_iter()
            .map(|f| {
                let path = f["filename"].as_str().unwrap_or("").to_owned();
                let is_rust = path.ends_with(".rs");
                let patch = f["patch"].as_str().map(|s| s.to_owned());
                ChangedFile { path, patch, is_rust }
            })
            .collect();

        Ok(files)
    }

    pub async fn pr_has_label(
        &self,
        owner: &str,
        repo: &str,
        pr_number: i32,
        label: &str,
    ) -> Result<bool> {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/issues/{pr_number}/labels"
        );
        let resp: Vec<serde_json::Value> = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.iter().any(|l| l["name"].as_str() == Some(label)))
    }

    /// Create a new comment or update an existing one. Returns the comment ID.
    pub async fn upsert_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_number: i32,
        existing_comment_id: Option<i64>,
        body: &str,
    ) -> Result<i64> {
        let resp: serde_json::Value = if let Some(id) = existing_comment_id {
            let url = format!(
                "https://api.github.com/repos/{owner}/{repo}/issues/comments/{id}"
            );
            let r = self.http
                .patch(&url)
                .bearer_auth(&self.token)
                .json(&json!({ "body": body }))
                .send()
                .await?;
            check_status(r).await?
        } else {
            let url = format!(
                "https://api.github.com/repos/{owner}/{repo}/issues/{pr_number}/comments"
            );
            let r = self.http
                .post(&url)
                .bearer_auth(&self.token)
                .json(&json!({ "body": body }))
                .send()
                .await?;
            check_status(r).await?
        };

        resp["id"]
            .as_i64()
            .context("GitHub response missing comment id")
    }
}

async fn check_status(resp: reqwest::Response) -> Result<serde_json::Value> {
    if resp.status().is_success() {
        return Ok(resp.json().await?);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    anyhow::bail!("GitHub API error {status}: {body}");
}
