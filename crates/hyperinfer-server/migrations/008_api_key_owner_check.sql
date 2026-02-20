-- Add CHECK constraint to ensure API keys have an owner

ALTER TABLE api_keys ADD CONSTRAINT api_key_has_owner CHECK (user_id IS NOT NULL OR team_id IS NOT NULL);
