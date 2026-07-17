-- PROJECT-PLAN PR4: the knowledge base learns the project dimension.
--
-- Same semantics as memories/sources (0035): nullable, NULL = an org-wide
-- page. A project-stamped page composes from its project's memories PLUS
-- org-shared ones (the inclusive lens); a NULL page composes exactly as
-- before — no behavior change for every page that exists today.
ALTER TABLE documents ADD COLUMN project_id uuid REFERENCES projects(id);
CREATE INDEX idx_documents_org_project ON documents (org_id, project_id);
