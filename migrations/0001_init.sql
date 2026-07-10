-- Brainiac schema v0 — ARCHITECTURE.md §2 (schema core) + job queue.
-- Conventions: all tables carry org_id; RLS keys off org + team visibility;
-- timestamps are timestamptz; soft-delete via deleted_at where relevant.

CREATE EXTENSION IF NOT EXISTS vector;

-- ── identity & tenancy (§2.1) ───────────────────────────────────────────

CREATE TABLE orgs (
    id          uuid PRIMARY KEY,
    name        text NOT NULL,
    settings    jsonb NOT NULL DEFAULT '{}',
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE teams (
    id            uuid PRIMARY KEY,
    org_id        uuid NOT NULL REFERENCES orgs(id),
    name          text NOT NULL,
    idp_group_id  text,
    UNIQUE (org_id, idp_group_id)
);

CREATE TABLE users (
    id           uuid PRIMARY KEY,
    org_id       uuid NOT NULL REFERENCES orgs(id),
    idp_subject  text,
    email        text
);

CREATE TABLE team_members (
    team_id  uuid NOT NULL REFERENCES teams(id),
    user_id  uuid NOT NULL REFERENCES users(id),
    role     text NOT NULL DEFAULT 'member',   -- member | maintainer
    PRIMARY KEY (team_id, user_id)
);

-- ── sources & provenance (§2.2) ─────────────────────────────────────────

CREATE TABLE sources (
    id            uuid PRIMARY KEY,
    org_id        uuid NOT NULL,
    team_id       uuid,
    kind          text NOT NULL,        -- session_transcript | repo | doc | manual
    external_ref  text,
    raw_text      text,                 -- v0: raw content in PG (S3 later — PLAN.md deviation 5)
    content_hash  bytea,
    created_by    uuid,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE provenance (
    id               uuid PRIMARY KEY,
    org_id           uuid NOT NULL,
    actor_kind       text NOT NULL,     -- human | agent | pipeline
    actor_id         text NOT NULL,
    model_ref        text,
    source_id        uuid REFERENCES sources(id),
    pipeline_run_id  uuid,
    created_at       timestamptz NOT NULL DEFAULT now()
);

-- ── memories (§2.3) ─────────────────────────────────────────────────────

CREATE TYPE memory_status AS ENUM ('raw','candidate','canonical','deprecated','rejected');
CREATE TYPE visibility    AS ENUM ('private','team','org');

CREATE TABLE memories (
    id             uuid PRIMARY KEY,
    org_id         uuid NOT NULL,
    team_id        uuid,
    owner_user_id  uuid,
    visibility     visibility NOT NULL DEFAULT 'private',
    status         memory_status NOT NULL DEFAULT 'raw',
    kind           text NOT NULL,
    content        text NOT NULL,
    content_fts    tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED,
    language       text NOT NULL DEFAULT 'en',
    valid_from     timestamptz,
    valid_to       timestamptz,
    superseded_by  uuid,
    confidence     real,
    provenance_id  uuid REFERENCES provenance(id),
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);

CREATE INDEX idx_memories_org_status ON memories(org_id, status);
CREATE INDEX idx_memories_fts ON memories USING gin(content_fts);

CREATE TABLE embedding_versions (
    id          serial PRIMARY KEY,
    model_name  text NOT NULL,
    dim         int NOT NULL,
    is_active   boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now()
);

-- Embeddings in a side table keyed by version: model swap = new version row,
-- re-embed backfill, flip is_active. No in-place corpus mutation. Dimension
-- lives on embedding_versions; the column is typmod-free so multiple dims can
-- coexist during a migration (ANN index arrives with the bake-off winner).
CREATE TABLE memory_embeddings (
    memory_id             uuid NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    embedding_version_id  int NOT NULL REFERENCES embedding_versions(id),
    embedding             vector NOT NULL,
    PRIMARY KEY (memory_id, embedding_version_id)
);

-- ── graph (§2.4) — collision-tolerant ───────────────────────────────────

CREATE TABLE entities (
    id             uuid PRIMARY KEY,
    org_id         uuid NOT NULL,
    team_id        uuid,
    name           text NOT NULL,
    kind           text NOT NULL,
    aliases        text[] NOT NULL DEFAULT '{}',
    provenance_id  uuid REFERENCES provenance(id),
    created_at     timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE canonical_entities (
    id          uuid PRIMARY KEY,
    org_id      uuid NOT NULL,
    name        text NOT NULL,
    kind        text NOT NULL,
    summary     text,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE entity_links (
    entity_id      uuid NOT NULL REFERENCES entities(id),
    canonical_id   uuid NOT NULL REFERENCES canonical_entities(id),
    confidence     real NOT NULL,
    method         text NOT NULL,      -- embedding_block | llm_adjudicated | human
    confirmed_by   uuid,
    provenance_id  uuid REFERENCES provenance(id),
    PRIMARY KEY (entity_id, canonical_id)
);

CREATE TABLE edges (
    id             uuid PRIMARY KEY,
    org_id         uuid NOT NULL,
    src_entity     uuid NOT NULL REFERENCES entities(id),
    dst_entity     uuid NOT NULL REFERENCES entities(id),
    relation       text NOT NULL,
    memory_id      uuid REFERENCES memories(id),
    provenance_id  uuid REFERENCES provenance(id),
    valid_from     timestamptz,
    valid_to       timestamptz
);

CREATE TABLE memory_entities (
    memory_id  uuid NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    entity_id  uuid NOT NULL REFERENCES entities(id),
    PRIMARY KEY (memory_id, entity_id)
);

CREATE INDEX idx_entity_links_canonical ON entity_links(canonical_id);
CREATE INDEX idx_edges_src ON edges(src_entity);
CREATE INDEX idx_edges_dst ON edges(dst_entity);
CREATE INDEX idx_memory_entities_entity ON memory_entities(entity_id);

-- ── governance (§2.5) ───────────────────────────────────────────────────

CREATE TABLE promotions (
    id               uuid PRIMARY KEY,
    org_id           uuid NOT NULL,
    memory_id        uuid NOT NULL REFERENCES memories(id),
    from_status      memory_status NOT NULL,
    to_status        memory_status NOT NULL,
    policy_decision  text NOT NULL,    -- auto_approved | needs_review | denied
    policy_rule      text,
    reviewer_id      uuid,
    reviewed_at      timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE contradictions (
    id               uuid PRIMARY KEY,
    org_id           uuid NOT NULL,
    memory_a         uuid NOT NULL REFERENCES memories(id),
    memory_b         uuid NOT NULL REFERENCES memories(id),
    detected_by      text NOT NULL,
    status           text NOT NULL DEFAULT 'open',
    resolution_note  text,
    resolved_by      uuid,
    resolved_at      timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now()
);

-- ── pipeline runs (observability anchor for provenance.pipeline_run_id) ─

CREATE TABLE pipeline_runs (
    id          uuid PRIMARY KEY,
    org_id      uuid NOT NULL,
    stage       text NOT NULL,
    status      text NOT NULL DEFAULT 'running',  -- running | ok | failed
    detail      text,
    started_at  timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz
);

-- ── job queue (PLAN.md deviation 1: pgmq-shaped, extension-free) ────────

CREATE SCHEMA queue;

CREATE TABLE queue.jobs (
    id          bigserial PRIMARY KEY,
    queue_name  text NOT NULL,
    payload     jsonb NOT NULL,
    attempts    int NOT NULL DEFAULT 0,
    visible_at  timestamptz NOT NULL DEFAULT now(),
    enqueued_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_queue_jobs_ready ON queue.jobs(queue_name, visible_at);

CREATE TABLE queue.archive (
    id           bigint PRIMARY KEY,
    queue_name   text NOT NULL,
    payload      jsonb NOT NULL,
    attempts     int NOT NULL,
    enqueued_at  timestamptz NOT NULL,
    archived_at  timestamptz NOT NULL DEFAULT now(),
    outcome      text NOT NULL              -- ok | failed | dead
);

-- ── row-level security (§2.6) ───────────────────────────────────────────
-- The application connects as brainiac_app (non-owner, no BYPASSRLS) and sets
-- app.org_id / app.user_id per transaction from the verified principal.
-- Similarity search inherits RLS automatically — the pgvector scan only sees
-- permitted rows. v0 scope note: memories carry the full three-tier model;
-- graph tables are org-scoped (team-level graph privacy is deferred — the
-- sensitive text lives in memories).

DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'brainiac_app') THEN
        CREATE ROLE brainiac_app NOLOGIN;
    END IF;
END $$;

GRANT USAGE ON SCHEMA public, queue TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA queue TO brainiac_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public, queue TO brainiac_app;

ALTER TABLE memories ENABLE ROW LEVEL SECURITY;

CREATE POLICY memories_read ON memories FOR SELECT USING (
    org_id = current_setting('app.org_id')::uuid
    AND deleted_at IS NULL
    AND (
        visibility = 'org'
        OR (visibility = 'team' AND team_id IN
            (SELECT tm.team_id FROM team_members tm
             WHERE tm.user_id = current_setting('app.user_id')::uuid))
        OR (visibility = 'private' AND owner_user_id = current_setting('app.user_id')::uuid)
    )
);

CREATE POLICY memories_write ON memories FOR INSERT WITH CHECK (
    org_id = current_setting('app.org_id')::uuid
);

CREATE POLICY memories_update ON memories FOR UPDATE USING (
    org_id = current_setting('app.org_id')::uuid
) WITH CHECK (
    org_id = current_setting('app.org_id')::uuid
);

-- team_members: readable org-wide (membership is not a secret in v0), needed
-- by the memories_read subquery.
ALTER TABLE team_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY team_members_read ON team_members FOR SELECT USING (
    team_id IN (SELECT t.id FROM teams t WHERE t.org_id = current_setting('app.org_id')::uuid)
);
CREATE POLICY team_members_write ON team_members FOR INSERT WITH CHECK (
    team_id IN (SELECT t.id FROM teams t WHERE t.org_id = current_setting('app.org_id')::uuid)
);

-- Org-scoped RLS for the remaining tenant tables.
ALTER TABLE sources ENABLE ROW LEVEL SECURITY;
CREATE POLICY sources_org ON sources USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
ALTER TABLE provenance ENABLE ROW LEVEL SECURITY;
CREATE POLICY provenance_org ON provenance USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
ALTER TABLE entities ENABLE ROW LEVEL SECURITY;
CREATE POLICY entities_org ON entities USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
ALTER TABLE canonical_entities ENABLE ROW LEVEL SECURITY;
CREATE POLICY canonical_entities_org ON canonical_entities USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
ALTER TABLE edges ENABLE ROW LEVEL SECURITY;
CREATE POLICY edges_org ON edges USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
ALTER TABLE promotions ENABLE ROW LEVEL SECURITY;
CREATE POLICY promotions_org ON promotions USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
ALTER TABLE contradictions ENABLE ROW LEVEL SECURITY;
CREATE POLICY contradictions_org ON contradictions USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
ALTER TABLE pipeline_runs ENABLE ROW LEVEL SECURITY;
CREATE POLICY pipeline_runs_org ON pipeline_runs USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

-- memory_embeddings / memory_entities / entity_links: access is always joined
-- through their parent row; the join target's RLS constrains what is
-- reachable. Direct scans stay open to the app role (no org_id column), which
-- is acceptable because ids are unguessable UUIDs AND every read path in the
-- store goes through the parent join. Revisit before SaaS multi-tenancy.
