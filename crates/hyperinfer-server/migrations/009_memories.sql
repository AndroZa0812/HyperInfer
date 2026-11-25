-- Shared cross-repo and team memory service (HYP-44)
CREATE TABLE IF NOT EXISTS memories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    repo_id TEXT,
    scope TEXT NOT NULL CHECK (scope IN ('user', 'repo', 'team')),
    content TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'pending_approval', 'expired')),
    valid_from TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_to TIMESTAMPTZ,
    superseded_by UUID REFERENCES memories(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    tsv tsvector
);

CREATE OR REPLACE FUNCTION memories_tsv_trigger() RETURNS trigger AS $$
BEGIN
    NEW.tsv := to_tsvector(
        'english',
        coalesce(NEW.content, '') || ' ' || coalesce(array_to_string(NEW.tags, ' '), '')
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_memories_tsv ON memories;
CREATE TRIGGER trg_memories_tsv
    BEFORE INSERT OR UPDATE OF content, tags ON memories
    FOR EACH ROW EXECUTE FUNCTION memories_tsv_trigger();

CREATE INDEX IF NOT EXISTS idx_memories_tsv ON memories USING GIN (tsv);
CREATE INDEX IF NOT EXISTS idx_memories_tags ON memories USING GIN (tags);
CREATE INDEX IF NOT EXISTS idx_memories_team_scope_valid ON memories (team_id, scope, valid_to);
CREATE INDEX IF NOT EXISTS idx_memories_team_repo ON memories (team_id, repo_id) WHERE repo_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_memories_team_user ON memories (team_id, user_id);

-- Optional pgvector support: add embedding column + HNSW index if pgvector is installed,
-- gracefully falling back to native Postgres full-text search when absent.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'memories' AND column_name = 'embedding'
        ) THEN
            EXECUTE 'ALTER TABLE memories ADD COLUMN embedding vector(384)';
        END IF;
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_memories_embedding ON memories USING hnsw (embedding vector_cosine_ops)';
    END IF;
END $$;
