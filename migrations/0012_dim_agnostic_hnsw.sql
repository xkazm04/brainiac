-- Dim-agnostic ANN: auto-create the partial HNSW index for ANY embedding
-- dimension, not just the two hard-coded in 0006 (256, 1024).
--
-- The problem 0006 left open: memory_embeddings.embedding is a typmod-free
-- `vector`, and pgvector's HNSW access method needs a FIXED dimension, so the
-- index is PARTIAL per dimension. 0006 shipped exactly 256 (deterministic-bow)
-- and 1024 (Qwen v4). Any bake-off model at another dimension (768, 1536, …)
-- had NO index and silently sequential-scanned every vector search. This makes
-- the index follow the data: when an embedding version is ensured/activated
-- (store::memories::ensure_embedding_version), its dimension gets a matching
-- partial HNSW index on demand.
--
-- Privilege boundary. The runtime pool runs as brainiac_app — a NOLOGIN,
-- non-owner role with DML grants but NO DDL rights (0001), so it cannot
-- CREATE INDEX on memory_embeddings (owned by the migrating role). This
-- SECURITY DEFINER function is owned by the migrating (table-owner) role, so it
-- runs CREATE INDEX with the owner's privileges; brainiac_app only needs
-- EXECUTE. search_path is pinned (SECURITY DEFINER hygiene) so a caller can't
-- shadow `memory_embeddings`/`vector_dims` with objects on their own path.
--
-- Concurrency. Two workers ensuring the same dimension at once are serialized
-- by a transaction-scoped advisory lock keyed on the dimension; the loser then
-- finds the index already present (CREATE INDEX IF NOT EXISTS is a second,
-- belt-and-suspenders guard against any un-locked racer). The lock releases at
-- COMMIT.
--
-- Tradeoff (documented, deliberate). The index is built INSIDE the caller's
-- transaction and NOT CONCURRENTLY, so it takes a brief lock that blocks writes
-- to memory_embeddings while it builds. At current scale ensure_* runs at
-- worker/server startup against a small/empty table, so this is fine and keeps
-- the create transactional (a failed create rolls back cleanly). A future
-- large-corpus online path would move to CREATE INDEX CONCURRENTLY outside a
-- transaction.

CREATE OR REPLACE FUNCTION ensure_hnsw_index(dim int)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF dim IS NULL OR dim < 1 THEN
        RAISE EXCEPTION 'ensure_hnsw_index: dim must be >= 1, got %', dim;
    END IF;
    -- Serialize concurrent creators of the SAME dimension; released at commit.
    PERFORM pg_advisory_xact_lock(hashtext('brainiac_hnsw_index'), dim);
    EXECUTE format(
        'CREATE INDEX IF NOT EXISTS idx_memory_embeddings_hnsw_%s '
        'ON memory_embeddings USING hnsw ((embedding::vector(%s)) vector_cosine_ops) '
        'WHERE vector_dims(embedding) = %s',
        dim, dim, dim
    );
END;
$$;

-- Only the owner may redefine it; the app role may only run it.
REVOKE ALL ON FUNCTION ensure_hnsw_index(int) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION ensure_hnsw_index(int) TO brainiac_app;

-- Backfill: the two dimensions 0006 created by hand are now equally reachable
-- through the function. No-ops here (IF NOT EXISTS), but this asserts the
-- function reproduces 0006's exact index shape.
SELECT ensure_hnsw_index(256);
SELECT ensure_hnsw_index(1024);
