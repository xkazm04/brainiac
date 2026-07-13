-- Persisted canonical-name embeddings (Direction 2): kill the O(n) live
-- re-embed of every canonical on every entity resolution. Written when a
-- canonical is created (and refreshed on any future rename/merge), read by a
-- single pgvector nearest-neighbour query in the resolve stage.
--
-- Shape mirrors memory_embeddings: keyed by embedding_version so a model swap
-- is a new version + backfill, never an in-place mutation. Like
-- memory_embeddings it carries no org_id — every access joins through
-- canonical_entities, whose org-scoped RLS constrains what is reachable.

CREATE TABLE canonical_entity_embeddings (
    canonical_id          uuid NOT NULL REFERENCES canonical_entities(id) ON DELETE CASCADE,
    embedding_version_id  int NOT NULL REFERENCES embedding_versions(id),
    embedding             vector NOT NULL,
    PRIMARY KEY (canonical_id, embedding_version_id)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON canonical_entity_embeddings TO brainiac_app;
