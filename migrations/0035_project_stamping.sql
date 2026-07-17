-- PROJECT-PLAN PR0: stamp the write path with the project dimension.
--
-- Nullable ON PURPOSE, and NULL is a tier, not missing data: org-shared
-- knowledge (standards, conventions, cross-cutting decisions) legitimately
-- belongs to no project. This follows the 0023 title pattern (nullable +
-- fallback), NOT the 0015 lifecycle pattern (NOT NULL + default) — defaulting
-- to some project would launder org-knowledge into an application it does not
-- belong to. Reads treat "my project" as `project_id = X OR project_id IS
-- NULL` (PROJECT-PLAN principle 4); nothing filters implicitly.
--
-- No RLS change: project answers WHAT a row is about; visibility answers WHO
-- may see it. Historical rows stay NULL — honest: their project is unknown.

ALTER TABLE sources  ADD COLUMN project_id uuid REFERENCES projects(id);
ALTER TABLE memories ADD COLUMN project_id uuid REFERENCES projects(id);

-- Facet/lens lookups always arrive org-scoped first (RLS), then narrow.
CREATE INDEX idx_sources_org_project  ON sources  (org_id, project_id);
CREATE INDEX idx_memories_org_project ON memories (org_id, project_id);
