-- Knowledge gaps — the miss list becomes WORK.
--
-- 0043 gave the org a record of what it does not know: every retrieval is
-- logged with whether anything came back, and `retrieval_events::top_unanswered`
-- already ranks the most-asked, least-answered questions by breadth across
-- projects then volume. Exactly one caller read any of it — an analytics
-- endpoint. The highest-signal artifact the product generates, *the questions
-- agents keep asking that memory cannot answer*, was a number on a page.
--
-- A number is not work. Work has a state, an owner-ish gate, and an end. This
-- table is the artifact: one row per distinct unanswered question, with a
-- lifecycle borrowed wholesale from the two governance artifacts that already
-- work this way — `contradictions` (open → resolved/dismissed, human-gated) and
-- `practice_divergences` (minted by a scan, given a STABLE identity in 0042 so
-- re-running the scan updates the row instead of minting a twin).
--
-- ── Why a separate table and not a column on the ledger ────────────────
--
-- 0043 grants `brainiac_app` only SELECT and INSERT on `retrieval_events`, and
-- says why: "analytics that can be rewritten are not analytics". A gap has to
-- be mutable — that is the entire point of it being work. So the ledger stays
-- exactly as append-only as it was, and the mutable artifact lives here,
-- derived from it. The ledger's other invariant is inherited by construction:
-- a gap aggregates over `query_norm`, which carries team/project/surface
-- attribution and NO user column, so nothing here can become per-person
-- telemetry either. The one uuid pointing at a person, `resolved_by`, names
-- the maintainer who DISMISSED a gap — the same shape as a resolved
-- contradiction, and about the reviewer, never the asker.
--
-- ── Identity, so the same gap does not reappear ────────────────────────
--
-- `(org_id, query_norm)` is unique, exactly as 0042 made
-- `(org_id, canonical_id, axis)` unique for divergences after discovering that
-- a re-minting scan produced a fresh row (and a dangling provenance trail) on
-- every run. Asking the same unanswered question a fourth time updates this
-- row's evidence; it does not create a second gap.

CREATE TABLE knowledge_gaps (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       uuid NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,

    -- The question, in the aggregate form the ledger groups by (lowercased,
    -- whitespace-collapsed, identifiers intact — see 0043 and
    -- `retrieval_events::normalize_query`), plus one verbatim sample so a
    -- human reading the backlog sees how it was actually typed.
    query_norm   text NOT NULL,
    query_sample text NOT NULL,

    -- open      → nobody has answered it and nobody has judged it noise
    -- answered  → the corpus can now answer it; closed by the scan, not a human
    -- dismissed → a maintainer judged it not worth acquiring (a one-off, a
    --             malformed query). Terminal: the scan never re-mints it.
    status       text NOT NULL DEFAULT 'open'
                 CHECK (status IN ('open', 'answered', 'dismissed')),

    -- The evidence, refreshed on every scan. `projects` first because breadth
    -- beats volume: one chatty repo is not an org-wide gap (0043's ordering,
    -- and the reason the non-goal here is "invent a second heuristic").
    asks         int NOT NULL,
    projects     int NOT NULL,
    -- Best anything scored for it while it was failing. NULL means nothing came
    -- back AT ALL, which is a sharper failure than "came back weak".
    best_score   double precision,

    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_asked_at timestamptz NOT NULL,

    -- How it ended.
    resolved_at       timestamptz,
    -- The memory that now answers it — the observable proof the loop closed.
    -- NULL for a dismissal.
    resolved_memory_id uuid REFERENCES memories(id) ON DELETE SET NULL,
    -- The maintainer who dismissed it. NULL when the scan closed it, because
    -- no person did.
    resolved_by       uuid REFERENCES users(id) ON DELETE SET NULL,
    resolution_note   text,

    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- The identity that makes the scan idempotent (see the 0042 note above).
CREATE UNIQUE INDEX idx_knowledge_gaps_identity ON knowledge_gaps (org_id, query_norm);
-- The backlog read: open gaps, breadth then volume — the ordering 0043 already
-- established for the miss list, kept rather than reinvented.
CREATE INDEX idx_knowledge_gaps_backlog
    ON knowledge_gaps (org_id, status, projects DESC, asks DESC);

ALTER TABLE knowledge_gaps ENABLE ROW LEVEL SECURITY;
CREATE POLICY knowledge_gaps_org ON knowledge_gaps
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);

-- New table → grants itself (0001's blanket grant only covered then-existing
-- tables). UPDATE is deliberate and is what separates this from the ledger it
-- is derived from; no DELETE — a dismissed gap is a decision worth keeping.
GRANT SELECT, INSERT, UPDATE ON knowledge_gaps TO brainiac_app;
