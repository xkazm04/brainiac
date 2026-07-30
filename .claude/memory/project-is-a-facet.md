# Project is a facet, not a partition — and the isolation opt-in has a known gap

`docs/PROJECT-PLAN.md` owns this axis; PR0–PR4 are all built. Principles that
constrain new work: team = *who*, project = *what about*; project is a facet,
not a partition; no implicit filters; attribution never widens silently. The
project of a source is **derived** from the repo whitelist at approval time,
never chosen by the caller; the API key is minted at claim exactly once and
must never enter an agent transcript.

Mechanics worth remembering because they are non-obvious:

- **Opt-in RLS isolation (`0040_project_isolation.sql`)**: `projects.isolated`
  bool, default false. Isolation is implemented as **RESTRICTIVE** SELECT
  policies (`memories_project_isolation`, `sources_project_isolation`) AND'd
  with the permissive ones, so they can only further-constrain reads — writes
  are untouched, and for a non-isolated project the policy evaluates
  always-true, making default behaviour byte-identical. `Principal.project_id`
  reaches SQL as the `app.project_id` GUC via `scoped_tx` (None → '' → NULL);
  the worker (`app.worker='on'`) and org-shared rows (`project_id IS NULL`)
  stay visible. Toggle: `POST /v1/projects/{id}/isolation` (admin).
  **KNOWN GAP (recorded in PROJECT-PLAN Deferred): the KB compose path
  (`compose::admits`) does NOT honor isolation — an org page can still project
  an isolated project's memory.**
- **Monorepo `path_prefix` (`0039`)**: one remote splits across projects by
  repo-relative prefix; `''` means whole-repo (back-compat), uniqueness is
  `(org_id, remote, path_prefix)`, and resolution picks the **longest** match
  **segment-aware** — prefix `client` never claims `clientdata/`.
- **Retrieval is an inclusive lens**, not a filter: `project_id = X OR IS NULL`,
  so org-shared knowledge always comes along.
- **Self-hosted Git**: `normalize_remote` accepts dot-less bare-internal hosts
  (`git@git-internal:team/repo`) and strips numeric ports from scp-style and
  `ssh://host:2222/…` before segmenting — the older code corrupted the owner
  into the port.
- **MCP repo-fingerprint attribution**: an org-wide MCP key stamps a source's
  project from `BRAINIAC_REPO_REMOTE` (set at `claude mcp add -e`), normalized
  and looked up via `find_by_remote` scoped to the session's own org. Garbage or
  unresolvable remotes fall back to org-shared, never error. A project-scoped
  key's own project always wins.
- Demo/fixture projects are **application-shaped** (payments-api, checkout-web,
  feature-store) — never team names, which would collapse the axis back onto
  teams.
