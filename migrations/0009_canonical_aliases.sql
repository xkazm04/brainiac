-- Canonical alias set (Direction 3): as raw surface forms link to a canonical,
-- their names and captured aliases accumulate here, so a later sighting of any
-- known surface form (e.g. an acronym from another team) resolves by an exact
-- lexical hit — no embedding round-trip, no model, and independent of any
-- hand-seeded fixture. Raw-entity aliases already live on entities.aliases;
-- this column is their org-level union at the canonical.

ALTER TABLE canonical_entities ADD COLUMN aliases text[] NOT NULL DEFAULT '{}';
