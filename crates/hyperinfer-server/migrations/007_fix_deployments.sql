ALTER TABLE deployments ALTER COLUMN cost_per_1k_input_tokens TYPE DOUBLE PRECISION;
ALTER TABLE deployments ALTER COLUMN cost_per_1k_output_tokens TYPE DOUBLE PRECISION;
ALTER TABLE deployments ADD CONSTRAINT deployments_name_unique UNIQUE (name);
