-- Managed API tokens (replaces relying solely on the BRAINIAC_TOKENS env
-- stub). Secrets are never stored: only a sha256 hash plus a display prefix.
--
-- Deliberately NO row-level security here: token resolution happens BEFORE
-- a principal exists (it is what produces the principal), so the lookup runs
-- outside any app.org_id-scoped transaction. Management queries enforce org
-- scoping explicitly in SQL instead.

CREATE TABLE api_tokens (
    id            uuid PRIMARY KEY,
    org_id        uuid NOT NULL,
    user_id       uuid NOT NULL,          -- principal the token acts as
    name          text NOT NULL,
    prefix        text NOT NULL,          -- e.g. brk_1a2b3c4d… (display only)
    token_hash    bytea NOT NULL UNIQUE,  -- sha256(full secret)
    scopes        text[] NOT NULL DEFAULT '{read}',  -- read | write | admin
    created_by    uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_used_at  timestamptz,
    revoked_at    timestamptz
);

CREATE INDEX idx_api_tokens_org ON api_tokens(org_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON api_tokens TO brainiac_app;
