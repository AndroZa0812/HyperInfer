-- Add CHECK constraints to prevent negative values

ALTER TABLE teams ADD CONSTRAINT teams_budget_positive CHECK (budget_cents >= 0);

ALTER TABLE quotas ADD CONSTRAINT quotas_rpm_positive CHECK (rpm_limit > 0);
ALTER TABLE quotas ADD CONSTRAINT quotas_tpm_positive CHECK (tpm_limit > 0);
