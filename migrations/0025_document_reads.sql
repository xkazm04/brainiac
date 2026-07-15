-- Page-read analytics (KB-PLAN follow-up #8): which pages are actually being
-- consumed, and through which door. One row per served page view — an event
-- log, not a counter, because every question worth asking is windowed ("read
-- in the last 30 days?", "read while dirty?") and a counter cannot answer a
-- window. Volume is page reads, not memory retrievals; rollup is a later
-- problem if it ever becomes one.
CREATE TABLE document_reads (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      uuid NOT NULL REFERENCES orgs(id),
    document_id uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    -- The channel, not the species: http = console/API readers, mcp = coding
    -- agents. We can see the door a reader came through; we do not pretend to
    -- know who they are.
    via         text NOT NULL CHECK (via IN ('http', 'mcp')),
    -- Whether the page was serving a superseded belief at the moment it was
    -- read. This is the signal that RANKS rot: a dirty page nobody reads is a
    -- chore, a dirty page being read is misleading someone right now.
    was_dirty   boolean NOT NULL DEFAULT false,
    read_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_document_reads_doc ON document_reads(org_id, document_id, read_at DESC);
CREATE INDEX idx_document_reads_window ON document_reads(org_id, read_at DESC);

-- Append-only from the app role: a reader records their own read, and nothing
-- in the product edits or deletes one — analytics that can be rewritten are
-- not analytics. SELECT is additionally gated by a visible parent document
-- (like document_sections): an org-wide aggregate must not become an oracle
-- for the existence of team-private pages.
ALTER TABLE document_reads ENABLE ROW LEVEL SECURITY;
CREATE POLICY document_reads_insert ON document_reads FOR INSERT
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
CREATE POLICY document_reads_read ON document_reads FOR SELECT
    USING (org_id = current_setting('app.org_id')::uuid
           AND document_id IN (SELECT id FROM documents));

-- The app role is created in 0001 with grants on the tables that existed
-- then; every new table grants explicitly or the API gets "permission denied"
-- at runtime. No UPDATE, no DELETE: append-only is enforced at the grant, not
-- just by convention.
GRANT SELECT, INSERT ON document_reads TO brainiac_app;
