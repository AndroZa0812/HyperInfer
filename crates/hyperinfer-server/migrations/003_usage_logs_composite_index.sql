-- Add composite index for common dashboard query patterns
-- Dashboard queries typically filter by team_id with a time range on recorded_at

CREATE INDEX idx_usage_logs_team_recorded ON usage_logs(team_id, recorded_at);
