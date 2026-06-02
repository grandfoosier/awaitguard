CREATE TABLE IF NOT EXISTS installations (
    installation_id bigint PRIMARY KEY,
    account_login   text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS prs (
    repo_id         bigint  NOT NULL,
    pr_number       int     NOT NULL,
    installation_id bigint  NOT NULL,
    head_sha        text    NOT NULL,
    comment_id      bigint,
    last_status     text    NOT NULL DEFAULT 'new',
    updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (repo_id, pr_number)
);

CREATE TABLE IF NOT EXISTS jobs (
    id              bigserial PRIMARY KEY,
    kind            text        NOT NULL DEFAULT 'analyze_pr',
    repo_id         bigint      NOT NULL,
    repo_full_name  text        NOT NULL,
    pr_number       int         NOT NULL,
    installation_id bigint      NOT NULL,
    head_sha        text        NOT NULL,
    attempt         int         NOT NULL DEFAULT 0,
    status          text        NOT NULL DEFAULT 'queued',
    run_after       timestamptz NOT NULL DEFAULT now(),
    last_error      text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS jobs_ready_idx ON jobs (status, run_after);

CREATE TABLE IF NOT EXISTS analysis_runs (
    id              bigserial PRIMARY KEY,
    repo_id         bigint  NOT NULL,
    pr_number       int     NOT NULL,
    head_sha        text    NOT NULL,
    risk_score      numeric NOT NULL,
    risk_level      text    NOT NULL,
    findings_json   jsonb   NOT NULL,
    findings_hash   text    NOT NULL,
    llm_cost_estimate_cents int NOT NULL DEFAULT 0,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS analysis_runs_lookup
    ON analysis_runs (repo_id, pr_number, head_sha);

CREATE TABLE IF NOT EXISTS llm_explain_cache (
    key             text PRIMARY KEY,
    explanation_md  text NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);
