-- LB0 (docs/LIBRARY-PLAN.md): the Library substrate — the normative layer.
--
-- The governing principle: A RULE IS THE ATOM. A standard is one rule with a
-- typed identity (stack → category → slug), one-sentence statement, examples,
-- an enforcement level, and a lifecycle — never a forty-page document. You
-- cannot measure a document; you can measure a rule. Skills are versioned
-- bundles in the open agent-skill format (LIBRARY-PLAN L4).
--
-- Two invariants live in this schema rather than in code, deliberately:
--
--   1. NO UNATTRIBUTED RULES (L-never #4): adopting a standard requires
--      provenance rows or an explicit named decree — enforced by
--      `standards_attribution_check` below, so no code path (present or
--      future) can create an adopted rule that cannot say why it exists.
--
--   2. NEVER A LEADERBOARD (L-never #1): `library_usage_events` has a team
--      column and NO user column. Per-person telemetry is not "not queried";
--      it is unrepresentable.
--
-- (0016 is practice_divergences — the Library's shipped ancestor and the
--  source of the L6 ratification bridge.)

-- ── standards: the rule is the atom ─────────────────────────────────────
CREATE TABLE standards (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       uuid NOT NULL,
    stack        text NOT NULL,             -- 'rust' | 'typescript' | 'general' | ...
    category     text NOT NULL,             -- 'errors' | 'testing' | 'practice' | ...
    slug         text NOT NULL,             -- deep-linkable within the org
    statement    text NOT NULL,             -- ONE sentence (mirrors memory content)
    rationale    text,
    detail_md    text,                      -- good/bad examples (same vocabulary as memories.detail_md)
    enforcement  text NOT NULL DEFAULT 'recommended',
    lifecycle    text NOT NULL DEFAULT 'proposed',
    -- The named human who ratified (NULL while proposed). `decreed_by` marks
    -- the only legal kind of evidence-free rule: one a named human signed for.
    adopted_by   uuid,
    adopted_at   timestamptz,
    decreed_by   uuid,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT standards_slug_unique UNIQUE (org_id, slug),
    CONSTRAINT standards_enforcement_check
        CHECK (enforcement IN ('mandatory', 'recommended', 'experimental')),
    CONSTRAINT standards_lifecycle_check
        CHECK (lifecycle IN ('proposed', 'adopted', 'deprecated')),
    -- Leaving `proposed` requires a named human on record.
    CONSTRAINT standards_adoption_check
        CHECK (lifecycle = 'proposed' OR adopted_by IS NOT NULL)
);

CREATE INDEX idx_standards_org_stack ON standards (org_id, stack, category);
CREATE INDEX idx_standards_org_lifecycle ON standards (org_id, lifecycle);

-- ── versions: every change to a rule is a numbered revision ─────────────
CREATE TABLE standard_versions (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    standard_id uuid NOT NULL REFERENCES standards(id) ON DELETE CASCADE,
    org_id      uuid NOT NULL,
    rev         int  NOT NULL,
    statement   text NOT NULL,
    rationale   text,
    detail_md   text,
    enforcement text NOT NULL,
    author      uuid,                       -- who wrote this revision (a human or the bridge's ratifier)
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT standard_versions_rev_unique UNIQUE (standard_id, rev)
);

-- ── provenance: a rule can say why it exists ─────────────────────────────
-- kind='memory'     → ref_id is a memories.id (the incident / decision behind it)
-- kind='divergence' → ref_id is a practice_divergences.id (the L6 bridge)
-- No FK on ref_id: memories and divergences have independent retention, and a
-- provenance row outliving its source is an audit trail, not a dangling error.
CREATE TABLE standard_provenance (
    standard_id uuid NOT NULL REFERENCES standards(id) ON DELETE CASCADE,
    org_id      uuid NOT NULL,
    kind        text NOT NULL,
    ref_id      uuid NOT NULL,
    PRIMARY KEY (standard_id, kind, ref_id),
    CONSTRAINT standard_provenance_kind_check CHECK (kind IN ('memory', 'divergence'))
);

CREATE INDEX idx_std_prov_ref ON standard_provenance (org_id, kind, ref_id);

-- Invariant 1, enforced at the row level: an adopted/deprecated standard must
-- carry provenance OR a named decree. A trigger (not a CHECK) because it reads
-- another table. A DEFERRED constraint trigger on INSERT and UPDATE, so (a) a
-- rule inserted directly as adopted is checked too, and (b) provenance rows
-- written later in the same transaction still count — the check runs at
-- commit, when the transaction's whole story is on the table.
CREATE FUNCTION standards_attribution_check() RETURNS trigger AS $$
BEGIN
    IF NEW.lifecycle <> 'proposed' AND NEW.decreed_by IS NULL THEN
        IF NOT EXISTS (SELECT 1 FROM standard_provenance sp WHERE sp.standard_id = NEW.id) THEN
            RAISE EXCEPTION 'standard % cannot leave proposed without provenance or a named decree', NEW.id
                USING ERRCODE = 'check_violation';
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE CONSTRAINT TRIGGER standards_attribution
    AFTER INSERT OR UPDATE OF lifecycle, decreed_by ON standards
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION standards_attribution_check();

-- ── skills: versioned bundles in the open agent-skill format ─────────────
CREATE TABLE skills (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          uuid NOT NULL,
    slug            text NOT NULL,
    name            text NOT NULL,
    description     text,
    domain          text,                   -- task-domain facet for the catalog
    maturity        text NOT NULL DEFAULT 'draft',
    current_version uuid,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT skills_slug_unique UNIQUE (org_id, slug),
    CONSTRAINT skills_maturity_check
        CHECK (maturity IN ('draft', 'published', 'deprecated'))
);

CREATE TABLE skill_versions (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_id     uuid NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    org_id       uuid NOT NULL,
    semver       text NOT NULL,
    -- The bundle, stored inline (LIBRARY-PLAN deviation #3): the manifest
    -- front-matter as jsonb, the markdown body, and any auxiliary resources as
    -- [{path, content}]. Content-addressed object storage is a revisit point,
    -- not a v1 requirement.
    manifest     jsonb NOT NULL DEFAULT '{}'::jsonb,
    content_md   text NOT NULL,
    resources    jsonb NOT NULL DEFAULT '[]'::jsonb,
    published_by uuid,                      -- the named human; NULL means draft, never served
    published_at timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT skill_versions_semver_unique UNIQUE (skill_id, semver)
);

CREATE INDEX idx_skill_versions_skill ON skill_versions (skill_id, created_at DESC);

-- ── usage: the vital signs ────────────────────────────────────────────────
-- Invariant 2: team, never person. There is no user column to query, join, or
-- subpoena. `version` is text (a semver or a standard rev) — display data.
CREATE TABLE library_usage_events (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    org_id        uuid NOT NULL,
    artifact_kind text NOT NULL,
    artifact_id   uuid NOT NULL,
    version       text,
    event         text NOT NULL,
    team_id       uuid,                     -- nullable: a token may be org-scoped
    occurred_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT lue_kind_check CHECK (artifact_kind IN ('standard', 'skill')),
    CONSTRAINT lue_event_check CHECK (event IN ('fetch', 'check', 'apply'))
);

CREATE INDEX idx_lue_org_artifact
    ON library_usage_events (org_id, artifact_kind, artifact_id, occurred_at);

-- ── RLS ──────────────────────────────────────────────────────────────────
-- Library artifacts are org-visible by design (a standard is the org's shared
-- judgment; a team-private rule is a contradiction in terms), so the policy is
-- the plain org scope — the same shape as practice_divergences.
ALTER TABLE standards ENABLE ROW LEVEL SECURITY;
CREATE POLICY standards_org ON standards
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE standard_versions ENABLE ROW LEVEL SECURITY;
CREATE POLICY standard_versions_org ON standard_versions
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE standard_provenance ENABLE ROW LEVEL SECURITY;
CREATE POLICY standard_provenance_org ON standard_provenance
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE skills ENABLE ROW LEVEL SECURITY;
CREATE POLICY skills_org ON skills
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE skill_versions ENABLE ROW LEVEL SECURITY;
CREATE POLICY skill_versions_org ON skill_versions
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE library_usage_events ENABLE ROW LEVEL SECURITY;
CREATE POLICY lue_org ON library_usage_events
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

-- New tables → grants themselves (0001's blanket grant covered then-existing
-- tables only). RLS applies on top — grants are the outer gate.
GRANT SELECT, INSERT, UPDATE, DELETE ON standards TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON standard_versions TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON standard_provenance TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON skills TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON skill_versions TO brainiac_app;
GRANT SELECT, INSERT, DELETE ON library_usage_events TO brainiac_app;
