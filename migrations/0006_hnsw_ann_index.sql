-- HNSW ANN index for pgvector cosine search (ARCHITECTURE.md §4 p95<150ms).
--
-- Until now every vector search was a full sequential scan over
-- memory_embeddings — 0001_init.sql deferred the ANN index to "the bake-off
-- winner". This adds it.
--
-- The dimension problem: memory_embeddings.embedding is a typmod-free `vector`
-- so several embedding versions with DIFFERENT dimensions can coexist
-- (deterministic-bow is 256-d, Qwen text-embedding-v4 is 1024-d, test fixtures
-- use tiny dims). pgvector's HNSW access method requires a FIXED dimension, so
-- a single index over the whole mixed column is impossible.
--
-- Resolution: one PARTIAL, EXPRESSION index per supported production dimension.
-- Each indexes `embedding::vector(N)` over only the rows whose
-- `vector_dims(embedding) = N`, so:
--   * inserts of any other dimension are simply not indexed (never rejected —
--     the partial predicate excludes them before the fixed-dim cast is applied),
--   * the planner uses the matching index only when the query constrains
--     `vector_dims(embedding) = N` (search_vector adds exactly that predicate,
--     which is a no-op on the result set because every row of a given
--     embedding_version already shares that version's dimension).
-- Dimensions without an index (e.g. the 4-d test vectors) fall back to a
-- correct sequential scan, which is fine at that scale.
--
-- vector_cosine_ops matches the `<=>` operator used by search_vector. Not
-- CONCURRENTLY: sqlx wraps each migration in a transaction and the tables are
-- empty/small at migration time, so a plain build under a brief lock is fine.

-- 256-d: the deterministic bag-of-tokens embedder (DeterministicEmbedder::DEFAULT_DIM).
CREATE INDEX IF NOT EXISTS idx_memory_embeddings_hnsw_256
    ON memory_embeddings USING hnsw ((embedding::vector(256)) vector_cosine_ops)
    WHERE vector_dims(embedding) = 256;

-- 1024-d: the Qwen text-embedding-v4 default (QwenEmbedder::DEFAULT_DIM).
CREATE INDEX IF NOT EXISTS idx_memory_embeddings_hnsw_1024
    ON memory_embeddings USING hnsw ((embedding::vector(1024)) vector_cosine_ops)
    WHERE vector_dims(embedding) = 1024;
