-- The demand ledger — what agents ASKED FOR, answered or not.
--
-- Brainiac has always recorded the write side: memories, promotions,
-- contradictions, entities. The read side — the side the product is actually
-- paid for — was persisted nowhere. `retrieval::search` / `search_reranked`
-- had no instrumentation at all; `mcp.rs` even computed `withheld_contested`
-- and dropped it on the floor. Pages had `document_reads` (0025) and library
-- artifacts had `library_usage_events` (0028); the MEMORY, the primary
-- artifact, had nothing.
--
-- Four consequences, all structural rather than missing-feature:
--   * no answered-rate and no coverage number is COMPUTABLE, so the analytics
--     surface can only ever report inventory;
--   * unanswered demand is invisible, so the corpus grows toward whatever was
--     easy to ingest instead of toward what agents keep failing to find;
--   * `memory_feedback` attaches a verdict to a memory with no link to the
--     question it was serving, so "helpful" can never mean "this ANSWER
--     worked" — only "someone liked a row";
--   * a resolved contradiction cannot reach the agents that already acted on
--     the wrong belief, because nothing recorded who was served what.
--
-- ── The privacy shape is the design, not a footnote ────────────────────
--
-- 0028 states the invariant twice and enforces it in the schema: NEVER A
-- LEADERBOARD — `library_usage_events` has a team column and NO user column,
-- so per-person telemetry "is not 'not queried'; it is unrepresentable".
-- 0025 says the same thing differently: "we can see the door a reader came
-- through; we do not pretend to know who they are."
--
-- This table keeps that invariant: **there is no user column.** It does carry
-- `token_id` — the API key, which is the AGENT, and which is what makes the
-- fleet feedback loop possible (telling the agents that acted on a belief that
-- the belief was withdrawn). Recorded limitation, deliberately written down
-- rather than glossed: onboarding mints one key per developer, so an admin CAN
-- join `api_tokens.user_id` and recover a person. For that case the promise is
-- policy-enforced, not schema-enforced. The rule that follows from it: every
-- rendered aggregate groups by team, project or surface — never by token, and
-- never per person.

CREATE TABLE retrieval_events (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       uuid NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    -- Team and project, never person (see above). Both nullable: an org-wide
    -- key belongs to no project, and an env bootstrap token to no team.
    team_id      uuid REFERENCES teams(id)    ON DELETE SET NULL,
    project_id   uuid REFERENCES projects(id) ON DELETE SET NULL,
    -- The agent that asked. NULL for env bootstrap tokens, which have no row.
    token_id     uuid REFERENCES api_tokens(id) ON DELETE SET NULL,
    -- The door, exactly as 0025 models it: http = console/API, mcp = agents.
    surface      text NOT NULL CHECK (surface IN ('http', 'mcp')),
    -- Which entry point asked — 'memory_search', 'search', 'memory_context'.
    -- Distinguishes a deliberate query from a bundle assembled for a task.
    tool         text NOT NULL,
    -- The question as asked, and the form aggregates read. `query_text` is
    -- org-scoped under RLS and never leaves the tenant; `query_norm` is
    -- lowercased/whitespace-collapsed and identifier-preserving, so
    -- "PSP retry  idempotency" and "psp retry idempotency" are one demand row.
    query_text   text NOT NULL,
    query_norm   text NOT NULL,
    -- The answer's shape. `top_score` is NULL when nothing came back at all,
    -- which is a different fact from "came back weak" and must stay separable.
    returned_n   int  NOT NULL,
    top_score    double precision,
    -- Governance debt as demand pressure: how many hits were withheld because
    -- the org has an unreconciled contradiction touching them. Previously
    -- computed and discarded at mcp.rs:863.
    withheld_contested int NOT NULL DEFAULT 0,
    latency_ms   int,
    -- Reserved for the causal arm (a 1-in-N withhold experiment). Every row is
    -- 'served' until that ships; the column exists now so the ledger does not
    -- need a backfill to answer the question later.
    arm          text NOT NULL DEFAULT 'served' CHECK (arm IN ('served', 'withheld')),
    occurred_at  timestamptz NOT NULL DEFAULT now()
);

-- The window queries: "unanswered in the last 30 days", "asks per project".
CREATE INDEX idx_retrieval_events_window ON retrieval_events (org_id, occurred_at DESC);
-- The gap query groups by the normalized question, newest window first.
CREATE INDEX idx_retrieval_events_demand ON retrieval_events (org_id, query_norm, occurred_at DESC);

-- What was actually served, so a verdict or an outcome can name the memory it
-- is about. This is the half that makes attribution — and recall notices —
-- possible at all.
CREATE TABLE retrieval_event_memories (
    event_id   uuid NOT NULL REFERENCES retrieval_events(id) ON DELETE CASCADE,
    memory_id  uuid NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    org_id     uuid NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    rank       int  NOT NULL,
    score      double precision,
    PRIMARY KEY (event_id, memory_id)
);

CREATE INDEX idx_rem_memory ON retrieval_event_memories (org_id, memory_id);

-- ── RLS: append-only, and never an existence oracle ────────────────────
--
-- Append-only is enforced at the GRANT, not by convention (0025's rule):
-- analytics that can be rewritten are not analytics.
ALTER TABLE retrieval_events ENABLE ROW LEVEL SECURITY;
CREATE POLICY retrieval_events_insert ON retrieval_events FOR INSERT
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
CREATE POLICY retrieval_events_read ON retrieval_events FOR SELECT
    USING (org_id = current_setting('app.org_id')::uuid);

ALTER TABLE retrieval_event_memories ENABLE ROW LEVEL SECURITY;
CREATE POLICY rem_insert ON retrieval_event_memories FOR INSERT
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
-- SELECT is additionally gated on the memory still being VISIBLE to the
-- reader, exactly as 0025 gates page reads on a visible parent document.
-- Without this, an org-wide aggregate over served memories would answer
-- "does a team-private memory with this id exist?" — the existence-oracle
-- failure the visibility tiers exist to prevent.
CREATE POLICY rem_read ON retrieval_event_memories FOR SELECT
    USING (org_id = current_setting('app.org_id')::uuid
           AND memory_id IN (SELECT id FROM memories));

-- The link that turns a verdict about a ROW into a verdict about an ANSWER.
-- Nullable: console feedback and every pre-existing verdict have no event.
ALTER TABLE memory_feedback
    ADD COLUMN retrieval_event_id uuid REFERENCES retrieval_events(id) ON DELETE SET NULL;

-- New tables grant explicitly — 0001's blanket grant only covered the tables
-- that existed then. No UPDATE, no DELETE.
GRANT SELECT, INSERT ON retrieval_events         TO brainiac_app;
GRANT SELECT, INSERT ON retrieval_event_memories TO brainiac_app;
