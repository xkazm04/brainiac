-- Federated identity → the one project a person owns (free-tier self-serve).
--
-- Stage 1: one Google account creates exactly ONE project (org). The PRIMARY KEY
-- on (provider, subject) is what enforces that — it makes provisioning idempotent
-- BY CONSTRUCTION rather than by a check-then-insert race: a second sign-in finds
-- the existing row and returns the same org instead of minting another. Two
-- concurrent first-sign-ins (double-clicked button, two tabs) collide on the key
-- and one loses, which is the correct outcome.
--
-- `subject` is the Firebase uid, not the email: emails get reassigned and change
-- case; the uid is stable for the life of the account. Email is stored alongside
-- for display/support only, never as the identity.
--
-- WHERE THIS GOES NEXT: when paid multi-user company accounts land, a person will
-- JOIN an existing org rather than own a fresh one. This table stays the join key
-- (provider+subject → user), and the "one org per identity" rule relaxes into
-- "one membership row per identity per org". Nothing here needs to be undone —
-- which is why the constraint lives on the identity, not on orgs.
CREATE TABLE identities (
    provider   text        NOT NULL,
    subject    text        NOT NULL,
    user_id    uuid        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    org_id     uuid        NOT NULL REFERENCES orgs(id)  ON DELETE CASCADE,
    email      text        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (provider, subject)
);

CREATE INDEX identities_user_idx ON identities (user_id);
CREATE INDEX identities_org_idx  ON identities (org_id);

-- NO grant to brainiac_app, and deliberately so.
--
-- Every other table grants the runtime role and leans on RLS to scope rows. This
-- one is provisioning metadata — a map of every account on the deployment to its
-- org — and nothing on the tenant request path ever needs to read it. There are no
-- DEFAULT PRIVILEGES in this schema, so withholding the grant means the runtime
-- role cannot touch the table at all: a strictly stronger guarantee than an RLS
-- policy, and one that cannot be defeated by a policy bug. Only the owner
-- (RLS-bypassing admin pool, used by the provisioning endpoint) reaches it.
