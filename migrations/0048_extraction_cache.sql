-- Extraction call cache — stop paying twice for the same extraction.
--
-- Nothing on the LLM path was ever deduplicated. The only dedup that exists,
-- `memory_exists_for_source`, runs AFTER the model call: a redelivered job, an
-- overlapping chunk, or a re-ingested source paid full price to produce rows
-- that were then discarded as duplicates. The queue makes that routine rather
-- than exotic — 30s exponential backoff and a 300s visibility window mean
-- redelivery is a normal event, and chunk windows overlap 800 chars by
-- construction.
--
-- Extraction is deterministic on its first pass (temperature 0.0), so identical
-- input genuinely should yield identical output. This table remembers that
-- output.
--
-- ── The key IS the design ──────────────────────────────────────────────
--
-- A cache that outlives a prompt or a model change is worse than no cache: it
-- would serve last month's extraction of a transcript under this month's prompt
-- and nobody would ever see the difference. So `cache_key` is a sha256 over
-- three things, not one:
--
--   1. a fingerprint of EXTRACT_SYSTEM_PROMPT_V1 itself — not a version string
--      someone has to remember to bump. Editing one word of the prompt changes
--      every key, and the cache empties itself;
--   2. the model ref the EXTRACT stage will actually call, provider prefix
--      included (`qwen:qwen-max`). Per-stage overrides exist
--      (BRAINIAC_MODEL_EXTRACT), so this is not derivable from global config;
--   3. the REDACTED chunk text.
--
-- ── Redaction is upstream of the key, deliberately ─────────────────────
--
-- The key and the stored response are both built from `redact()` output, the
-- same secret firewall a memory body passes through. Two consequences, both
-- intended: no raw credential is ever retained in a cache row, and two chunks
-- that differ ONLY in a secret share a key. The second is safe precisely
-- because the secret can never reach a memory body anyway — it is redacted out
-- of `content` before the row is written — so the cached extraction is the same
-- extraction either way.
--
-- Only NON-EMPTY successful extractions are cached. Caching a zero would freeze
-- a stochastic failure into a permanent one and defeat the resample floor that
-- exists to catch it.

CREATE TABLE extraction_cache (
    org_id        uuid NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    -- sha256 hex of prompt fingerprint + model ref + redacted chunk.
    cache_key     text NOT NULL,
    -- The provider's raw response, REDACTED. Re-parsed on a hit by the same
    -- parser the live path uses, so a hit and a miss cannot diverge.
    response      text NOT NULL,
    -- Recorded for operators, never for matching — the key already binds them.
    model_ref     text NOT NULL,
    prompt_fingerprint text NOT NULL,
    -- Honest accounting: how many model calls this row has actually avoided.
    hits          int  NOT NULL DEFAULT 0,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_used_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, cache_key)
);

-- Eviction / operator queries ("what is this org's cache doing"), newest first.
CREATE INDEX idx_extraction_cache_age ON extraction_cache (org_id, last_used_at);

-- RLS: a cache row is derived from one org's transcript and is that org's
-- data. No cross-tenant read, ever — a shared cache would be a content oracle
-- ("does another org have a chunk that redacts to exactly this?").
ALTER TABLE extraction_cache ENABLE ROW LEVEL SECURITY;
CREATE POLICY extraction_cache_read ON extraction_cache FOR SELECT
    USING (org_id = current_setting('app.org_id')::uuid);
CREATE POLICY extraction_cache_insert ON extraction_cache FOR INSERT
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
-- UPDATE is for the hit counter only; USING and WITH CHECK both pin the org so
-- a row can neither be read nor re-homed across the tenant boundary.
CREATE POLICY extraction_cache_update ON extraction_cache FOR UPDATE
    USING (org_id = current_setting('app.org_id')::uuid)
    WITH CHECK (org_id = current_setting('app.org_id')::uuid);
-- DELETE so an operator sweep can evict; still org-pinned.
CREATE POLICY extraction_cache_delete ON extraction_cache FOR DELETE
    USING (org_id = current_setting('app.org_id')::uuid);

-- New tables grant explicitly (0001's blanket grant only covered then-existing
-- tables).
GRANT SELECT, INSERT, UPDATE, DELETE ON extraction_cache TO brainiac_app;
