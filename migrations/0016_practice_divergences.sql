-- Practice divergences — the standardization surface. Where `contradictions`
-- catch two facts that cannot both be true, this catches something subtler and
-- more valuable to a platform team: several teams solving the SAME problem in
-- DIFFERENT ways, each locally reasonable, invisible in the aggregate. An LLM
-- adjudicates cross-team clusters (anchored on a shared canonical entity) into a
-- named practice, each team's approach, and a recommended single standard.
--
-- Stored (not computed on read) because adjudication is an LLM sweep: expensive,
-- provider-specific, and run on a schedule — the same shape as extraction.

CREATE TABLE practice_divergences (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    canonical_id uuid,                 -- the entity the divergence anchors on
    practice text NOT NULL,            -- the named practice ("service retry policy")
    summary text,                      -- one line: what actually diverges
    recommended_standard text,         -- the adjudicator's recommended single standard
    impact text NOT NULL,              -- high | medium | low
    positions jsonb NOT NULL,          -- [{team, approach}] — each team's take
    model_ref text,                    -- provenance: which model adjudicated
    detected_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_pd_org ON practice_divergences (org_id, detected_at);

ALTER TABLE practice_divergences ENABLE ROW LEVEL SECURITY;
CREATE POLICY pd_org ON practice_divergences
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

-- New table → grants itself (0001's blanket grant only covered then-existing tables).
GRANT SELECT, INSERT, UPDATE, DELETE ON practice_divergences TO brainiac_app;
