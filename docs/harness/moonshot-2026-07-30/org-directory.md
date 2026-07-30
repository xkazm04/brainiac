# Moonshot scan — org-directory

> Total: 2
> Files read: 11

## 1. The org chart becomes the review engine — teams that own code and route work

- **Severity**: Critical
- **Category**: governance-throughput
- **File**: `crates/brainiac-store/src/orgs.rs:20` (`upsert_team`) and `:56` (`upsert_member`) — the only
  writers of organizational structure in the product, reachable from nowhere but fixtures.
  Anchors: `crates/brainiac-server/src/console.rs:102` (`is_maintainer`), `:1261`
  (`is_any_maintainer`), `:4250` (`org_users`, read-only, `admin`-gated),
  `crates/brainiac-server/src/http.rs:599` and `:653` (`body.team_id.or_else(|| principal.team_ids.first().copied())`),
  `crates/brainiac-store/src/identities.rs:83-84` (one team named `"workspace"`, one maintainer),
  `migrations/0039_project_path_prefix.sql` (longest-prefix repo→project resolution).

- **Scenario**: An org with 200 engineers points Brainiac at its repos. `apps/payments` knowledge
  routes to the payments team's queue; `platform/*` routes to platform. A payments maintainer opens
  the console and sees *their* 14 pending promotions, not the org's 900 — with queue depth, oldest
  pending age, and an escalation for anything unreviewed past its SLA. Knowledge health is reported
  per team, so "payments has 40% stale canon and a 9-day review lag" is a number a team lead owns.
  New hires join by invitation into an existing org, and a lead promotes someone to maintainer from
  a Directory page. The governance loop stops being one person's inbox and becomes the org's
  distributed review process — which is the only shape in which it survives contact with a real
  company.

- **Root cause**: Teams are load-bearing for authority but inert as structure. Every governance gate
  in the product — promotion approve/reject (`console.rs:196`), bulk review (`:396`), contradiction
  supersede (`:693`), feedback answer (`:1112`), re-verify (`:1400`), doc publish
  (`docs.rs:496,609`) — asks "is the caller a maintainer of the owning team". But **no endpoint
  creates a team, a user, or a membership.** `upsert_team`/`upsert_member` are called only from
  `brainiac-eval` seeding and from `identities::provision`, which hardcodes one team `"workspace"`
  with one maintainer. There is no invite, no join, no role change, no team CRUD; `/v1/org/users` can
  only *read* the directory. Consequently `principal.team_ids` is almost always a single element,
  and ingest stamps `memories.team_id` with `team_ids.first()` — an arbitrary pick that happens to be
  correct only because there is exactly one team. Owning-team authority is therefore a gate with
  nothing behind it: the reviewer set is always {the operator}, and review throughput is capped at
  one human regardless of how many the customer employs. Meanwhile the physical→logical mapping the
  routing needs *already exists* (`projects` + `project_repos.path_prefix`, longest-prefix
  `find_by_remote`) and is simply not wired to teams.

- **Impact**: Lifts the hard ceiling on the product's core loop. Review capacity goes from O(1
  operator) to O(headcount) — a 200-engineer org with 20 maintainers is a 20x throughput change, and
  it is the difference between "the backlog drains" and "canon rots because one person can't keep
  up", which is the failure mode that kills a knowledge product. Second-order: correct `team_id`
  stamping makes the shipped `team` visibility tier and every per-team RLS clause
  (`0001_init.sql:259`, `0017_document_layer.sql:123`) mean something real for the first time; and
  per-team knowledge health turns Brainiac from an operator tool into a management surface a VP buys.

- **Fix sketch**:
  1. **Directory write plane (smallest slice that proves it).** `POST /v1/org/teams`,
     `POST /v1/org/teams/{id}/members`, `PATCH .../members/{user_id}` (role), thin wrappers over the
     existing `brainiac_store::orgs::upsert_team`/`upsert_user`/`upsert_member`; authorize with
     `is_any_maintainer` + an `org:admin` scope added to `auth::SCOPES` (note the lesson recorded at
     `auth.rs:85-96` — add the scope to the vocabulary in the same commit that enforces it).
     Console gains a Directory page beside the read-only `org_users` payload.
  2. **Invites.** Reuse the device-flow shape already built in `onboard.rs`
     (`onboard_requests`, user/device code split, `/v1/onboard/requests/{id}/approve`) rather than
     inventing a second pattern: an org-join request is that table with a different payload. The one
     genuinely new branch is in `identities::provision` — today a verified Google identity with no
     org *always mints a fresh org* (`identities.rs:73-84`), so an invited colleague would land in
     their own empty project. Add `accept_invite`: when a pending invite matches the verified email,
     bind the identity to the inviting org and its team instead of creating one.
  3. **Ownership edges.** `team_projects(org_id, team_id, project_id, path_prefix)`. Replace
     `principal.team_ids.first().copied()` at `http.rs:599,653` with resolution through
     `brainiac_store::projects::find_by_remote` (already longest-prefix aware for monorepos), fed by
     the shipped repo-fingerprint auto-attribution on the MCP path. Fall back to the current
     behaviour when no edge matches, so nothing regresses.
  4. **Routing + accountability.** `?assigned=me|team` on `/v1/reviews/promotions`; per-team queue
     depth and oldest-pending age in `analytics`/`knowledge-health`; an escalation sweep registered
     in `sweep_schedules` (0018) that flags promotions past SLA.
  5. **Proof.** `console_pg.rs` already builds three principals with distinct roles across two teams —
     extend it to assert a memory ingested against `github.com/acme/mono` + `apps/api` lands in the
     api team's queue, is absent from the web team's, and that only the api maintainer can approve it.

## 2. Identity above org — multi-membership, directory sync, and knowledge that survives departure

- **Severity**: High
- **Category**: identity-network-effect
- **File**: `migrations/0022_identities.sql:20-28` (`PRIMARY KEY (provider, subject)` — one identity,
  one org, forever) and `crates/brainiac-store/src/identities.rs:31-43` (`find` returns at most one org).
  Anchors: `migrations/0001_init.sql:20-21` (`teams.idp_group_id`, `UNIQUE (org_id, idp_group_id)`)
  and `:27` (`users.idp_subject`) — both declared in v0 and **read or written by zero lines of code**;
  `identities.rs:1` ("SCIM reconciliation later"); `console.rs:1261` (`is_any_maintainer`);
  `migrations/0001_init.sql:70-73` (`owner_user_id`, `visibility='private'`).

- **Scenario**: A staff engineer signs in once and sees a workspace switcher: her employer's org, the
  two client orgs she contracts for, and her own personal project. Her employer's directory is not
  hand-typed — a sync job reconciles Google Workspace groups (or a GitHub org's teams) into
  `teams`/`team_members` nightly, so `idp_group_id` finally carries the group it was designed for and
  GitHub team maintainers become Brainiac maintainers automatically. When she leaves, that is an
  *event*: her tokens are revoked, the canonical memories she owned are reassigned to her team, every
  memory whose only human provenance was hers is pushed into re-verification, and a successor signs
  off — instead of today's outcome, where a departed person's row stays a valid maintainer forever and
  her private memories become permanently unreadable ballast. And the knowledge that was genuinely
  *hers* — her own pitfalls and howtos — travels with her identity to the next org, arriving as
  candidates that must pass that org's promotion gate. Engineers accumulate a permanent professional
  memory across employers; orgs adopt Brainiac because a senior hire lands productive in week one.

- **Root cause**: The identity plane models a person as a *property of one org* rather than a durable
  node with memberships. `identities`' PK makes "one Google account = one org" a database-level
  invariant, `users.org_id` is `NOT NULL`, and `find` is typed to a single `(org_id, user_id)`. So
  three whole classes of relationship are unrepresentable: a person in two orgs (contractor, agency,
  OSS maintainer, acquisition), an org composed of orgs (subsidiary, vendor, guest), and a person who
  *leaves*. There is no deactivation of any kind — no `users.status`, no `left_at`; `is_maintainer`
  and `is_any_maintainer` query `team_members` with no liveness predicate, so authority never lapses.
  And the IdP columns that would let the directory mirror the systems a company already runs have sat
  unused since v0, which means every org's structure must be hand-entered — the very thing that makes
  finding #1's routing model too expensive to maintain at scale. The upside is that 0022's own comment
  (`:16-19`) already names the correct target shape ("one membership row per identity per org"), so
  this is the design's stated destination, not a reversal.

- **Impact**: Converts a single-tenant internal tool into a plane with a network effect. Concretely:
  the addressable user goes from "one account per org" to every engineer who works across boundaries
  (contractors, agencies, consultancies, post-acquisition orgs — a large fraction of senior
  engineering time), each of whom becomes a *carrier* of the product into the next org they join.
  Directory sync removes the per-org setup cost that otherwise caps rollout at the teams someone was
  willing to type in. Departure handling removes a compounding correctness liability — in a 200-person
  org with 15% annual attrition, today's design accrues ~30 phantom maintainers and ~30 orphaned
  private corpora per year, forever.

- **Fix sketch** (deliberately *not* credential attenuation or cross-org norm sharing — siblings own
  those; this is the membership/directory model itself):
  1. **Smallest slice that proves it.** Relax the PK: `(provider, subject)` → `(provider, subject,
     org_id)` with a non-unique index on `(provider, subject)`. `identities::find` returns
     `Vec<(Uuid, Uuid)>`; `provision` keeps its exact current behaviour when the list is empty (still
     the one-org-on-first-sight rule) and gains an explicit "join this org" path. `/v1/provision`
     returns a workspace list; the console gets a switcher. Crucially `Principal` is unchanged — one
     org per request — so `scoped_tx`, every RLS policy, and the tenancy story are untouched, which
     is what makes this cheap. Prove with a pg test: one identity, two orgs, each `scoped_tx` sees
     only its own memories and `is_maintainer` answers per-org.
  2. **Directory sync.** A `directory_sync` job in `brainiac-pipeline`, same worker/job pattern as
     `library_sweep` and the `sweep_schedules` sweeps (0018), reconciling an external group source
     into `teams.idp_group_id` (the `UNIQUE (org_id, idp_group_id)` constraint is already the right
     idempotency key) and `users.idp_subject`; membership and `role='maintainer'` derive from the
     source. Idempotent-upsert semantics already exist in `orgs.rs`; this is what those functions'
     doc comment was written for.
  3. **Org federation edges.** `org_relations(parent_org_id, child_org_id, kind)` with
     `kind ∈ {subsidiary, vendor, guest}` as *directory* structure only — it decides who may be
     invited and what appears in a switcher, not what any credential can read.
  4. **Departure as an event.** `users.status` + `left_at`; add the liveness predicate to
     `is_maintainer`/`is_any_maintainer`; an offboard endpoint that revokes tokens via the existing
     `tokens::revoke`, reassigns `owner_user_id`/team-owned canon, and pushes single-provenance
     memories through the shipped `memory_reverify` + TTL machinery for successor sign-off.
  5. **The portable pack (the flywheel).** `visibility='private'` memories are already keyed by
     `owner_user_id`, so an identity-scoped export/import moves exactly the rows the person owns —
     nothing else can leak by construction — and they land in the new org as `candidate`, subject to
     that org's promotion gate. Ship after (1) and (4); it is the payoff, not the starting point.
