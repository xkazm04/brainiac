-- Poison-page compose backoff.
--
-- A page whose compose fails deterministically (a provider hard-reject, a
-- malformed binding, a persistently failing embedder) stayed dirty and was picked
-- up again by `dirty_documents` on the VERY NEXT tick — one LLM call per tick,
-- forever, never producing a revision. The ingest queue has MAX_ATTEMPTS,
-- exponential backoff and a dead-letter archive; the compose path had no attempt
-- counter, no backoff and no terminal state, so one poison page was an unbounded
-- money/quota drain that also crowded healthy pages out of each tick's limit.
--
-- Keeping the page dirty is still correct (a failed compose MUST retry, never
-- silently leave a stale page looking fresh) — but it must retry on a schedule.
--
-- `compose_attempts` also makes the stuck state queryable: previously a poison
-- page was permanently and invisibly stuck. Operators can now find them with
--   SELECT slug, compose_attempts, compose_next_at FROM documents
--   WHERE compose_attempts > 0 ORDER BY compose_attempts DESC;
ALTER TABLE documents
    ADD COLUMN compose_attempts int NOT NULL DEFAULT 0,
    ADD COLUMN compose_next_at  timestamptz;

-- The dirty scan filters on this, so keep it cheap.
CREATE INDEX IF NOT EXISTS documents_compose_next_at_idx
    ON documents (compose_next_at)
    WHERE dirty_at IS NOT NULL;
