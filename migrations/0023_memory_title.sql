-- A short title on every memory.
--
-- The archive has always rendered a truncated `content` as a row's identity,
-- which is how a 1,400-row corpus reads as a wall of half-sentences: the column
-- that should anchor the eye is the one carrying the most text. `content` is the
-- claim — it has to stay long enough to be true on its own, because a memory is
-- served to an agent without its row. So the label is a separate field, not a
-- shorter content.
--
-- NULLABLE on purpose, and it stays nullable:
--   * every memory that already exists has no title, and inventing one from the
--     first N characters at migration time would bake a truncation into the
--     schema and call it a title;
--   * the extractor does not write one yet (see brainiac-pipeline), so memories
--     arriving from the pipeline will have NULL until it does.
-- Readers therefore MUST fall back to `content`. A NOT NULL here would force
-- every writer to make a title up.
alter table memories add column title text;

-- The one invariant worth enforcing: a title is a label, not a paragraph. 120
-- characters is roughly a headline; past that it is prose wearing a title's
-- clothes and the archive is back where it started.
alter table memories
  add constraint memories_title_len check (title is null or char_length(title) <= 120);

-- The archive's search is a title-first lookup — an operator types the name of
-- the thing, not a phrase from the middle of the claim.
create index idx_memories_title on memories (org_id, title) where title is not null;
