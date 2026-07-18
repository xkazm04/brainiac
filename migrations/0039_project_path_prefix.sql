-- Monorepo support: split one remote across several projects by path.
--
-- Until now `project_repos` had UNIQUE(org_id, remote), so a remote mapped to
-- exactly ONE project — a monorepo (one remote, many apps) could not split
-- across projects at all. `path_prefix` is the repo-relative subdirectory a
-- row claims ('' = the whole repo, back-compat default for every existing
-- row). `github.com/acme/mono` + `apps/web` can now be project A while the
-- same remote + `apps/api` is project B; resolution picks the LONGEST
-- matching prefix (see brainiac_store::projects::find_by_remote), so a more
-- specific split always wins over a whole-repo fallback row.
ALTER TABLE project_repos
    ADD COLUMN path_prefix text NOT NULL DEFAULT '';

ALTER TABLE project_repos
    DROP CONSTRAINT project_repos_org_id_remote_key;

ALTER TABLE project_repos
    ADD CONSTRAINT project_repos_org_id_remote_path_prefix_key
    UNIQUE (org_id, remote, path_prefix);

-- The checkout-relative subdir an onboarding request was opened from, so
-- approval can resolve the same remote's split across projects by
-- longest-prefix match instead of assuming whole-repo. '' (default, same as
-- project_repos.path_prefix) means "no subdir given" — whole-repo lookup,
-- identical to today's behavior for every existing/back-compat caller.
ALTER TABLE onboard_requests
    ADD COLUMN path text NOT NULL DEFAULT '';
