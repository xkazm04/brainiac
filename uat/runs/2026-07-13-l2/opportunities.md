# Feature opportunities — resolving the gaps the trial found

Derived from the L1 sweep (`../2026-07-13-l1/`) + the live L2 (`report.md`, `arms.md`). Four fixes
have shipped and are validated (governance floor, mid-task descriptions, H8 provenance,
open-contradiction serving). What remains is ranked by **net-value leverage** — how much of the
`harmful-as-shaped` → `adopt` distance each one closes — against effort. Every item cites the finding
and the code locus so it's actionable, not aspirational.

---

## P0 — the `harmful-as-shaped` core: make governance health OBSERVABLE

The trial's central negative finding (relay `promotion-queue-backlog`, H5/H6): auto-capture
industrializes candidate memories, review is rate-limited by two maintainers' calendars, and when
they quietly stop, `memory_search` kept serving the backlog as truth **and nothing went red**. The
buyer (Dana) funds it, can't see it (RLS-truncated analytics), and can't act on it (no maintainer
role). You cannot fix abandonment you can't see. These make it visible.

| # | Feature | Evidence | Effort | Locus |
|---|---|---|---|---|
| **P0.1 ✅ SHIPPED** | **Review-velocity metrics in `/v1/analytics`**: `reviewed_last_7d/30d`, `median_time_to_review_secs` (the SLO number), `rubber_stamp_rate` (share of a reviewer's decisions taken <5s after their previous one — a burst/rubber-stamp proxy via `lag()`, no schema change). The abandonment curve is now quantified. Tested in `console_pg.rs`. | H5: "no dwell-time captured, rubber-stamping is invisible" (`console.rs:165-178`) | **S** (done) | `console.rs` analytics |
| **P0.2** | **Org-level governance-health view that survives RLS** — queue depth, oldest-pending age, velocity computed from org-scoped `promotions`/`contradictions` metadata (counts + ages, not content, so no leak), visible to a teamless principal. Resolves the buyer's "invoice minus a Slack channel." | Dana: sees `canonical:0` next to `47 pending`; the one metric she wants is hidden | **S–M** | `console.rs` analytics, a governance/observability role |
| **P0.3** | **Raw-memory TTL sweep**: a worker that auto-`rejects` (or archives) `raw` memories older than N days that never reached candidate — so the ungoverned backlog cannot grow unbounded and silently. Bounds the H1 blast radius by construction. | H5: "queue grows monotonically; capture unthrottled, review human-rate-limited" | **M** | `brainiac-pipeline` worker + a `queue` sweep |
| **P0.4** | **SLO breach alerting**: when oldest-pending age crosses their own 48h SLO (ARCHITECTURE §7), emit a signal (webhook/metric). Turns silent rot into a page. | H5: "median promotion review < 48h or the flywheel dies" | **M** | worker + metrics |

## P0 — close the contradiction-precision dependency my own fix created

The open-contradiction serving fix (this run) **withholds** both sides of an open contradiction. That
is safe against poison, but it means a **false-positive** contradiction now *hides real knowledge*
until a human dismisses it. The fixture deliberately ships 8/12 contradictions as negatives, so
precision is load-bearing now in a way it wasn't.

| # | Feature | Evidence | Effort | Locus |
|---|---|---|---|---|
| **P0.5 ✅ SHIPPED** | Dismissal already existed and is cheap (`resolve` accepts `dismiss`, needs only `write` scope, not maintainer) and **auto-restores serving** (the withhold filter keys on `status='open'`, so dismiss un-hides — tested). The gap was *visibility*: shipped `contradiction_dismiss_rate` in `/v1/analytics` so an over-eager detector (now suppressing knowledge, not just noise) is observable and tunable. | contradiction queue is 8/12 negatives; withhold-by-default raises the stakes | **S** (done) | `console.rs` analytics |
| **P0.6** | **Contradiction-detector precision pass**: raise the bar to open a contradiction (coexist/dismiss traps like con-005 "same number, different knob" should not fire). An over-eager detector now suppresses knowledge, not just noise. | fixture design note: "an over-eager detector trains reviewers to ignore the queue" | **M–L** | `brainiac-pipeline` contradict worker |

## P1 — the H4 leak surface (security), still fully open

| # | Feature | Evidence | Effort | Locus |
|---|---|---|---|---|
| **P1.1 ✅ SHIPPED** | **Redaction pass** shipped as `brainiac_core::redact` (recall-biased scanner: PEM private keys, provider key prefixes sk-/brk_/AKIA/ghp_/xox/AIza, connection-string passwords, `secret\|password\|token\|api_key = value`). Applied at BOTH cited exposures: the extractor masks it before a credential becomes a durable memory body, and `memory_provenance` masks the verbatim excerpt before serving. Unit-tested + an end-to-end mcp_pg assertion (planted `sk-…` comes back `[REDACTED]`). | `contractor-webhook-scope` F2 (blocker): `mcp.rs:986-997`, no scrub in `extract.rs` | **M** (done) | `redact.rs`, `extract.rs`, `mcp.rs` |
| **P1.2** | **A 4th visibility tier (scoped/contractor)** or per-memory ACL. Today the model is org/team/private and a contractor must be a full team member (sees the signing-secret runbook by design) or teamless (can't work). | `contractor-webhook-scope` F1 (headline): `migrations/0001_init.sql:252-262` | **L** | schema + RLS + auth |

## P1 — the cross-team seat (Brainiac's own thesis)

| # | Feature | Evidence | Effort | Locus |
|---|---|---|---|---|
| **P1.3** | **A governance/observability role decoupled from team membership** — lets a staff engineer / EM read across teams (or at least see org-level health) and, for maintainer-managers, participate in review. Today `is_maintainer` is strictly per-owning-team and the extractor defaults to `team`, so the people whose job is cross-team see almost nothing. | `v1-fallback` (staff, L1-fail), `promotion-queue-backlog` (Dana can approve nothing) | **L** | schema, RLS, `console.rs:75-89` |
| **P1.4** | **Extractor visibility default reconsidered / promotion nudge** — since `team` is the default, cross-team-valuable knowledge stays invisible unless a human marks it `org`. Surface a "promote to org?" nudge for memories that get cross-team retrieval hits. | `refund-burst` finding: "cross-team win confined to the hand-marked org sliver" (`extract.rs:381-385`) | **M** | extractor + a usage signal |

## P1 — the invocation problem (proactive surfacing)

The `after-the-file` and `decay` journeys share a root cause: **you can't retrieve the answer to a
question you don't know to ask.** The new mid-task tool descriptions help, but nothing *proactively*
tells Jonas the payments timeout changed.

| # | Feature | Evidence | Effort | Locus |
|---|---|---|---|---|
| **P1.5** | **"What changed in your area" digest** — a session-start (or scheduled) push of recent canonical changes/reversals touching the entities/repos a developer works in. Turns after-the-file from pull (must ask) to push (told). | `checkout-timeout-drift` F4, `ledger-payout-recon` (decay): the whole win hinges on an unprompted call | **M–L** | a digest builder over recent canonical mutations + entity anchoring |
| **P1.6** | **Structured `memory_context` scoping** — accept `repo`/`files`/`team` instead of one free-text `task_hint`, so retrieval targets the developer's actual surface instead of hoping the hint ranks the right memory into the top-25. | `memory_context` has no repo/team param (`mcp.rs:614`); several units: "would it even rank in?" | **M** | `mcp.rs` + retrieval |

## P2 — measure what's now measurable, and calibration

| # | Feature | Evidence | Effort | Locus |
|---|---|---|---|---|
| **P2.1** | **Run the H3 cross-stack probe** — `fixtures/v2` now ships the Rust-`tokio` vs browser-`AbortController` pair. Confirm whether a cross-team retrieval floods a dev with stack-wrong advice; if so, add stack tags + stack-aware ranking. | H3 was `not probed` on v1; v2 makes it measurable | **S** (probe) / **M** (fix) | L2 probe, then retrieval |
| **P2.2** | **Confidence calibration** — the auto-candidate gates (≥0.90/≥0.95) key on the LLM's *uncalibrated self-report* (`extract.rs:386`). A confident model walks candidate promotion with no human. Replace with a calibrated or ensemble score, or drop the numeric gate. | `promotion-queue-backlog` F: "thresholds on a number the model made up" | **M** | extractor + policy |
| **P2.3** | **Provider-backed quality L2** — the delta table is still directional (L1) + a plumbing embedder. A real BYOM run gives the actual `C − B` efficiency numbers (the endpoint the literature says the effect lives in). | skill trust rule: "MockProvider measures plumbing, not knowledge" | **S** (run, needs key) | eval + L2 harness |

---

## Recommended sequence

1. **P0.1 + P0.2** (this session, below): governance-health analytics. Small, no schema change,
   and it converts the `harmful-as-shaped` core from invisible to measured — the precondition for
   every other governance fix and the direct resolution of the buyer-blindness finding.
2. **P0.5** next: cheap contradiction dismissal, because the contested-serving fix made precision
   load-bearing and this keeps the safe-default from becoming a knowledge-suppression footgun.
3. **P1.1** (redaction) — the one open *security* blocker; ship before any real transcript ingest.
4. Then the structural/larger items (4th tier, cross-team role, proactive digest) as roadmap.
