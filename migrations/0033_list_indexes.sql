-- Indexes for server-side list/paginate/search over the corpus.
--
-- The console moved every list surface (archive, wiki) from "fetch the whole
-- RLS-scoped corpus and filter in the browser" to "filter, order, page and
-- facet in the database". At 1–10k memories and pages that is the only shape
-- that stays under a keystroke. These are the indexes that make it cheap; the
-- query shapes live in brainiac-store (memories.rs / documents.rs).

-- ── ordered paging ────────────────────────────────────────────────────────
-- The archive orders by (created_at DESC, id) and pages with LIMIT/OFFSET. That
-- sort had no supporting index — fine at 80 rows, a full scan + sort at 10k.
create index if not exists idx_memories_org_created
  on memories (org_id, created_at desc, id);

-- The wiki orders by (updated_at DESC, id). Same story.
create index if not exists idx_documents_org_updated
  on documents (org_id, updated_at desc, id);

-- ── title / slug search ───────────────────────────────────────────────────
-- Memory CONTENT search already has a GIN tsvector (idx_memories_fts,
-- content_fts, migration 0007). But the archive's search box is title-first —
-- an operator types the name of the thing — and documents have no FTS at all.
-- Trigram indexes make `ILIKE '%term%'` on those short text columns indexed
-- rather than a scan, which is what the list handlers OR alongside the tsvector.
create extension if not exists pg_trgm;

create index if not exists idx_memories_title_trgm
  on memories using gin (title gin_trgm_ops);

create index if not exists idx_documents_title_trgm
  on documents using gin (title gin_trgm_ops);

create index if not exists idx_documents_slug_trgm
  on documents using gin (slug gin_trgm_ops);
