# awaitguard

A GitHub App that automatically reviews Rust pull requests for async concurrency bugs. When a PR is opened or updated, awaitguard fetches the diff, runs a set of pattern-based detectors over the added lines, optionally enriches findings with an LLM explanation, and posts a single structured comment — updated in place on every push.

Async concurrency bugs in Rust are easy to introduce and hard to catch in review. A `MutexGuard` kept alive across an `.await` point, a `std::thread::sleep` inside an async function, or an unbounded `tokio::spawn` inside a loop are all legal Rust that compile without warning but can stall an executor or exhaust system resources in production. awaitguard catches these at the diff level before they merge.

---

## Example output

> The comment is created on the first push and updated in place on every subsequent push to the same PR.

---

## 🔴 Concurrency Risk: **High** (9/15)

Analyzed `acme/api-server` PR #104 at `e3a17f2`.
Files scanned: 4 (Rust: 3). Findings: 2.

### Findings

1\. **Lock held across await** — `src/handler.rs` (near line 84) \[🔴 High\]
```rust
let mut guard = self.cache.lock().await
...
let resp = client.get(&url).send().await?
```
- **Why it matters:** The MutexGuard remains live across the network call. Any other task trying to acquire the same lock will be unable to make progress until this await resolves, which can cause latency spikes or deadlocks under load.
- **Suggested fix:** Drop the guard before awaiting. Extract needed data first, or scope the guard with braces.

2\. **std::thread::sleep in async context** — `src/retry.rs` (near line 23) \[🔴 High\]
```rust
std::thread::sleep(Duration::from_millis(backoff_ms));
```
- **Suggested fix:** Use `tokio::time::sleep` instead.

<sub>To suppress: add label `concurrency-ok`.</sub>

---

## Detectors

Each detector examines only **newly added lines** in the diff — pre-existing bugs outside the changeset are not reported.

| Detector | Severity | What it catches |
|---|---|---|
| **`lock_across_await`** | High | `MutexGuard` / `RwLockGuard` kept live across an `.await` point; tracks brace depth to handle scope boundaries and explicit `drop()` calls |
| **`blocking_in_async`** | High / Medium | `std::thread::sleep`, `std::fs::*`, `reqwest::blocking`, `std::sync::Mutex`, `parking_lot::Mutex`, `block_in_place` in async context |
| **`unbounded_spawn`** | High / Medium | `tokio::spawn` inside a loop, `join_all` on an unbounded collection, `FuturesUnordered` without a concurrency limit |

## Architecture

```
GitHub webhook
      │  HMAC-SHA256 verified
      ▼
  HTTP server  ──────────────────────────────────────────┐
  (axum 0.7)                                             │
      │  upsert PR row + enqueue job                     │
      │  (only on head SHA change; partial unique index  │
      │   prevents duplicate jobs on concurrent delivery)│
      ▼                                                  │
  jobs table (Postgres)                                  │
      │  SELECT … FOR UPDATE SKIP LOCKED                 │
      ▼                                                  │
  Worker pool  (tokio + Semaphore, n concurrent)         │
      │                                                  │
      ├─ Check `concurrency-ok` suppression label        │
      ├─ Post "analysis in progress…" placeholder        │
      ├─ Fetch PR files from GitHub API                  │
      │                                                  │
      ├─ Analysis engine                                 │
      │   ├─ lock_across_await detector                  │
      │   ├─ blocking_in_async detector                  │
      │   └─ unbounded_spawn detector                    │
      │   → deduplicate · sort by severity · score       │
      │                                                  │
      ├─ LLM enrichment (optional)                       │
      │   └─ top N findings → OpenAI / Anthropic API     │
      │      results cached in Postgres by snippet hash  │
      │                                                  │
      └─ Upsert single comment on PR · persist findings  │
                                                         │
  Sweeper (every 30s) ───────────────────────────────────┘
      Reset jobs stuck in `running` beyond timeout
      (crashed workers, network hangs)
```

**Job queue design.** Jobs are stored in Postgres and claimed with `SELECT … FOR UPDATE SKIP LOCKED`, which lets multiple worker replicas dequeue concurrently without coordination. Failed jobs retry with exponential backoff (capped at 10 minutes); a background sweeper reclaims jobs that got stuck in `running` state. The HTTP server and worker can run as a single process or as separate deployments backed by the same database.

**Detector design.** Each detector implements a two-method trait (`id()` + `analyze_patch()`) and receives the full unified diff patch for one file. Detectors work on raw strings — no AST parsing — which keeps them fast and easy to extend. The `lock_across_await` detector is the most involved: it tracks a stack of active guards with brace-depth scoping, handles explicit `drop()` calls, and only fires when at least one of the lock acquisition or the await point is a newly added line (so pre-existing bugs in unchanged context aren't reported). Line hints are resolved against the `+A,B` hunk offsets in the diff header so they point to actual source file line numbers, not diff-relative positions.

**LLM enrichment.** When enabled, the top findings are sent to an OpenAI or Anthropic model with a structured prompt asking for a one-sentence `why` and a concrete `fix`. Both fields are cached in Postgres by a SHA-256 hash of `(detector_id, snippet)` to avoid redundant API calls across PRs that share the same pattern.

## Tech stack

- **Rust** — tokio async runtime, axum web framework, sqlx (compile-time-checked queries), reqwest HTTP client
- **PostgreSQL** — job queue with `SKIP LOCKED`, partial unique index for dedup, JSONB findings storage, LLM explanation cache
- **GitHub Apps API** — RS256 JWT authentication, installation access tokens, webhook HMAC-SHA256 signature verification
- **Docker / Docker Compose** — multi-stage release build, health-checked Postgres dependency

## Development

```bash
cargo test          # all tests (detectors + scoring + patch line mapping)
cargo clippy        # lint
cargo build --release --bin awaitguard
```

See [SETUP.md](SETUP.md) for configuration, deployment, and how to add a new detector.
