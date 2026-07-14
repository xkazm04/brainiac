-- KB3 (docs/KB-PLAN.md D4–D7; ARCHITECTURE §8.5): publishing the knowledge base
-- OUTWARD — to Git, and to the wiki a team already reads.
--
-- Three ideas are encoded here, and each one is a refusal:
--
-- 1. `kb_enabled` — the KB layer is OPTIONAL (D6). A single-team org that wants
--    only the memory layer never pays for composition or publishing. Off by
--    default: a feature that turns itself on in someone's Confluence is not a
--    feature, it is an incident.
--
-- 2. `publish_targets` — publishing is a PUBLISHER TRAIT, not Atlassian code
--    (D4). Git and Confluence are two rows here, and Notion is a third one later.
--    Credentials are NOT stored: `secret_ref` names an env var / vault key the
--    operator supplies, so a database dump can never contain a PAT that can
--    write to a customer's wiki.
--
-- 3. `document_publications` — what we actually pushed, where, and when. This is
--    the ledger that lets a publish be idempotent (don't re-push an unchanged
--    revision), lets a paused breaker say "you are reading the version from
--    Tuesday", and lets an operator prove what left the building.

ALTER TABLE orgs
    ADD COLUMN kb_enabled boolean NOT NULL DEFAULT false;

CREATE TABLE publish_targets (
    id          uuid PRIMARY KEY,
    org_id      uuid NOT NULL,
    kind        text NOT NULL,          -- git | confluence
    -- Non-secret config: {repo_path, docs_dir} for git;
    -- {base_url, space_key, user_email} for confluence.
    config      jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- The NAME of the env var / vault key holding the token. Never the token.
    secret_ref  text,
    enabled     boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT publish_targets_kind_check CHECK (kind IN ('git', 'confluence'))
);

CREATE INDEX idx_publish_targets_org ON publish_targets(org_id, enabled);

CREATE TABLE document_publications (
    document_id  uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    target_id    uuid NOT NULL REFERENCES publish_targets(id) ON DELETE CASCADE,
    org_id       uuid NOT NULL,
    -- The revision that is live in the external system RIGHT NOW. Comparing it
    -- to documents.current_revision is how we know whether a push is needed and
    -- how stale a paused page has become.
    revision_id  uuid NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
    -- The external system's own handle (Confluence page id; git path).
    external_ref text,
    published_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (document_id, target_id)
);

CREATE INDEX idx_publications_org ON document_publications(org_id);

-- RLS: both tables are org-scoped. Publish targets carry no secrets, but they do
-- carry where an org's knowledge goes — which is not another tenant's business.
ALTER TABLE publish_targets ENABLE ROW LEVEL SECURITY;
CREATE POLICY publish_targets_org ON publish_targets
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE document_publications ENABLE ROW LEVEL SECURITY;
CREATE POLICY document_publications_org ON document_publications
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON publish_targets TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON document_publications TO brainiac_app;
