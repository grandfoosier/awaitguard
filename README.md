# awaitguard

A GitHub App that automatically reviews Rust pull requests for async concurrency bugs. When a PR is opened or updated, awaitguard fetches the diff, runs pattern-based detectors over added lines, and posts a single comment with a risk score and per-finding details.

## What it detects

| Detector | Severity | Example |
|---|---|---|
| **Lock across await** | High | `MutexGuard` held across `.await` starves the executor |
| **Blocking in async** | High / Medium | `std::thread::sleep`, `std::fs::read`, `reqwest::blocking` inside async fn |
| **Unbounded spawn** | High / Medium | `tokio::spawn` inside a loop, unguarded `join_all` / `FuturesUnordered` |

Detectors only flag **newly added lines** — they won't report pre-existing bugs that aren't part of your diff.

## How it works

```
GitHub webhook → job queue (Postgres) → worker → analysis engine → PR comment
```

1. A `pull_request` event (opened / synchronize / reopened) arrives at `/webhook/github`.
2. The webhook handler verifies the HMAC-SHA256 signature and enqueues a job — only if the head SHA changed, so label/title edits don't trigger re-analysis.
3. A background worker dequeues jobs (`SELECT … FOR UPDATE SKIP LOCKED`), runs the three detectors over every changed Rust file, and upserts a single comment on the PR.
4. Optionally, the top findings are sent to an LLM for a plain-English `why` and `fix` explanation (results are cached in Postgres).

Add the label **`concurrency-ok`** to any PR to suppress the comment entirely.

## Setup

### 1. Create a GitHub App

1. Go to **Settings → Developer settings → GitHub Apps → New GitHub App**.
2. Set the webhook URL to `https://<your-host>/webhook/github`.
3. Generate a **webhook secret** and note it.
4. Under **Permissions**, grant:
   - *Pull requests*: Read & Write (to post comments)
   - *Contents*: Read (to fetch PR files)
   - *Issues*: Read (to read labels)
5. Generate and download a **private key** (PEM file).
6. Note the **App ID** shown on the app's settings page.
7. Install the app on your account or target organization.

### 2. Configure

```bash
cp .env.example .env
```

Edit `.env`:

```env
GITHUB_APP_ID=123456
GITHUB_PRIVATE_KEY_PEM=-----BEGIN RSA PRIVATE KEY-----\nMIIE...\n-----END RSA PRIVATE KEY-----
GITHUB_WEBHOOK_SECRET=your_webhook_secret
DATABASE_URL=postgres://postgres:postgres@localhost:5432/awaitguard
```

For `GITHUB_PRIVATE_KEY_PEM`, you can paste the PEM as a single line with literal `\n` between lines — the app normalizes them automatically. Or use actual newlines inside a quoted string.

### 3. Run locally

```bash
# Start Postgres
docker-compose up db

# Run the app (migrations run automatically on startup)
cargo run --bin awaitguard
```

The server listens on `http://0.0.0.0:8080` by default. Use [ngrok](https://ngrok.com) or similar to expose it to GitHub during local development.

### 4. Run with Docker Compose

```bash
# Fill in credentials first
cp .env.example .env && vim .env

docker-compose up --build
```

Both the app and Postgres start together. The app waits for the database to be healthy before starting.

## Configuration reference

| Variable | Default | Description |
|---|---|---|
| `GITHUB_APP_ID` | required | GitHub App numeric ID |
| `GITHUB_PRIVATE_KEY_PEM` | required | RSA private key (PEM format, `\n` OK) |
| `GITHUB_WEBHOOK_SECRET` | required | Webhook HMAC secret |
| `DATABASE_URL` | required | Postgres connection string |
| `MAX_DB_CONNECTIONS` | `10` | Postgres connection pool size |
| `HTTP_BIND` | `0.0.0.0:8080` | Address and port to listen on |
| `MODE` | `all` | `api`, `worker`, or `all` |
| `WORKER_CONCURRENCY` | `4` | Max parallel jobs |
| `JOB_MAX_ATTEMPTS` | `5` | Retry limit before marking a job dead |
| `JOB_LOCK_TIMEOUT_SECS` | `300` | Seconds before sweeper reclaims a stuck job |
| `MAX_FILES` | `40` | Rust files analyzed per PR |
| `MAX_PATCH_BYTES` | `200000` | Max patch size per file |
| `MAX_FINDINGS` | `20` | Max findings reported per PR |
| `LLM_ENABLED` | `false` | Enable LLM explanations |
| `LLM_PROVIDER` | `openai` | `openai` or `anthropic` |
| `LLM_API_KEY` | — | API key for the selected provider |
| `LLM_MODEL` | `gpt-4.1-mini` | Model name (e.g. `claude-haiku-4-5-20251001` for Anthropic) |
| `LLM_TIMEOUT_SECS` | `20` | Per-request LLM timeout |
| `MAX_LLM_EXPLAINS` | `3` | Top N findings sent to LLM |
| `RUST_LOG` | `info` | Log level (`debug`, `info`, `warn`, `error`) |

## Scaling

`MODE=api` and `MODE=worker` let you run the HTTP server and job processor as separate services (e.g., separate containers) backed by the same Postgres instance. The worker uses `SELECT … FOR UPDATE SKIP LOCKED` so multiple worker replicas are safe to run concurrently.

## Adding a detector

1. Create `crates/app/src/analysis/detectors/<name>.rs` implementing the `Detector` trait:

```rust
pub struct MyDetector;

impl Detector for MyDetector {
    fn id(&self) -> &'static str { "my_detector" }

    fn analyze_patch(&self, file: &ChangedFile) -> Vec<Finding> {
        // Only look at added lines (start with '+', not '+++')
        todo!()
    }
}
```

2. Register it in `detectors::all()` in `detectors/mod.rs`.
3. Add unit tests directly in the file (see existing detectors for examples).

Keep detector logic to regex/string matching — no AST parsing. Detectors receive the full unified diff patch string; ignore lines that don't start with `+` or that start with `+++`.

## Development

```bash
cargo build --release --bin awaitguard   # release build
cargo test                                # all tests
cargo test detects_lock_across_await      # single test by name
cargo clippy                              # lint
```

Tests for each detector live in the detector file itself. The scoring logic tests are in `analysis/scoring.rs`.

## Health endpoints

| Endpoint | Description |
|---|---|
| `GET /healthz` | Always 200 — liveness check |
| `GET /readyz` | 200 if Postgres is reachable — readiness check |
