-- Request-level observability metadata for usage_logs (HYP-71)

ALTER TABLE usage_logs
    ADD COLUMN IF NOT EXISTS status_code SMALLINT NOT NULL DEFAULT 200,
    ADD COLUMN IF NOT EXISTS cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cached_input_tokens >= 0),
    ADD COLUMN IF NOT EXISTS reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK (reasoning_tokens >= 0),
    ADD COLUMN IF NOT EXISTS cost_cents DOUBLE PRECISION NOT NULL DEFAULT 0.0 CHECK (cost_cents >= 0.0),
    ADD COLUMN IF NOT EXISTS cache_hit BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS failover_used BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS deployment_name VARCHAR(255),
    ADD COLUMN IF NOT EXISTS stop_reason VARCHAR(64);

CREATE INDEX IF NOT EXISTS idx_usage_logs_recorded_desc ON usage_logs(recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_logs_model ON usage_logs(model);
