-- Opt-in per-project RLS isolation.
--
-- Today project_id is purely advisory (PROJECT-PLAN principle 4): a
-- project-scoped key can still read the whole org corpus, because project
-- answers WHAT a row is about, not WHO may see it. That is right for the
-- default org — cross-project knowledge sharing is the point. It is NOT
-- enough for a customer that needs contractual separation inside one org (an
-- agency's client projects): those rows must be invisible to org-wide and
-- other-project principals, enforced by RLS rather than by a filter a caller
-- could forget.
--
-- So a project may OPT IN to isolation. Default stays false, and the policy
-- below is written so that a non-isolated project's rows are byte-identically
-- visible to exactly whom they are today.
ALTER TABLE projects ADD COLUMN isolated boolean NOT NULL DEFAULT false;

-- RESTRICTIVE, so it is AND'd with the existing permissive `memories_read`
-- (migrations/0002) rather than replacing it: `memories_read` is UNCHANGED
-- and still answers the org/team/private visibility question; this policy can
-- only FURTHER constrain SELECT, never widen it. It applies to SELECT only, so
-- INSERT/UPDATE/DELETE — and thus the write path — are untouched.
--
-- Clause by clause (the correctness lives here):
--   (1) app.worker='on' — the pipeline worker (Store::worker_tx) sees
--       everything. It MUST: extraction/embedding/contradiction stages read
--       back rows they wrote, and blinding them on isolated rows would break
--       the pipeline for isolated projects.
--   (2) project_id IS NULL — org-shared knowledge (standards, conventions,
--       cross-cutting decisions) belongs to no project and is always visible.
--   (3) project_id = app.project_id — a principal scoped to the row's OWN
--       project sees it. nullif(...,'') turns the empty-string GUC (an
--       org-wide or unset principal) into NULL, so this clause simply does not
--       match for them (NULL = uuid is unknown/false) rather than erroring.
--   (4) NOT isolated — a NON-isolated project's rows stay visible to everyone,
--       exactly as today. THIS is the clause that guarantees the default is
--       byte-identical: with isolated=false the restrictive policy is
--       always-true, so it AND's harmlessly with memories_read. Only when a
--       project is flipped to isolated does (4) go false and (1)-(3) become
--       the sole gates.
CREATE POLICY memories_project_isolation ON memories AS RESTRICTIVE FOR SELECT
USING (
  coalesce(current_setting('app.worker', true), '') = 'on'
  OR project_id IS NULL
  OR project_id = nullif(current_setting('app.project_id', true), '')::uuid
  OR NOT coalesce((SELECT p.isolated FROM projects p WHERE p.id = memories.project_id), false)
);

-- The same isolation on sources. Sources today carry only `sources_org`
-- (permissive, org-only, FOR ALL); this restrictive SELECT policy is added
-- ALONGSIDE it — sources_org is unchanged, and writes stay untouched.
CREATE POLICY sources_project_isolation ON sources AS RESTRICTIVE FOR SELECT
USING (
  coalesce(current_setting('app.worker', true), '') = 'on'
  OR project_id IS NULL
  OR project_id = nullif(current_setting('app.project_id', true), '')::uuid
  OR NOT coalesce((SELECT p.isolated FROM projects p WHERE p.id = sources.project_id), false)
);
