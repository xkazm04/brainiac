-- Idempotent ingest: let POST /v1/memories carry an Idempotency-Key so a
-- network retry returns the ORIGINAL source instead of minting a fresh one
-- (each duplicate source burns a full extraction pipeline / LLM call).
--
-- Additive and nullable: existing sources and every non-keyed insert keep
-- idempotency_key NULL and are unaffected. The uniqueness is scoped per org
-- (org_id is part of the key) and only applies to keyed rows — a PARTIAL
-- unique index WHERE idempotency_key IS NOT NULL, so the flood of NULL-keyed
-- sources never collides. Lifetime of a key == lifetime of its source row.

ALTER TABLE sources ADD COLUMN idempotency_key text;

CREATE UNIQUE INDEX sources_org_idempotency_key
    ON sources (org_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
