-- Remove redundant budget_cents from quotas (already in teams table)

ALTER TABLE quotas DROP COLUMN budget_cents;
