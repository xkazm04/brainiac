# Moonshot scan — onboarding

> Total: 2
> Files read: 14

## 1. Flip the whitelist: pairing that provisions its own home and spreads by developer quorum

- **Severity**: Critical
- **Category**: bottom-up-adoption
- **File**: `crates/brainiac-server/src/onboard.rs:451` (the whitelist 409 in `request_approve`) · `crates/brainiac-store/src/onboard.rs:91` (`list_pending`, deployment-wide, org-less) · `crates/brainiac-store/src/projects.rs:165` (`find_by_remote`) · `crates/brainiac-server/src/provision.rs:135` (`identities::provision`) · `migrations/0034_projects_onboarding.sql:34` (`UNIQUE (org_id, remote)`)

- **Scenario**: A developer at a 400-engineer company hears about Brainiac from a teammate, runs
  `curl $API/v1/onboard/skill | claude`, and pairs the repo they are already working in. Nobody
  registered that repo. Nobody had to. The pairing lands in a **provisional project** derived from
  the remote's owner segment (`github.com/acme/payments` → project `payments`), created inside the
  org their verified email domain routes them to — `acme.com` already has three paired repos, so
  they join it; if no org claims the domain, one is provisioned for them exactly as
  `/v1/provision` does today for a Google sign-in. The provisional project is born
  `isolated = true` (migration 0040), so whatever it accumulates is invisible to the rest of the
  org until someone adopts it: a developer can never contaminate org-wide memory by pairing.
  Their `user_code` doubles as a **shareable invite** — `#brainiac join AZKQ-4RT7` in the team
  channel — and each teammate who pairs the same remote co-signs the same provisional project
  rather than opening a fresh orphan request. At the **third** distinct co-signer the project
  auto-nominates itself: the org's admins see one card — *"payments · 3 developers · 118 candidate
  memories · 6 pitfalls surfaced · adopt into the org?"* — and adoption is a single click that
  flips `isolated` off and promotes the provisional row into a real `project_repos` entry. The
  admin's job stops being *gatekeeping onboarding* and becomes *ratifying traction*. Brainiac
  spreads the way Slack and Figma spread: the org follows the developers.

- **Root cause**: `request_approve` refuses with 409 unless `projects::find_by_remote` already
  matches — the whitelist is a **precondition**, so the causal chain runs admin → project → repo
  → developer. Every new repo requires a human who already knows about Brainiac, already has the
  `admin` scope, and already knows the repo exists. That is a strictly top-down funnel, and it
  caps the product at the rate one operator can enumerate their company's repos. Worse, the data
  model has no way to even *route* an unclaimed pairing: `onboard_requests` rows begin org-less
  (0034), `list_pending(pool)` takes no `org_id` at all, and matching is done per-caller-org in
  the handler loop (`onboard.rs:395`). So an unmatched request is not just refused — it is
  addressed to nobody, visible to every admin of every org on the deployment, and it dies in 15
  minutes. The whole viral vector is missing because the schema never asked "which org does this
  repo *want* to belong to?"

- **Impact**: Converts onboarding from O(1 admin's attention) to O(developers who try it). Today
  the realistic ceiling for a company is the handful of repos one operator registers in a sitting
  — call it 5–20. With domain routing plus quorum graduation, every engineer is an onboarding
  path, and each successful pairing produces a shareable invite code that recruits the next
  one: 400 engineers × even a 5% try-rate seeds 20 provisional projects in week one with zero
  admin involvement, and the admin's first contact with the product is a card showing 118
  memories already accumulated instead of an empty form asking them to type remote URLs. It also
  removes the single most common cold-start failure the current skill has to hand-wave —
  SKILL.md's *"If the console says the repo isn't registered, an admin must add it under a project
  first ... report it and stop"* — which is a dead end at exactly the moment a new user is most
  motivated. Quorum-gated graduation is the safety story: nothing enters org-wide memory on one
  person's say-so, so the growth mechanic and the governance thesis reinforce rather than fight.

- **Fix sketch**:
  1. **Schema (smallest slice).** Add `orgs.domain text UNIQUE NULL` and, to `onboard_requests`,
     `provisional boolean NOT NULL DEFAULT false` plus `cosigned_by uuid[]`. Add
     `projects.provisional boolean` and `project_repos.provisional boolean`. Add
     `orgs.domain` claim on first `identities::provision` (derive from `email`'s domain, skip
     free-mail domains via a small denylist constant next to `DEVICE_SCOPES`).
  2. **Routing.** New `brainiac_store::orgs::find_by_domain(conn, domain) -> Option<Uuid>` (one
     indexed lookup). New `brainiac_store::onboard::cosign(pool, remote, path, user_id)` — an
     `UPDATE ... WHERE remote = $1 AND provisional AND status = 'approved'` returning the row, so
     two teammates racing land on one project, exactly the single-shot discipline `claim()`
     already uses.
  3. **The escalation path in `request_approve`.** Where it currently 409s, branch instead: if the
     caller's org has no whitelist match *and* the request carries a verified identity, call a new
     `onboard::self_provision` that (a) resolves the org via `find_by_domain` or falls through to
     `identities::provision` on `state.admin_pool` (the existing comment in `provision.rs:134`
     already documents exactly why the owner pool is required here), (b) `projects::create` named
     from `normalize_remote`'s owner/name split, (c) `projects::set_isolated(..., true)`,
     (d) `projects::add_repo` with `provisional = true`. The `ONBOARD_SCOPES` constant stays
     untouched — a provisional key is still `read,write`, never `admin`.
  4. **Identity on the unauthenticated side.** `OnboardStartBody` gains an optional
     `verification` field; when absent the poll returns a new
     `status: "needs_identity"` carrying a console URL — the developer signs in with Google
     (the shipped signup flow at `console/app/signup`), the console posts the verified
     subject onto the pending request, and the next poll self-provisions. This is the only
     genuinely new UX surface, and it reuses the console's existing Firebase verification
     wholesale.
  5. **Quorum + adoption.** A `GET /v1/onboard/nominations` (admin) listing provisional projects
     with `array_length(cosigned_by, 1) >= 3` and their memory counts, and a
     `POST /v1/projects/{id}/adopt` that flips both `provisional` flags and calls
     `set_isolated(false)`. Adoption is deliberately *not* automatic — the graduation gate is a
     named human signing, the same principle `approve` already encodes.
  6. **Fix the org-blind query while you are in there.** `list_pending` should take an `org_id`
     and filter by domain-routed or whitelist-matched org, so the console stops showing one org's
     admins another org's pending requests (visible today at `onboard.rs:390`).

  First slice that proves it end to end: steps 1–3 with `verification` hardcoded to the console's
  own identity, plus an integration test in `crates/brainiac-server/tests/onboard_pg.rs` asserting
  that pairing an *unregistered* remote yields a claimed key scoped to a fresh isolated project —
  one test that today would necessarily 409.

## 2. Make the pairing a knowledge handshake: first-hour value from the tacit config on the developer's machine

- **Severity**: High
- **Category**: cold-start-value
- **File**: `docs/skills/brainiac-onboard/SKILL.md:116` (the honest admission: *"any hit count, including 0 on a fresh org"*) · `crates/brainiac-server/src/onboard.rs:361` (the claim response, currently token-only) · `crates/brainiac-pipeline/src/manual.rs:1` (the verbatim no-LLM path for pre-distilled statements) · `crates/brainiac-pipeline/src/divergence.rs:1` (`DIVERGENCE_SYSTEM_PROMPT_V1`) · `crates/brainiac-pipeline/src/okf_ingest.rs:1` (the "someone else's artifacts as a witness" precedent)

- **Scenario**: The pairing handshake stops being a credential exchange and becomes a knowledge
  exchange. Today the flow is one-directional: Brainiac hands over a token and the skill *writes
  into* the developer's `CLAUDE.md`. Invert the arrow. The claim response carries a **bootstrap
  contract** alongside the token — a manifest of local artifact classes to harvest and the
  visibility tier each lands in. The skill walks it and posts what it finds as `manual` sources:
  the developer's own `~/.claude/CLAUDE.md` and personal memory, `.claude/settings.local.json`
  gotchas, uncommitted `.cursor/rules`, machine-local skills, the "how I actually run this
  locally" knowledge that lives in one person's head and nowhere in version control. Nine minutes
  after pairing, the developer runs their first `memory_search` and gets **their own hard-won
  conventions back, typed and attributed** — not an empty result set. That alone converts the
  first session. Then the compounding part: developer #2 pairs the same remote a week later, and
  their harvested tacit config **disagrees** with #1's. `divergence.rs` — which already clusters
  cross-team statements on a shared canonical entity and adjudicates them into a named practice
  with a recommended standard — fires on the pair. Day-one deliverable, unprompted: *"Two
  developers on payments disagree about how migrations are applied locally. Recommended standard:
  …"* That is a finding neither developer could have produced, it exists only because both
  onboarded, and it gets sharper with every additional pairing. **Onboarding becomes the product's
  first demo of its own thesis**, and the thesis is demonstrated by the act of adoption itself.

- **Root cause**: The onboarding flow's success criterion is *"a key that authenticates"*, and
  it is measured that way — SKILL.md's Step 2 verification explicitly accepts zero hits as
  success. That is a credential test masquerading as a value test. An org-level memory server's
  actual first-run failure mode is not a bad token, it is an empty index: the developer connects,
  searches, gets nothing, concludes the thing is a stub, and never returns. The pairing moment is
  the one instant where a human is attentive, motivated, and sitting at a machine full of exactly
  the knowledge the product wants — and the current design harvests none of it and asks for none
  of it. There is also no mechanism anywhere that makes the *second* pairing worth more than the
  first; every pairing is an independent, identical transaction, so onboarding contributes nothing
  to the compounding asset.

- **Impact**: Time-to-first-useful-retrieval drops from *"after someone ingests sources, days
  later, if an operator remembers"* to **inside the pairing session**. Concretely: a typical
  engineer's `CLAUDE.md` plus local agent config yields on the order of 10–40 distilled
  statements, and `manual.rs`'s verbatim path means that costs **zero extraction LLM calls** —
  the statement *is* the memory. Ten paired developers therefore seed 100–400 candidate memories
  before any source ingestion is configured at all, from knowledge that is currently unrecorded
  and lost entirely when a person changes teams. And the divergence dividend is superlinear: with
  n paired developers on a repo there are n(n−1)/2 comparison pairs, so the platform-lead value
  of onboarding grows quadratically in adoption — the strongest possible argument for the viral
  mechanic in finding 1, and the reason these two should ship in that order.

  **Boundary note**: this is deliberately *not* repo-as-ground-truth ingestion (covered by a
  sibling). The harvest target here is the **uncommitted, machine-local, human-tacit** layer —
  the stuff that is in no repo and has no other capture path — and the deliverable is
  cross-developer divergence, not repo fidelity.

- **Fix sketch**:
  1. **Extend the claim response, nothing else, first.** Add
     `bootstrap: Option<BootstrapPlan>` to `OnboardPollResponse` (`onboard.rs:129`) — a serialized
     list of `{ class, glob_hints, kind_hint, visibility, max_items }` entries. Serve it from a
     new `const BOOTSTRAP_PLAN_V1` next to `ONBOARD_SCOPES` so the server owns the policy and the
     skill stays a dumb executor. This is a purely additive DTO change; every existing caller
     ignores it.
  2. **A new Step 1.5 in SKILL.md** that walks the plan and posts each finding through
     `POST /v1/memories` with the kind hint encoded via the existing `manual` wire format
     (`ManualSource` / `HINT_MARKER` in `manual.rs`) — so the statements skip extraction, land as
     candidates, and hit the promotion review gate like everything else. Governance is unchanged:
     harvested tacit config is a *witness*, exactly the stance `okf_ingest.rs` takes toward
     someone else's wiki, and a named human still signs what the org believes.
  3. **Attribution.** Stamp each bootstrap source with the pairing's `label` and `approved_by` so
     provenance names the machine and the human — `memory_provenance` (already shipped) then
     answers *"who on this team believes this?"* for free.
  4. **The divergence trigger.** After a claim on a remote that already has a prior claimed
     pairing, enqueue a scoped divergence sweep over the two pairings' bootstrap sources.
     `divergence.rs` already clusters on canonical entity anchors and only stores
     `divergence: true` verdicts, so the conservative behavior is inherited; the only new code is
     the narrow candidate query (memories whose source label differs, sharing an entity anchor)
     and the enqueue call in the claim branch of `onboard_poll` (`onboard.rs:352`, right after
     `tokens::create`).
  5. **Close the loop in the skill's wrap-up.** Replace Step 2's `{ok: true, hits: 0}` check with
     a real assertion: search for a term from the harvest and require a hit. Onboarding now
     *fails* if the org's memory cannot answer a question about the repo the developer just
     paired — which is the honest bar.

  First slice: steps 1, 2, 5 with a single artifact class (`CLAUDE.md`-family files) and no
  divergence sweep. That alone turns Step 2 from a token test into a value test and is provable
  with one manual run against a fresh org.
