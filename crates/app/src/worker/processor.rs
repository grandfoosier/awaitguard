use std::sync::Arc;
use anyhow::Result;
use tracing::{info, instrument, warn};

use crate::{
    analysis::{engine, formatter, llm::{cache::ExplainCache, client::LlmClient}, RiskLevel},
    config::Config,
    github::client::GitHubClient,
    store::{comments::CommentStore, findings::FindingsStore, jobs::{Job, JobStore}},
};

const SUPPRESSION_LABEL: &str = "concurrency-ok";

#[instrument(
    skip_all,
    fields(job_id = job.id, repo = %job.repo_full_name, pr = job.pr_number, sha = %job.head_sha)
)]
pub async fn process(
    job: Job,
    config: Arc<Config>,
    pool: sqlx::PgPool,
    job_store: Arc<JobStore>,
) -> Result<()> {
    let github = GitHubClient::for_installation(&config, job.installation_id).await?;

    tracing::info!(installation_id = job.installation_id, "Using installation token");

    let parts: Vec<&str> = job.repo_full_name.splitn(2, '/').collect();
    let (owner, repo) = (parts[0], parts[1]);

    let comment_store = CommentStore::new(pool.clone());
    let existing_comment_id = comment_store.get_comment_id(job.repo_id, job.pr_number).await?;

    // Check suppression label before doing any analysis work
    if github.pr_has_label(owner, repo, job.pr_number, SUPPRESSION_LABEL).await? {
        info!("PR has suppression label, skipping analysis");
        github
            .upsert_comment(
                owner,
                repo,
                job.pr_number,
                existing_comment_id,
                "_Concurrency analysis suppressed via `concurrency-ok` label._",
            )
            .await?;
        job_store.mark_done(job.id).await?;
        return Ok(());
    }

    // Post "working" placeholder so the user sees something immediately
    let comment_id = github
        .upsert_comment(
            owner,
            repo,
            job.pr_number,
            existing_comment_id,
            "_Concurrency analysis in progress..._",
        )
        .await?;
    comment_store.set_comment_id(job.repo_id, job.pr_number, comment_id).await?;

    // Fetch and filter changed files
    let all_files = github.get_pr_files(owner, repo, job.pr_number).await?;
    let rust_files: Vec<_> = all_files
        .into_iter()
        .filter(|f| f.is_rust)
        .take(config.max_files)
        .collect();

    let mut result = engine::analyze(&rust_files, &config).await?;

    // Optional LLM explanation step for the top findings
    if config.llm_enabled {
        if let Some(api_key) = &config.llm_api_key {
            let llm = LlmClient::new(
                config.llm_model.clone(),
                api_key.clone(),
                config.llm_timeout_secs,
            );
            let explain_cache = ExplainCache::new(pool.clone());

            for finding in result.findings.iter_mut().take(config.max_llm_explains) {
                let key = ExplainCache::make_key(&finding.detector_id, &finding.snippet);

                let (why, fix) = if let Ok(Some(cached)) = explain_cache.get(&key).await {
                    cached
                } else {
                    match llm.explain(&finding.detector_id, &finding.title, &finding.snippet).await {
                        Ok((why, fix)) => {
                            let _ = explain_cache.set(&key, &why, &fix).await;
                            (why, fix)
                        }
                        Err(e) => {
                            warn!("LLM explain failed for {}: {e}", finding.detector_id);
                            continue;
                        }
                    }
                };

                if !why.is_empty() {
                    finding.details = Some(why);
                }
                if !fix.is_empty() {
                    finding.suggestion = Some(fix);
                }
            }
        } else {
            warn!("LLM_ENABLED=true but LLM_API_KEY is not set");
        }
    }

    // Format and post final comment (updates the "working" placeholder)
    let body = formatter::format_comment(owner, repo, job.pr_number, &job.head_sha, &result);
    github
        .upsert_comment(owner, repo, job.pr_number, Some(comment_id), &body)
        .await?;

    // Persist findings for caching / debugging
    let findings_json = serde_json::to_value(&result.findings)?;
    let level_str = match result.risk_level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
    };
    FindingsStore::new(pool)
        .save(
            job.repo_id,
            job.pr_number,
            &job.head_sha,
            result.risk_score,
            level_str,
            &findings_json,
        )
        .await?;

    job_store.mark_done(job.id).await?;
    info!(
        risk_level = level_str,
        findings = result.findings.len(),
        "Job complete"
    );
    Ok(())
}
