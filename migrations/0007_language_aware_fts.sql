-- Language-aware full-text search.
--
-- 0001_init.sql hard-coded the FTS config to 'english' at index time, so the
-- memories.language column was never honored: Czech memories (fixtures plant a
-- whole `language: cs` slice) were stemmed with English rules — wrong stems,
-- English stopword removal — degrading Czech recall.
--
-- Fix: regenerate the stored tsvector with a CASE over `language` picking the
-- regconfig. English keeps the 'english' config (stemming + stopwords help
-- English recall). Czech and 'unknown' use 'simple', which does no
-- language-specific stemming or stopword removal — the safe, no-false-stem
-- choice for content we can't stem correctly (Postgres ships no 'czech'
-- dictionary by default). Any other language falls back to 'english' as the
-- documented default. The generated expression is IMMUTABLE (each branch pins
-- a literal regconfig), which a STORED generated column requires.
--
-- Backfill-safe: recreating the STORED column recomputes it for every existing
-- row, and the CASE is total (ELSE branch), so no row is left without an fts.

DROP INDEX IF EXISTS idx_memories_fts;
ALTER TABLE memories DROP COLUMN content_fts;
ALTER TABLE memories ADD COLUMN content_fts tsvector GENERATED ALWAYS AS (
    CASE lower(language)
        WHEN 'cs' THEN to_tsvector('simple', content)
        WHEN 'unknown' THEN to_tsvector('simple', content)
        ELSE to_tsvector('english', content)
    END
) STORED;
CREATE INDEX idx_memories_fts ON memories USING gin(content_fts);
