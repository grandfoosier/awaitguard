-- Prevent duplicate active jobs for the same PR commit.
-- Two concurrent webhook deliveries for the same SHA will now produce only one job;
-- the second INSERT silently does nothing instead of creating a duplicate.
CREATE UNIQUE INDEX jobs_no_dup_active
    ON jobs (repo_id, pr_number, head_sha)
    WHERE status IN ('queued', 'running');

-- llm_cost_estimate_cents was never populated (always DEFAULT 0).
ALTER TABLE analysis_runs DROP COLUMN llm_cost_estimate_cents;
