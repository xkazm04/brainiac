> Context: Server: Governance Console API
> Total: 5 (Critical: 0, High: 2, Medium: 2, Low: 1)

## 1. Knowledge Health report & persisted snapshots are viewer-RLS-scoped, silently hiding cross-team contradictions
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: rls-scoping / metric-integrity
- **File**: crates/brainiac-server/src/console.rs:1638-1746 (compute_health_core) + :1755-1779 (GET) + :1988-2015 (POST snapshot)
- **Scenario**: A payments-team member (e.g. the `tok_pay_lead` fixture, `teams=[payments]`) calls `GET /v1/analytics/knowledge-health`, then `POST …/snapshot`. Both handlers call `compute_health_core(&mut tx, None)` under the caller's `scoped_tx`, so every table is filtered to that caller's *visible slice*, not the org. The contradiction query (:1665-1678) `JOIN memories ma … JOIN memories mb` is an INNER join: for a cross-team contradiction whose *other* side is a team-only memory the caller can't read, the whole row drops from both `open` and `cross_team` counts. Result: `open_contradictions` and the flagship `cross_team_contradictions` are undercounted, the consistency pillar and composite `score` are inflated — precisely for the conflicts "no individual team can see," which is the product's stated value. Corroboration: the sibling `analytics` handler counts `open_contradictions` with no memories join (:1216-1221), so the same caller gets a *higher* open count there than in knowledge-health — an internal contradiction.
- **Root cause**: The endpoints only require `read`/`write` scope, never org-wide visibility, but the design (comment :1631-1637) assumes the caller is "a leader" who sees everything. `org_filter=None` delegates scoping to RLS, which for a normal member is their slice, not org truth. The scheduled `snapshot_all_orgs` correctly passes `Some(org)` on the admin pool (org-true), so the two writers disagree.
- **Impact**: The headline leadership metric is wrong and viewer-dependent (two leaders in one org see different scores). Worse, `POST /snapshot` (write scope) *persists* the distorted, member-scoped numbers into `knowledge_health_snapshots` under `principal.org_id`, and the trend read (:1925-1929) mixes them with the org-true rows written by the sweep — so the tracked trend line a VP watches for decisions blends two incompatible measures and can trend "up" purely because a narrower-visibility member took the last snapshot.
- **Fix sketch**: Require org-wide visibility (admin scope) for both the report and the snapshot, and/or compute org-true totals via `compute_health_core(tx, Some(principal.org_id))` on the admin pool for these org-level signals; at minimum use `LEFT JOIN memories` in the contra query so an invisible side does not delete the whole contradiction row. Never persist a member-scoped snapshot into shared org history.

## 2. Promotion approve/reject is a non-atomic TOCTOU — the double-review guard is bypassable under concurrency
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: race-condition / toctou
- **File**: crates/brainiac-server/src/console.rs:138-161 (actionable_promotion) + :190-199 (UPDATE)
- **Scenario**: `actionable_promotion` selects the promotion `WHERE … policy_decision='needs_review' AND reviewed_at IS NULL` with **no `FOR UPDATE`**, then `review_promotion` issues `UPDATE promotions SET … WHERE id = $1` — the WHERE does **not** re-assert `reviewed_at IS NULL`. Two concurrent approve (or approve+reject) requests on the same promotion both pass the SELECT (READ COMMITTED: both see it pending), then serialize on the row lock at UPDATE; the second simply overwrites reviewer/decision and re-runs `set_memory_status`. The sequential-only test at :137-145 asserts a *second* call returns 404, but that only holds because the first already committed — it does not cover the concurrent window.
- **Root cause**: The actionability check and the state transition are separated by an unlocked read; the mutating UPDATE trusts the earlier snapshot instead of re-proving the precondition atomically.
- **Impact**: Two maintainers (or a double-submitting client) can both "decide" one promotion; the reviewer_id/reviewed_at/decision recorded is last-writer-wins, and `set_memory_status` runs twice. With the two-pending-promotions-on-one-memory shape the tests themselves create (`promo_approve` + `promo_reject` on `raw_mem`), a concurrent approve+reject leaves the memory in a nondeterministic status while both promotions show a human decision. Governance audit integrity is lost.
- **Fix sketch**: Add `FOR UPDATE` to `actionable_promotion`'s SELECT so the second txn blocks then re-reads the now-reviewed row, or make the UPDATE self-guarding — `UPDATE … WHERE id=$1 AND reviewed_at IS NULL` and treat `rows_affected == 0` as 404/409, only calling `set_memory_status` when the update actually landed.

## 3. Contradiction `coexist`/`dismiss` resolution has no maintainer gate — any write principal can lift the open-contradiction withhold
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: authorization / governance
- **File**: crates/brainiac-server/src/console.rs:428-527 (resolve_contradiction; branches :498-499 vs the gated supersede :451-497)
- **Scenario**: `resolve_contradiction` requires only `auth_of("write")` and that the `status='open'` row is visible under RLS (:437-447). The `supersede` branch additionally verifies `is_maintainer` of the losing memory's team (:475-481), but `coexist` (:498) and `dismiss` (:499) fall straight through to the terminal UPDATE (:509-521) with **no maintainer check and no proof the caller can see either memory**. Since contradictions are org-scoped metadata, a plain team member can `dismiss` any open contradiction in the org — including a cross-team one whose contents they cannot read.
- **Root cause**: The module's governance rule (header :8-12) enumerates only "approve/reject and contradiction supersede" as maintainer-gated, so the equally-consequential terminal actions were left at the `write`-scope bar.
- **Impact**: Per the analytics comment (:1128-1133), an *open* contradiction withholds both sides from serving; resolving it (any terminal status) releases them. A non-maintainer with no visibility into one side can therefore dismiss a real cross-team conflict and push both contradictory memories back into agent retrieval — defeating the withhold-by-default safety the system relies on, with zero audit of "was this person allowed to decide."
- **Fix sketch**: Gate `coexist`/`dismiss` the same way as supersede — require `is_maintainer` of (at least one of) the involved memories' teams, resolving each `team_id` under the caller's RLS so an invisible side yields 404, not a silent authorization bypass.

## 4. `GET /v1/memories?status=` casts unvalidated input to the enum → 500 instead of 400
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: input-validation / edge-case
- **File**: crates/brainiac-server/src/console.rs:2570-2622 (memories_list; FILTER :2588-2593, bind :2605)
- **Scenario**: The status filter is applied as `AND ($2::text IS NULL OR m.status = $2::memory_status)` with `p.status` bound verbatim and never validated. A typo or adversarial value (`?status=foobar`, `?status=Open`) makes Postgres evaluate `'foobar'::memory_status`, which raises `invalid input value for enum memory_status`; the error flows through `map_err(internal)` as a 500. `list_contradictions` (:321-330) shows the intended pattern — it validates its status against an allow-list and returns 400.
- **Root cause**: Client-supplied query params are trusted directly into an enum cast; the handler assumes the console only ever sends valid statuses.
- **Impact**: Any external caller (or a stale console build) can turn a routine archive listing into a 5xx, and the failure reads as a server fault rather than a client input error — noisy alerts, no actionable message, and a trivial way to make the endpoint error on demand.
- **Fix sketch**: Validate `p.status` against the known `memory_status` variants up front and return `400` for anything else (mirror `list_contradictions`), or parse it via `MemoryStatus::parse` and bind the canonical text; likewise consider validating `kind`.

## 5. Duplicated 9-column Knowledge-Health snapshot INSERT across the POST handler and the sweep
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-server/src/console.rs:1995-2014 (knowledge_health_snapshot) + :2038-2054 (snapshot_all_orgs)
- **Scenario**: The exact same `INSERT INTO knowledge_health_snapshots (org_id, score, consistency, currency, liquidity, governance, cross_team_contradictions, stale_beliefs, total_memories) VALUES ($1..$9)` with the same nine `HealthCore` binds is written twice — the code even flags it ("Mirrors the POST handler's insert", :2027). They differ only in `RETURNING captured_at`.
- **Root cause**: Two callers (per-org POST vs. cross-org sweep) each grew their own copy instead of sharing one writer; there is no compiler pressure keeping the two column/bind lists in lockstep.
- **Impact**: Adding a column to the snapshot (e.g. a new pillar) requires editing both sites; miss one and that path silently records a stale/zeroed history row with no build error — a quiet divergence in the very trend data leadership tracks. Also low-grade maintenance cost and a subtle inconsistency (one has `RETURNING`, one doesn't).
- **Fix sketch**: Extract `async fn insert_health_snapshot(exec, org: Uuid, c: &HealthCore) -> …` that owns the column list and binds; have the POST handler call it (returning `captured_at`) and the sweep loop reuse it. While there, the identical `by_status` histogram (`analytics` :1173 / `observatory` :1357) and the contradiction histogram (`list_contradictions` :360 / `observatory` :1424) are candidates for the same treatment.
