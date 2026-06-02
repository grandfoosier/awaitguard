# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`awaitguard` is a GitHub App that automatically reviews Rust pull requests for async concurrency bugs. When a PR is opened or updated, it fetches the diff, runs pattern-based detectors over added lines, optionally enriches findings with LLM explanations, and posts a single upserted comment with a risk score and finding details.

## Commands

```bash
# Build
cargo build --release --bin awaitguard

# Run all tests
cargo test

# Run tests for a specific module
cargo test --test-files crates/app/src/analysis/detectors/lock_across_await.rs
# or by test name
cargo test detects_lock_across_await

# Lint
cargo clippy

# Start Postgres locally
docker-compose up db

# Run the full app locally (requires .env)
cp .env.example .env   # fill in GitHub App credentials
cargo run --bin awaitguard
```

Migrations run automatically at startup via `sqlx::migrate!`. There is no separate migration command.

## Architecture

The app has three runtime modes controlled by `MODE` env var: `api` (HTTP only), `worker` (job processor only), or `all` (both, the default). In `all` mode both are spawned as independent tokio tasks.

**Request → Queue → Worker → GitHub comment flow:**

1. `http/github_webhook.rs` — receives GitHub webhook events, verifies HMAC-SHA256 signature, and for `pull_request` events (opened/synchronize/reopened) calls `JobStore::upsert_pr_and_enqueue`. Jobs are only enqueued when the head SHA changes, preventing duplicate work on label/title edits.

2. `store/jobs.rs` — PostgreSQL job queue using `SELECT ... FOR UPDATE SKIP LOCKED` for concurrent workers. Failed jobs get exponential backoff retries (capped at 600s). `worker/sweeper.rs` periodically resets jobs stuck in `running` beyond `JOB_LOCK_TIMEOUT_SECS`.

3. `worker/runner.rs` — polls the queue every 2 seconds, uses a `Semaphore` to cap concurrency at `WORKER_CONCURRENCY`, spawns each job as a `JoinSet` task.

4. `worker/processor.rs` — the main job handler: checks for the `concurrency-ok` suppression label, posts a "working" placeholder comment, fetches PR files from GitHub, runs the analysis engine, optionally calls the LLM, then upserts the final comment and persists findings.

**Analysis pipeline (`analysis/`):**

- `engine.rs` iterates over Rust files in the diff, calling each `Detector` on the patch. Results are deduplicated by `(detector_id, snippet)`, sorted by severity descending, and capped at `MAX_FINDINGS`.
- `scoring.rs` converts findings into a numeric risk score (High=6, Medium=3, Low=1, capped at 15) and maps it to Low/Medium/High risk level.
- `detectors/` — each detector implements the `Detector` trait (`id()` + `analyze_patch()`). Detectors only look at added lines (lines starting with `+`) in the unified diff patch string. The three detectors are: `lock_across_await` (mutex guard held across `.await`), `blocking_in_async` (blocking calls in async context), `unbounded_spawn` (unbounded task spawning).

**LLM enrichment (`analysis/llm/`):**

Optional. When `LLM_ENABLED=true`, the top `MAX_LLM_EXPLAINS` findings are sent to the OpenAI chat completions API. The response is parsed for `why:` and `fix:` fields. Results are cached in the `llm_explain_cache` Postgres table by `(detector_id, snippet)` hash to avoid redundant API calls.

**GitHub client (`github/`):**

Authenticates as a GitHub App installation using a short-lived JWT (signed with `GITHUB_PRIVATE_KEY_PEM`) to obtain an installation access token per repo. `client.rs` exposes: `get_pr_files`, `upsert_comment` (create or update by comment ID), and `pr_has_label`.

**Store layer (`store/`):**

All database access is through store structs (`JobStore`, `CommentStore`, `FindingsStore`) backed by a `PgPool`. `CommentStore` tracks the GitHub comment ID per `(repo_id, pr_number)` so the app always upserts rather than creating multiple comments.

## Adding a new detector

1. Create `crates/app/src/analysis/detectors/<name>.rs` implementing `Detector`.
2. Register it in `detectors::all()` in `detectors/mod.rs`.
3. Add fixture files under `fixtures/rust/<name>_1.rs` and `fixtures/expected/<name>_1.json` for regression testing.

Detectors receive the full `ChangedFile` (path, patch string, `is_rust` flag). Only analyze added lines (`+` prefix, excluding `+++`). Keep detector logic to regex/string matching — no AST parsing.

## Environment variables

Required: `GITHUB_APP_ID`, `GITHUB_PRIVATE_KEY_PEM`, `GITHUB_WEBHOOK_SECRET`, `DATABASE_URL`.

Notable optional vars (see `.env.example` for all defaults):
- `MODE` — `api | worker | all` (default: `all`)
- `LLM_ENABLED` / `LLM_API_KEY` / `LLM_MODEL` — LLM enrichment (default: disabled)
- `MAX_FILES`, `MAX_PATCH_BYTES`, `MAX_FINDINGS`, `MAX_LLM_EXPLAINS` — analysis limits
