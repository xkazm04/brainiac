-- Projects, the repo whitelist, and the onboarding pairing table.
--
-- WHY PROJECTS ARE NOT ORGS. The org stays the tenancy/billing/RLS root
-- (migration 0022: one identity = one org). A project is the LOGICAL unit
-- beneath it — an application or business domain — the thing a developer's
-- key should be scoped to and the thing memory will eventually attribute
-- knowledge to. A repo is the PHYSICAL unit: a normalized git remote. They
-- are deliberately separate tables because they don't map 1:1 — a project
-- can span several repos, and (later, via a path prefix) a monorepo can
-- split across projects.
--
-- WHY NO RLS, like api_tokens (0003): the whitelist is consulted during
-- onboarding approval and token minting — the machinery that PRODUCES
-- principals — and management queries enforce org scoping explicitly in SQL.

CREATE TABLE projects (
    id         uuid PRIMARY KEY,
    org_id     uuid NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name       text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    -- One name per org: the console's picker and the skill's output both
    -- address projects by name, and two "payments" would make both ambiguous.
    UNIQUE (org_id, name)
);

CREATE INDEX projects_org_idx ON projects (org_id);

-- The whitelist: which git remotes belong to which project. `remote` is the
-- NORMALIZED form ("github.com/owner/name" — lowercase host+owner, no
-- protocol, no .git); the server normalizes every submitted URL to this shape
-- so ssh/https/plain spellings of the same repo collide instead of coexisting.
-- UNIQUE(org_id, remote): within an org a repo maps to exactly ONE project —
-- that is what lets onboarding derive the project from the remote alone.
CREATE TABLE project_repos (
    id         uuid PRIMARY KEY,
    org_id     uuid NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    remote     text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, remote)
);

CREATE INDEX project_repos_project_idx ON project_repos (project_id);

-- A key can now be scoped to a project. NULL = org-wide (every existing token
-- keeps exactly the authority it had — zero-migration back-compat). The FK is
-- deliberately NOT ON DELETE SET NULL: nulling would silently WIDEN a
-- project-scoped key to org-wide when its project is deleted. Default NO
-- ACTION means a project with live keys cannot be dropped — revoke first.
ALTER TABLE api_tokens
    ADD COLUMN project_id uuid REFERENCES projects(id);

-- Onboarding pairing requests (the device-authorization pattern, self-hosted):
-- `POST /v1/onboard/start` is UNAUTHENTICATED — a developer's CLI has no
-- credentials yet; acquiring one is the point — so rows begin org-less and
-- acquire org/project only when an authenticated operator approves them in
-- the console.
--
-- Two codes per request, standard device-flow split:
--   user_code        — short, human-readable, shown in the console for the
--                      operator to match against what the developer sees.
--   device_code_hash — sha256 of the long secret the CLI polls with. Stored
--                      hashed for the same reason api_tokens hashes secrets:
--                      a database read must never yield a usable credential.
--
-- The minted token is NEVER stored here. Approval only marks the row; the
-- CLI's next poll mints the key at claim time (status → claimed, atomically),
-- so the secret exists exactly once, in one HTTP response, in transit to the
-- machine that will hold it.
CREATE TABLE onboard_requests (
    id               uuid PRIMARY KEY,
    user_code        text  NOT NULL UNIQUE,
    device_code_hash bytea NOT NULL UNIQUE,
    remote           text  NOT NULL,   -- normalized, same shape as project_repos.remote
    label            text  NOT NULL,   -- "who is asking": hostname/username, display only
    status           text  NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'approved', 'denied', 'claimed')),
    org_id           uuid REFERENCES orgs(id)     ON DELETE CASCADE,
    project_id       uuid REFERENCES projects(id) ON DELETE CASCADE,
    approved_by      uuid,             -- users.id of the approving operator
    created_at       timestamptz NOT NULL DEFAULT now(),
    expires_at       timestamptz NOT NULL,
    claimed_at       timestamptz
);

-- The console lists pending requests; the sweep below prunes dead ones.
CREATE INDEX onboard_requests_status_idx ON onboard_requests (status, expires_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON projects        TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON project_repos   TO brainiac_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON onboard_requests TO brainiac_app;
