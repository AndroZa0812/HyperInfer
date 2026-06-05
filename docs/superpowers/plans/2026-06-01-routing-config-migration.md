# Routing Config Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `routing_config` singleton table and `team_id` column to `deployments` table for multi-tenant routing support.

**Architecture:** Single SQL migration file that extends existing schema with a new table and column, following established migration patterns.

**Tech Stack:** PostgreSQL, SQL migration

**Working directory:** All commands run from the worktree root at `.worktrees/routing-system/`

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `crates/hyperinfer-server/migrations/006_routing_config.sql` | Add team_id to deployments, create routing_config singleton table |

---

## Task 1: Create migration file

**Files:**
- Create: `crates/hyperinfer-server/migrations/006_routing_config.sql`

- [ ] **Step 1: Create migration file**

```sql
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
```

- [ ] **Step 2: Verify migration compiles**

Run: `cd crates/hyperinfer-server && cargo check 2>&1 | tail -5`
Expected: `Finished` (no errors)

- [ ] **Step 3: Commit**

```bash
git add crates/hyperinfer-server/migrations/006_routing_config.sql
git commit -m "feat: add routing_config table and team_id column migration"
```

---

## Self-Review

1. **Spec coverage:** Migration adds team_id to deployments and creates routing_config table as specified.
2. **Placeholder scan:** No placeholders; exact SQL provided.
3. **Type consistency:** Uses UUID for team_id, references teams(id) with ON DELETE CASCADE. Uses TIMESTAMP WITH TIME ZONE for updated_at (consistent with other tables). Uses update_updated_at_column() function already defined in 001_initial_schema.sql.
4. **Dependencies:** Migration assumes deployments table exists (created in 005_deployments.sql). Function update_updated_at_column() exists (created in 001_initial_schema.sql). No other dependencies.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-01-routing-config-migration.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?