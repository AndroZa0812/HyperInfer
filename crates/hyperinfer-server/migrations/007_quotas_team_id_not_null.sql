-- Add NOT NULL constraint to team_id in quotas table

ALTER TABLE quotas ALTER COLUMN team_id SET NOT NULL;
