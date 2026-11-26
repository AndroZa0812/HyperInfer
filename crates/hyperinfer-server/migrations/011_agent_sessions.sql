-- HyperCode Agent Sessions & Trajectory Observability (HYP-76)

CREATE TABLE IF NOT EXISTS agent_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_key VARCHAR(255) NOT NULL,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    api_key_id UUID REFERENCES api_keys(id) ON DELETE SET NULL,
    agent_name VARCHAR(128) NOT NULL DEFAULT 'hypercode',
    model VARCHAR(255) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'completed',
    title VARCHAR(512),
    input_tokens BIGINT NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    cached_input_tokens BIGINT NOT NULL DEFAULT 0 CHECK (cached_input_tokens >= 0),
    output_tokens BIGINT NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    reasoning_tokens BIGINT NOT NULL DEFAULT 0 CHECK (reasoning_tokens >= 0),
    cost_cents DOUBLE PRECISION NOT NULL DEFAULT 0.0 CHECK (cost_cents >= 0.0),
    cache_savings_cents DOUBLE PRECISION NOT NULL DEFAULT 0.0 CHECK (cache_savings_cents >= 0.0),
    cache_hits INTEGER NOT NULL DEFAULT 0 CHECK (cache_hits >= 0),
    total_latency_ms BIGINT NOT NULL DEFAULT 0 CHECK (total_latency_ms >= 0),
    mcp_tool_calls JSONB NOT NULL DEFAULT '[]'::jsonb,
    trajectory JSONB NOT NULL DEFAULT '[]'::jsonb,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    UNIQUE (team_id, session_key)
);

CREATE INDEX IF NOT EXISTS idx_agent_sessions_team_started ON agent_sessions(team_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_sessions_api_key ON agent_sessions(api_key_id);
CREATE INDEX IF NOT EXISTS idx_agent_sessions_status ON agent_sessions(status);
