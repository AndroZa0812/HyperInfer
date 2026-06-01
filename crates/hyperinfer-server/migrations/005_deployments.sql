-- Routing deployments table

CREATE TABLE deployments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    provider VARCHAR(50) NOT NULL,
    model VARCHAR(255) NOT NULL,
    api_key_ref VARCHAR(512) NOT NULL DEFAULT '',
    base_url VARCHAR(1024) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    weight INTEGER NOT NULL DEFAULT 1 CHECK (weight >= 0),
    priority INTEGER NOT NULL DEFAULT 0 CHECK (priority >= 0),
    max_tpm INTEGER CHECK (max_tpm > 0),
    max_rpm INTEGER CHECK (max_rpm > 0),
    cost_per_1k_input_tokens NUMERIC(10,6) CHECK (cost_per_1k_input_tokens >= 0),
    cost_per_1k_output_tokens NUMERIC(10,6) CHECK (cost_per_1k_output_tokens >= 0),
    metadata JSONB NOT NULL DEFAULT '{}',
    sort_order INTEGER NOT NULL DEFAULT 0 CHECK (sort_order >= 0),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_deployments_provider_model ON deployments(provider, model);
CREATE INDEX idx_deployments_is_active ON deployments(is_active);
CREATE INDEX idx_deployments_sort_order ON deployments(sort_order);

CREATE TRIGGER update_deployments_updated_at
    BEFORE UPDATE ON deployments
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
