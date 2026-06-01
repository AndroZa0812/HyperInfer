-- Routing config table + team_id for deployments

-- Add team_id to deployments
ALTER TABLE deployments ADD COLUMN team_id UUID REFERENCES teams(id) ON DELETE CASCADE;
CREATE INDEX idx_deployments_team_id ON deployments(team_id);

-- Routing configuration (singleton row)
CREATE TABLE routing_config (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    strategy TEXT NOT NULL DEFAULT 'weighted-shuffle',
    strategy_params JSONB NOT NULL DEFAULT '{}',
    fallback_config JSONB NOT NULL DEFAULT '{}',
    routing_groups JSONB NOT NULL DEFAULT '[]',
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

INSERT INTO routing_config (id) VALUES (1) ON CONFLICT DO NOTHING;

CREATE TRIGGER update_routing_config_updated_at
    BEFORE UPDATE ON routing_config
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
