-- Knowledge Health history — one row per recorded snapshot, so the leadership
-- report can show a TREND (the score over weeks) rather than a lone number. The
-- report's power is the line, not the point: "your consistency has slipped three
-- weeks running" is the sentence that makes a VP Eng act. Aggregate scores only —
-- no memory content — so it is plain org-scoped metadata under the same RLS as
-- promotions/contradictions.

CREATE TABLE knowledge_health_snapshots (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    captured_at timestamptz NOT NULL DEFAULT now(),
    score int NOT NULL,
    consistency int NOT NULL,
    currency int NOT NULL,
    liquidity int NOT NULL,
    governance int NOT NULL,
    cross_team_contradictions int NOT NULL,
    stale_beliefs int NOT NULL,
    total_memories int NOT NULL
);

CREATE INDEX idx_khs_org_time ON knowledge_health_snapshots (org_id, captured_at);

ALTER TABLE knowledge_health_snapshots ENABLE ROW LEVEL SECURITY;
CREATE POLICY khs_org ON knowledge_health_snapshots
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

-- The app role (set by scoped_tx) needs table access; 0001's blanket grant only
-- covered tables that existed then, so every new table grants itself.
GRANT SELECT, INSERT, UPDATE, DELETE ON knowledge_health_snapshots TO brainiac_app;
