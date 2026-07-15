# Visibility: the contractor tier and the observer role

Status: **design** (2026-07-15). Implementation is deliberately not started —
this document exists so that when it starts, it starts from a contract, and so
the parts that need *no* implementation stop being listed as gaps.

The UAT surfaced two personas the three-tier model (`private | team | org`)
cannot express. They sound similar and are structurally opposite, which is why
lumping them as "the 4th visibility tier" stalled: one needs **less than
membership grants**, the other needs **breadth without membership**.

## 0. The design decision that shapes everything

**Neither persona becomes a new `visibility` value on memories.**

The memory-side ladder stays three rungs. A row's visibility answers "who is
this knowledge *for*?" — and both personas are questions about who the
*reader* is, not about the knowledge. Adding a `restricted` rung to the ladder
would force every author (human and extractor) to make a sensitivity decision
they have no basis to make, would fork the publish rule (D5), and would still
not express "this contractor may see recent work only."

Both personas are **principal-side attributes**, enforced where every other
read rule lives: the RLS predicate.

## 1. The observer role — already expressible, zero schema

**Persona**: a leader, staff engineer, or auditor who needs cross-team
*awareness* — the Knowledge Health report, the entity graph, org-wide
knowledge — without inheriting any team's private history.

**Design**: an observer is **a user with org membership and no team
memberships**, holding a token scoped `read` (+ `kb:read` where the KB layer
is on).

That is not a workaround; it is the model working as built:

- `memories_read` (0001) resolves a teamless principal to org-visible rows
  only — the same clause every user goes through, no new predicate to audit.
- Knowledge Health is already **org-true**: `compute_health_core` computes the
  org's real totals independent of the viewer's vantage, so an observer's
  dashboard shows the true score while their *browsing* stays org-visible.
  The count of team-private memories appears in aggregates (`siloed`,
  `team_only`); their content never does. That aggregate exposure is the
  entire point of the role and is already what the report shows every org
  member today.
- Pages: an observer sees org-visible pages (`documents_read` mirrors the
  memory policy). Team pages stay invisible.

**What implementation remains**: provisioning affordance only — a documented
"observer" preset when issuing tokens/memberships (console UI + docs). The
eval side is done (2026-07-15): `user-observer` is a fixture user, the leak
suite pins the posture (`leak-016..019`: one team-visible target per team plus
a private one, zero tolerance), and `asking_as.team` is optional with the
linter refusing a teamless asker declared on a teamed user. No migration, no
policy change.

## 2. The contractor tier — real schema work, gated by eval

**Persona**: someone working *inside* a team whose access should be narrower
than the team's full history — typically time-scoped ("what the team produced
since they joined") plus explicit shares ("and these three runbooks").

Today `team_members` is binary: membership grants the team's entire past.

### 2.1 Schema

```sql
ALTER TABLE team_members
    ADD COLUMN access text NOT NULL DEFAULT 'member'
        CHECK (access IN ('member', 'restricted')),
    ADD COLUMN access_since timestamptz;  -- required when restricted

-- Explicit grants that pierce the time fence, one memory at a time.
CREATE TABLE memory_shares (
    memory_id  uuid NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL,
    org_id     uuid NOT NULL,
    granted_by uuid NOT NULL,          -- a named human, always
    granted_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (memory_id, user_id)
);
```

### 2.2 The predicate

The `team` arm of `memories_read` becomes (sketch):

```sql
visibility = 'team' AND team_id IN (
    SELECT tm.team_id FROM team_members tm
    WHERE tm.user_id = current_setting('app.user_id')::uuid
      AND (tm.access = 'member' OR m.created_at >= tm.access_since)
)
OR EXISTS (SELECT 1 FROM memory_shares s
           WHERE s.memory_id = m.id
             AND s.user_id = current_setting('app.user_id')::uuid)
```

Time fence on `created_at`, not `updated_at`: a pre-existing belief edited
yesterday is still the team's past. Deliberately **no** per-kind or per-entity
carve-outs in v1 — every additional predicate clause is attack surface, and
the sharp lesson of this codebase is that RLS subtleties fail silently.

### 2.3 The page is the leak vector — the rule that makes this safe

A composed team page is a **projection over the team's full history**. Serving
it to a restricted member leaks every pre-fence memory through the
composition. So:

> **A restricted member does not see team pages at all.** Pages follow the
> most-restricted memory that may compose into them, and a team page may
> contain anything in the team's past by construction.

This is one clause in `documents_read` (deny `team` documents to restricted
members), and it is non-negotiable in v1. A later increment may add
fence-aware recomposition (a page variant composed only from post-fence
memories — the same visibility-capped-principal trick org pages use), but that
is an optimization on top of a safe default, never a prerequisite for one.
Same reasoning applies to `document_reads` aggregation, MCP `doc_get` /
`doc_search`, digests (time-windowed but still full-team sources), and the
retrieval graph expansion: **every surface that summarizes team knowledge
inherits the deny**.

### 2.4 What does NOT change

- The publish rule (D5): external publish remains org-visible-only; restricted
  members change nothing about what leaves the org.
- The extractor and review gate: a restricted member's contributions enter as
  their own team-visible memories like anyone's. Restriction is a read
  posture, not a write posture.
- `Visibility` enum, fixture memory schema, compose firewalls.

### 2.5 The eval gate (build this FIRST, like everything else here)

The leak suite gains a `user-pay-contractor` fixture asker (restricted,
`access_since` mid-corpus) with gold expectations:

1. pre-fence team memory: **invisible** (leak = build failure),
2. post-fence team memory: visible,
3. explicitly shared pre-fence memory: visible,
4. team page: **not found** (the §2.3 rule),
5. org + own-private rows: unchanged.

The RLS matrix test in `store_pg` gains the same persona. Implementation may
not begin until these fixtures exist and fail — red first, then the migration.

### 2.6 Rollout

1. Fixtures + failing eval (above).
2. Migration (additive columns + `memory_shares` + policy replace; explicit
   GRANTs — the 0017 lesson).
3. `documents_read` deny + MCP/docs surface tests.
4. Provisioning UI: mark a membership restricted at invite time; share flow
   writes `memory_shares` with the granting human's id.
5. Only then: mention it in public docs.

## 3. Sequencing note

The contractor tier touches the same policy file as any concurrent RLS work
(`memories_read` is replaced, not amended). It should be built in a quiet lane
— not alongside another session's schema work — and the migration should be
the only thing in its commit that touches 0001-era policies.
