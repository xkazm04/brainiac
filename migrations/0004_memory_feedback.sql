-- Agent/operator feedback on retrieved memories — the retrieval loop's
-- return channel (MCP memory_feedback tool). Verdicts feed future ranking
-- and re-verification; rows are org-scoped like every governance table.

CREATE TABLE memory_feedback (
    id          uuid PRIMARY KEY,
    org_id      uuid NOT NULL,
    memory_id   uuid NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    user_id     uuid NOT NULL,
    verdict     text NOT NULL CHECK (verdict IN ('helpful', 'wrong', 'outdated')),
    note        text,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_memory_feedback_memory ON memory_feedback(memory_id);

ALTER TABLE memory_feedback ENABLE ROW LEVEL SECURITY;
CREATE POLICY memory_feedback_org ON memory_feedback
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON memory_feedback TO brainiac_app;
