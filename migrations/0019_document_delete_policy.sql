-- 0017 gave documents SELECT/INSERT/UPDATE policies but no DELETE policy, so
-- under RLS every delete silently affected zero rows — the failure mode Postgres
-- RLS is notorious for: not an error, just nothing happening. The `docs` eval
-- profile found it immediately (it re-seeds the tenant, and its second run
-- collided with the first run's page ids).
--
-- Deleting a page is a legitimate org action: a page is a projection, not
-- knowledge, so removing it destroys nothing that the memory layer does not
-- still hold. Sections/revisions/dependencies cascade from the FK.
CREATE POLICY documents_delete ON documents FOR DELETE USING (
    org_id = current_setting('app.org_id')::uuid
);
