-- KB1 (docs/KB-PLAN.md; ARCHITECTURE.md §8): the document layer.
--
-- The governing principle, and the reason this schema looks the way it does:
-- PAGES ARE PROJECTIONS, NOT A SECOND SOURCE OF TRUTH. Nothing here stores
-- knowledge. A document is a compiled VIEW over canonical memories, and every
-- claim in a published revision must be traceable to the memory it came from.
-- That is what makes the wiki immune to rot: when a memory is superseded or a
-- contradiction is resolved, the pages that cited the losing memory are marked
-- dirty and recompose. Nobody has to remember to update a page.
--
-- (0016 belongs to practice_divergences — a parallel workstream.)

-- ── documents ───────────────────────────────────────────────────────────
CREATE TABLE documents (
    id               uuid PRIMARY KEY,
    org_id           uuid NOT NULL,
    team_id          uuid,
    slug             text NOT NULL,
    title            text NOT NULL,
    -- Reuses the memory visibility vocabulary: a page can never be more visible
    -- than the memories allowed to compose into it (enforced in the compose
    -- worker by running retrieval as a visibility-capped synthetic principal).
    visibility       visibility NOT NULL DEFAULT 'team',
    doc_kind         text NOT NULL DEFAULT 'topic_page',
    status           text NOT NULL DEFAULT 'draft',
    current_revision uuid,
    -- Set when an underlying memory changes; the compose worker claims dirty
    -- pages. This column IS the anti-rot mechanism.
    dirty_at         timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT documents_slug_unique UNIQUE (org_id, slug),
    CONSTRAINT documents_kind_check
        CHECK (doc_kind IN ('entity_page', 'topic_page', 'runbook', 'onboarding')),
    CONSTRAINT documents_status_check
        CHECK (status IN ('draft', 'published', 'archived'))
);

CREATE INDEX idx_documents_dirty ON documents(org_id, dirty_at) WHERE dirty_at IS NOT NULL;

-- ── sections ────────────────────────────────────────────────────────────
-- A section is either COMPOSED (bound to a memory query, machine-regenerated,
-- never hand-owned) or PINNED (human prose, never touched by regeneration).
-- The split is what lets a human keep authorship of the parts that are genuinely
-- theirs — intent, caveats, welcome text — without forking the truth.
CREATE TABLE document_sections (
    id             uuid PRIMARY KEY,
    document_id    uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    org_id         uuid NOT NULL,
    position       int  NOT NULL,
    heading        text NOT NULL,
    mode           text NOT NULL,
    -- composed: {entities:[uuid], kinds:[text], lifecycle:[text], query:text, max_items:int}
    binding        jsonb,
    -- pinned: preserved byte-identical across every regeneration (eval gate).
    pinned_content text,
    CONSTRAINT document_sections_mode_check CHECK (mode IN ('composed', 'pinned')),
    -- A composed section without a binding would compose nothing; a pinned
    -- section without content would render nothing. Neither is a legal page.
    CONSTRAINT document_sections_shape_check CHECK (
        (mode = 'composed' AND binding IS NOT NULL)
        OR (mode = 'pinned' AND pinned_content IS NOT NULL)
    )
);

CREATE INDEX idx_sections_doc ON document_sections(document_id, position);

-- ── revisions ───────────────────────────────────────────────────────────
-- Immutable. `composed_from` is the provenance closure: the exact memory ids
-- that produced this markdown. A claim that cannot be traced to one of them is,
-- by definition, a hallucination — which is why the eval can gate on it.
CREATE TABLE document_revisions (
    id              uuid PRIMARY KEY,
    document_id     uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    org_id          uuid NOT NULL,
    content_md      text NOT NULL,
    composed_from   jsonb NOT NULL DEFAULT '[]'::jsonb,
    trigger         text NOT NULL,
    policy_decision text NOT NULL,
    reviewed_by     uuid,
    published_at    timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT revisions_trigger_check
        CHECK (trigger IN ('memory_change', 'manual', 'schedule')),
    CONSTRAINT revisions_policy_check
        CHECK (policy_decision IN ('auto_published', 'needs_review', 'rejected'))
);

CREATE INDEX idx_revisions_doc ON document_revisions(document_id, created_at DESC);
-- The review queue: revisions awaiting a human, oldest first (same 48h SLO
-- shape as promotions).
CREATE INDEX idx_revisions_pending ON document_revisions(org_id, created_at)
    WHERE policy_decision = 'needs_review' AND reviewed_by IS NULL;

-- ── dependencies (the inverted index that makes rot impossible) ──────────
-- Which pages does this memory feed? Written at compose time from
-- `composed_from`; read at memory-change time to mark pages dirty. Without this
-- table the wiki would have to be rebuilt wholesale (or, as everywhere else in
-- the industry, never).
CREATE TABLE document_dependencies (
    document_id uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    memory_id   uuid NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    org_id      uuid NOT NULL,
    PRIMARY KEY (document_id, memory_id)
);

CREATE INDEX idx_doc_deps_memory ON document_dependencies(memory_id);

-- ── RLS ─────────────────────────────────────────────────────────────────
-- Documents mirror the memories read policy exactly (org / team / private-by-
-- owner is not meaningful for a page, so a page is org- or team-visible). The
-- leak invariant that actually matters — a team-private memory must never
-- compose into an org page — is enforced upstream in the compose worker by
-- retrieving as a visibility-capped principal, and verified end-to-end by the
-- `docs` eval profile. This policy is the second line, not the first.
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;
CREATE POLICY documents_read ON documents FOR SELECT USING (
    org_id = current_setting('app.org_id')::uuid
    AND (
        visibility = 'org'
        OR (visibility = 'team' AND team_id IN
            (SELECT tm.team_id FROM team_members tm
             WHERE tm.user_id = current_setting('app.user_id')::uuid))
        OR current_setting('app.worker', true) = 'on'
    )
);
CREATE POLICY documents_write ON documents FOR INSERT WITH CHECK (
    org_id = current_setting('app.org_id')::uuid
);
CREATE POLICY documents_update ON documents FOR UPDATE USING (
    org_id = current_setting('app.org_id')::uuid
) WITH CHECK (
    org_id = current_setting('app.org_id')::uuid
);

-- Sections/revisions/dependencies inherit their document's reachability; they
-- are org-scoped and additionally gated by a visible parent document.
ALTER TABLE document_sections ENABLE ROW LEVEL SECURITY;
CREATE POLICY document_sections_org ON document_sections
    USING (org_id = current_setting('app.org_id')::uuid
           AND document_id IN (SELECT id FROM documents))
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE document_revisions ENABLE ROW LEVEL SECURITY;
CREATE POLICY document_revisions_org ON document_revisions
    USING (org_id = current_setting('app.org_id')::uuid
           AND document_id IN (SELECT id FROM documents))
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE document_dependencies ENABLE ROW LEVEL SECURITY;
CREATE POLICY document_dependencies_org ON document_dependencies
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

-- The app role is created in 0001 with grants on the tables that existed THEN;
-- every new table has to grant explicitly or the API/worker gets "permission
-- denied" at runtime (RLS still applies on top — grants are the outer gate).
GRANT SELECT, INSERT, UPDATE, DELETE ON documents TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON document_sections TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON document_revisions TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON document_dependencies TO brainiac_app;
