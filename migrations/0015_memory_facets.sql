-- KB0 (docs/KB-PLAN.md D2, D3): the two memory facets the document layer needs
-- to exist BEFORE composition does, because extraction must start populating
-- them now — a page compiled from sentence-atoms with no lifecycle signal is
-- exactly the doc-rot we are trying to kill.
--
-- lifecycle — "what is in product" vs "what is on its way". Temporal validity
--   (valid_from/valid_to) answers WHEN a belief held; it cannot answer whether
--   the belief describes shipped reality or a roadmap intent. A decision memory
--   about an unshipped feature is true *about the plan* and false *about
--   production*; composed pages must be able to split those. Default 'shipped':
--   every pre-existing memory describes the world as it was captured, and the
--   extractor only departs from the default when the transcript is explicit.
--
-- detail_md — the structure-preserving payload. `content` stays the distilled
--   one-sentence statement (retrieval, FTS and embeddings keep pointing at it,
--   unchanged); detail_md optionally carries the code block / table / config
--   snippet the sentence summarizes. Extraction currently flattens all structure
--   away, which is a hard quality ceiling for composed prose. Deliberately NOT
--   in content_fts: search matches the claim, the page renders the evidence.

ALTER TABLE memories
    ADD COLUMN lifecycle text NOT NULL DEFAULT 'shipped',
    ADD COLUMN detail_md text;

ALTER TABLE memories
    ADD CONSTRAINT memories_lifecycle_check
    CHECK (lifecycle IN ('shipped', 'in_flight', 'proposed'));

-- Pages bind on (entity, kind, lifecycle); leadership currency views count
-- in-flight beliefs separately from shipped ones.
CREATE INDEX idx_memories_org_lifecycle ON memories(org_id, lifecycle);
