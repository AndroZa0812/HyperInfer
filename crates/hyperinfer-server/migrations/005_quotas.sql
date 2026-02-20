CREATE TABLE quotas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE UNIQUE,
    rpm_limit INTEGER DEFAULT 60,
    tpm_limit INTEGER DEFAULT 100000,
    budget_cents BIGINT DEFAULT 0,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
