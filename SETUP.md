# Setup & deployment

## Prerequisites

- Rust (stable) and Cargo
- Docker (for local Postgres and/or containerised deployment)
- A GitHub account with permission to create GitHub Apps

---

## 1. Create a GitHub App

1. Go to **Settings → Developer settings → GitHub Apps → New GitHub App**.
2. Set the webhook URL to `https://<your-host>/webhook/github`.
3. Generate a **webhook secret** — you'll need this for `GITHUB_WEBHOOK_SECRET`.
4. Under **Permissions**, grant:
   - *Pull requests*: Read & Write (to post comments)
   - *Contents*: Read (to fetch PR files)
   - *Issues*: Read (to read labels)
5. Under **Subscribe to events**, check **Pull request**.
6. Generate and download a **private key** (PEM file).
7. Note the **App ID** from the app's settings page.
8. Install the app on your account or target organisation.

---

## 2. Configure

```bash
cp .env.example .env
```

Fill in at minimum:

```env
GITHUB_APP_ID=123456
GITHUB_PRIVATE_KEY_PEM=-----BEGIN RSA PRIVATE KEY-----\nMIIE...\n-----END RSA PRIVATE KEY-----
GITHUB_WEBHOOK_SECRET=your_webhook_secret
DATABASE_URL=postgres://postgres:postgres@localhost:5432/awaitguard
```

`GITHUB_PRIVATE_KEY_PEM` can be a single line with literal `\n` separators — the app normalises them on startup. Alternatively, use a multi-line quoted string.

---

## 3. Run locally

```bash
# Start Postgres
docker-compose up db

# Start the app — migrations run automatically on startup
cargo run --bin awaitguard
```

The server listens on `http://0.0.0.0:8080` by default. Use [ngrok](https://ngrok.com) or similar to expose it to GitHub during local development.

---

## 4. Run with Docker Compose

```bash
docker-compose up --build
```

Both the app and Postgres start together. The app waits for the database health check before starting.

---

## Configuration reference

| Variable | Default | Description |
|---|---|---|
| `GITHUB_APP_ID` | required | GitHub App numeric ID |
| `GITHUB_PRIVATE_KEY_PEM` | required | RSA private key (PEM, `\n` OK) |
| `GITHUB_WEBHOOK_SECRET` | required | Webhook HMAC secret |
| `DATABASE_URL` | required | Postgres connection string |
| `MAX_DB_CONNECTIONS` | `10` | Postgres connection pool size |
| `HTTP_BIND` | `0.0.0.0:8080` | Address and port to listen on |
| `MODE` | `all` | `api`, `worker`, or `all` |
| `WORKER_CONCURRENCY` | `4` | Max parallel jobs |
| `JOB_MAX_ATTEMPTS` | `5` | Retry limit before marking a job dead |
| `JOB_LOCK_TIMEOUT_SECS` | `300` | Seconds before the sweeper reclaims a stuck job |
| `MAX_FILES` | `40` | Rust files analysed per PR |
| `MAX_PATCH_BYTES` | `200000` | Max patch size per file (bytes) |
| `MAX_FINDINGS` | `20` | Max findings reported per PR |
| `LLM_ENABLED` | `false` | Enable LLM explanations |
| `LLM_PROVIDER` | `openai` | `openai` or `anthropic` |
| `LLM_API_KEY` | — | API key for the selected provider |
| `LLM_MODEL` | `gpt-4.1-mini` | Model name (e.g. `claude-haiku-4-5-20251001` for Anthropic) |
| `LLM_TIMEOUT_SECS` | `20` | Per-request LLM timeout |
| `MAX_LLM_EXPLAINS` | `3` | Top N findings sent to the LLM |
| `RUST_LOG` | `info` | Log level (`debug`, `info`, `warn`, `error`) |

---

## Scaling

`MODE=api` and `MODE=worker` let you run the HTTP server and job processor as separate services backed by the same Postgres instance. Multiple worker replicas can run concurrently — `SELECT … FOR UPDATE SKIP LOCKED` ensures each job is claimed by exactly one worker.

---

## Health endpoints

| Endpoint | Description |
|---|---|
| `GET /healthz` | Always 200 — liveness check |
| `GET /readyz` | 200 if Postgres is reachable — readiness check |

---

## Adding a detector

1. Create `crates/app/src/analysis/detectors/<name>.rs` implementing the `Detector` trait:

```rust
use std::sync::OnceLock;
use regex::Regex;
use crate::{analysis::{patch::source_line_map, Finding, Severity}, github::models::ChangedFile};
use super::Detector;

static MY_RE: OnceLock<Regex> = OnceLock::new();

pub struct MyDetector;

impl Detector for MyDetector {
    fn id(&self) -> &'static str { "my_detector" }

    fn analyze_patch(&self, file: &ChangedFile) -> Vec<Finding> {
        let patch = match &file.patch { Some(p) => p, None => return vec![] };
        let re = MY_RE.get_or_init(|| Regex::new(r"your_pattern").unwrap());
        let line_map = source_line_map(patch);
        let mut findings = Vec::new();

        for (idx, line) in patch.lines().enumerate() {
            if !line.starts_with('+') || line.starts_with("+++") { continue; }
            let content = line.trim_start_matches('+').trim();
            if re.is_match(content) {
                findings.push(Finding {
                    detector_id: self.id().to_owned(),
                    title: "My finding".to_owned(),
                    severity: Severity::High,
                    path: file.path.clone(),
                    line_hint: line_map.get(idx).copied().flatten(),
                    snippet: content.to_owned(),
                    details: None,
                    suggestion: Some("How to fix it.".to_owned()),
                });
            }
        }
        findings
    }
}
```

2. Register it in `detectors::all()` in `detectors/mod.rs`.
3. Add unit tests directly in the file — see the existing detectors for the pattern.

Detectors work on raw strings, not an AST. Only examine added lines (lines starting with `+`, excluding `+++`). Use `source_line_map` to get real source file line numbers from the diff hunk headers.
