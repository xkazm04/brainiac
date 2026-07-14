# Code-Refactor + Bug-Hunter Dual-Lens Scan — Brainiac, 2026-07-14

> Full-coverage dual-lens audit (🐛 bug-hunter + 🧹 code-refactor combined, 5 findings/unit).
> 32 parallel subagent runs across **all 217 source files** (78 Rust / 8 crates + 139 console TS/TSX), batched in 4 waves of 8.
> Scan units were derived fresh from disk because the Vibeman context map (19 ctx, built 2026-07-10) covered only ~30% of the current codebase and pointed 8 paths at deleted files.

---

## Totals

| | Critical | High | Medium | Low | **Total** |
|---|---:|---:|---:|---:|---:|
| Across 32 units | 5 | 56 | 81 | 18 | **160** |
| Share | 3% | 35% | 51% | 11% | 100% |

Lens split: **112 bug-hunter** / **48 code-refactor**. Counts verified two ways (Σ`> Total:` headers = Σ`**Severity**:` bullets = 160).

Baseline health (pre-fix): `cargo check --workspace --all-targets` → **0 errors**. Console has vitest suites (age/markdown/kb-data/api/demo-fallback/edit-copy).

---

## Per-unit breakdown (sorted by criticals, then highs)

| Unit | Name | C | H | M | L | Report |
|---|---|---:|---:|---:|---:|---|
| R04 | Server: Docs + Sweeps + OpenAPI | 1 | 1 | 3 | 0 | `srv-docs-sweeps-openapi.md` |
| R06 | Pipeline: Worker + Compose | 1 | 2 | 2 | 0 | `pipe-worker-compose.md` |
| R07 | Pipeline: Divergence + Reembed | 1 | 2 | 2 | 0 | `pipe-divergence-reembed.md` |
| R13 | Gateway: BYOM + Resilience + Health/Redact | 1 | 3 | 1 | 0 | `gateway-byom-resilience.md` |
| C05 | Console: Promotion Review Queue | 1 | 2 | 2 | 0 | `con-reviews.md` |
| R03 | Server: MCP Agent Surface | 0 | 3 | 2 | 0 | `srv-mcp.md` |
| R05 | Pipeline: Extract + Resolve + Contradict | 0 | 3 | 2 | 0 | `pipe-extract-resolve.md` |
| C03 | Console: API Client + Types + Auth | 0 | 3 | 2 | 0 | `con-lib-api-client.md` |
| C15 | Console: Demo Mode + Login | 0 | 3 | 1 | 1 | `con-demo-login.md` |
| R01 | Server: Governance Console API | 0 | 2 | 2 | 1 | `srv-console-governance.md` |
| R09 | Store: Orgs + Entities + Governance | 0 | 2 | 2 | 1 | `store-orgs-entities-governance.md` |
| R10 | Store: Documents + Feedback + Tokens | 0 | 2 | 2 | 1 | `store-documents-feedback-tokens.md` |
| R14 | Fixtures: Golden Loader + Validate | 0 | 2 | 3 | 0 | `fixtures-loader.md` |
| R15 | Eval: Harness Core + Retrieval/Grid | 0 | 2 | 3 | 0 | `eval-harness-core.md` |
| R16 | Eval: Docs/Contradiction/…​ Profiles | 0 | 2 | 3 | 0 | `eval-profiles-2.md` |
| R17 | Publish: Render + Confluence + Git | 0 | 2 | 3 | 0 | `publish-crate.md` |
| C04 | Console: Next.js API Routes | 0 | 2 | 3 | 0 | `con-api-routes.md` |
| C06 | Console: Disputes Bench | 0 | 2 | 2 | 1 | `con-disputes.md` |
| C08 | Console: Ingest Monitor | 0 | 2 | 2 | 1 | `con-ingest.md` |
| C09 | Console: API Keys Management | 0 | 2 | 2 | 1 | `con-keys.md` |
| C10 | Console: Memory Archive + Inspector | 0 | 2 | 2 | 1 | `con-memories.md` |
| C11 | Console: Docs Reader + Editor | 0 | 2 | 3 | 0 | `con-docs.md` |
| R02 | Server: REST HTTP + Auth + Entrypoint | 0 | 1 | 3 | 1 | `srv-http-auth-main.md` |
| R08 | Store: Memories + Hybrid Retrieval + Queue | 0 | 1 | 4 | 0 | `store-memories-retrieval-queue.md` |
| R11 | Core: Domain Types + Embeddings | 0 | 1 | 3 | 1 | `core-types-embed.md` |
| R12 | Core: Fusion + Temporal + Metrics + Scoring | 0 | 1 | 3 | 1 | `core-fusion-scoring.md` |
| C02 | Console: Home + Station Modules | 0 | 1 | 3 | 1 | `con-home-stations.md` |
| C07 | Console: Knowledge Graph Explorer | 0 | 1 | 3 | 1 | `con-graph.md` |
| C12 | Console: Health + Analytics + Ops | 0 | 1 | 3 | 1 | `con-health-analytics-ops.md` |
| C14 | Console: Pitch / Marketing | 0 | 1 | 3 | 1 | `con-pitch.md` |
| C01 | Console: Shell + Nav + Design System | 0 | 0 | 4 | 1 | `con-shell-nav-design.md` |
| C13 | Console: Knowledge Base Explainer | 0 | 0 | 3 | 2 | `con-kb.md` |

---

## The 5 Critical findings

1. **[R04] `doc_edit` is a write endpoint gated on `kb:read` with no maintainer check.** Any read-scoped agent token can rewrite published pinned prose AND inject "a maintainer edited…"-framed text into the extraction pipeline — defeating the read/write separation `doc_approve` (kb:publish + is_maintainer) enforces. `crates/brainiac-server/src/docs.rs:398`.
2. **[R06] `compose_tick` clears `dirty_at` unconditionally** after a multi-second LLM compose; a dependency memory that changes mid-compose is silently marked clean and dropped — the "wiki cannot rot" guarantee, defeated by a lost-update. `crates/brainiac-pipeline/src/worker.rs:210-293`.
3. **[R07] A half-backfilled embedding version is fully servable.** `ensure_embedding_version` eager-sets `is_active=true`, `is_active` is never read, and the served version derives from the configured embedder — so an interrupted per-batch reembed leaves a partially-populated version returning silently incomplete results corpus-wide. `crates/brainiac-pipeline/src/reembed.rs:55-106`.
4. **[R13] Provider HTTP calls have no timeout.** `reqwest::Client::new()` with no `.timeout()` and no `tokio::time::timeout` — a stalled-but-connected upstream hangs the worker forever; retries never fire, the circuit-breaker never records a failure. `crates/brainiac-gateway/src/lib.rs:66,181` + `resilience.rs:152-192`.
5. **[C05] The governance gate has no per-maintainer authorization.** The console approve/reject server actions do no console-side maintainer check and act as one shared server principal, so any passcode holder approves, and the signed audit trail cannot name the human. `console/app/console/(modules)/reviews/actions.ts:27-54`. *(Root cause is shared with C06/C11 — see Theme A.)*

---

## Triage themes

Clustered from the 5 Criticals + 56 Highs (Mediums/Lows roll up into these or into the refactor themes below).

| # | Theme | Findings | Why it's a wave, not scattered fixes |
|---|---|---:|---|
| **A** | **Governance authorization collapse** — console has no per-user identity; every server action authorizes via one shared `BRAINIAC_API_TOKEN`, so the backend's per-team maintainer check is enforced against a constant. Plus the Rust `doc_edit` scope hole and non-atomic promotion TOCTOU. | 6 (2C/4H) | Same root cause across reviews/disputes/docs + the server twin; fix the identity-forwarding seam once and all four modules benefit. This is the product's core value prop. |
| **B** | **No HTTP/fetch timeouts (hang / resource exhaustion)** — `reqwest::Client::new()` and browser `fetch()` with no timeout, on the worker, the publisher (holds a DB txn!), and every console call. | 4 (1C/3H) | One pattern, two stacks; a shared timeout wrapper closes all of them. |
| **C** | **Anti-rot write-loss (compose / revisions)** — `dirty_at` cleared unconditionally, revision regression to a superseded version, concurrent-edit lost updates. | 4 (1C/3H) | All converge on the `documents` dirty_at / revision path; one transactional redesign. |
| **D** | **Poisoned / incomplete knowledge enters retrieval** — auto-promotion ignores a just-opened contradiction, lexical false-merge at conf 1.0, silent 0-extraction, unvalidated supersession winner (self/cycle), cyclic-chain dedupe failure. | 6 (1C/5H) | The knowledge-integrity invariant; each fix guards a different door into the same corrupt-retrieval outcome. |
| **E** | **Silent failures / success theater** — 0-row UPDATE → phantom approval; swallowed revoke; `withDemoFallback` hides 401/500; live fetch failures fabricate demo evidence/memories as real; empty provider usage meters 0 & fakes success. | 7 (7H) | A single mental model: "an error path that reports success"; the fixes share a surface-the-error pattern. |
| **F** | **Eval gate integrity (gates that pass when they shouldn't)** — superseded-in-top3 gate uses hand-annotated strings; refusal violations no gate consumes; leak gate only reads prose; hallucination gate checks a substring; missing `pages.yaml` voids the whole gate; partial id-collision coverage. | 6 (6H) | The CI quality guard silently green — dangerous as a class; fix the whole eval trust boundary in one session. |
| **G** | **Reliability: shutdown, retry storms, breaker, unbounded** — SIGTERM never drains; unbounded stdin read (DoS); failed audit-row aborts ack → reprocess; deterministic-fail page recomposes forever; ingest 6s poll no in-flight guard; breaker half-open herd. | 6 (6H) | Operational robustness under adverse conditions; naturally one "make it survive prod" wave. |
| **H** | **Console session / route auth** — keyless unsalted SHA-256 session cookie (forgeable/unrevocable), open-redirect bypass, no passcode rate-limit, GET-by-id routes reachable without a session via the `.txt` matcher hole. | 4 (4H) | The console's own front-door auth; distinct from Theme A (which is backend authz forwarding). |
| **I** | **Correctness edge cases** — `as_of` degrades to now, reembed dim-mismatch silently excluded, graph-expansion picks by UUID not strength, `SectionBinding` Default=0 vs serde=12, divergence crash on bad date (no error boundary), reduced-motion hero blank, RAF leak, silent 40-row archive truncation. | 8 (8H) | Wrong-but-not-crashing results + a couple render breaks; each is a self-contained repro. |
| **J** | **Redaction / metering** — redaction misses bearer/JWT + compound token keys; metering-bypass on empty usage. | 1–2 (H) | Small; can fold into Theme B or E. |
| **R** | **Refactor tail (code-refactor lens)** — duplication (`vector_literal` ×2, `cosine` ×3, TRUNCATE lists ×3–4, design palette/tokens across C01/C13/C14, fetch/error boilerplate across console, demo-data forks), dead code (`NavStatus.tsx`, `MODULE_BAND`, `Severity::Warning`, `DEMO_MODULE_IDS`, dead props), god-components (Home 765 / StationModules 906 / Pitch 851 LOC). | ~48 (M/L) | Best done as dedicated consolidation sessions after the bug waves; low regression risk, high readability payoff. |

---

## Suggested wave plan

Bug waves first (Criticals + Highs, ~61 findings → ~9 focused sessions), refactor tail last. Each wave is one mental model, ~5–7 fixes.

- **Wave 1 — Governance authorization (Theme A).** C05▲, R04▲, C06-authz, C11-authz, R01 promotion-TOCTOU, R01 health-RLS-scope. *The product is human-in-the-loop governance; start here.*
- **Wave 2 — Knowledge integrity (Theme D).** R07▲, R05 auto-promote-vs-contradiction, R05 false-merge, R05 silent-0-extract, R09 supersession-winner, R12 cyclic-dedupe.
- **Wave 3 — Anti-rot write-loss (Theme C).** R06▲, R10 insert_revision dirty_at, R10 revision-regression, R04 concurrent-edit lost-update. *(R06 & R10 are the same bug from worker vs store side — fix together.)*
- **Wave 4 — Timeouts & hangs (Theme B).** R13▲, R17 confluence-timeout-holds-txn, C03 console-no-timeout, C04 route-no-timeout. Add a shared timeout helper per stack.
- **Wave 5 — Silent failures / success theater (Theme E).** R09 phantom-approval, C09 swallowed-revoke, C03 withDemoFallback-hides-errors, C07 fabricated-graph-evidence, C10 fabricated-memory, R13 empty-usage-fake-success.
- **Wave 6 — Eval gate integrity (Theme F).** R15 superseded-gate, R15 refusal-unconsumed, R16 prose-only-leak-gate, R16 substring-hallucination-gate, R14 missing-pages voids gate, R14 id-collision-coverage.
- **Wave 7 — Reliability (Theme G).** R02 SIGTERM, R03 unbounded-stdin, R06 audit-row-aborts-ack, R06 recompose-storm, C08 poll-no-guard, R13 breaker-herd.
- **Wave 8 — Console session/route auth (Theme H).** C15 session-cookie, C15 open-redirect, C15 rate-limit, C04 `.txt` matcher hole. *(Product decision needed: does the console want per-user identity or is single-passcode acceptable? See open questions.)*
- **Wave 9 — Correctness edge cases (Theme I).** R03 as_of-degrade, R07 dim-mismatch, R08 DISTINCT-ON-by-UUID, R11 SectionBinding-default, C12 divergence-crash + error boundary, C02 reduced-motion-hero, C14 RAF-leak, C10 silent-truncation.
- **Waves 10+ — Refactor tail (Theme R).** Dedicated dedup/dead-code/god-component sessions; batch by shared-file to avoid churn.

*(▲ = Critical)*

---

## Cross-cutting notes for whoever runs the waves

- **Theme A severity is contested by design.** C05 rated the shared-token authz gap **Critical**; C11 and C06 rated the same root cause **High** because `console/middleware.ts` blocks *anonymous* access — so the real gap is authenticated-but-not-authorized (any passcode holder acts as every team's maintainer). The console currently has **no role tiers at all** (single passcode, per C12/C15). Whether this is Critical or High hinges on a product decision about console identity. Treat the theme as top-priority regardless.
- **Verify-before-fix paid off.** Six subagents explicitly declined to fabricate a Critical after checking a crown-jewel hypothesis and finding the defense sound (MCP RLS re-scoping, docs XSS, token entropy/hashing, C09 secret-in-state, C12 sweep auth, C07/C10 fetch-race). Don't re-flag these as new findings.
- **Dedup:** R06 (worker `compose_tick`) and R10 (store `insert_revision`) describe the **same** `dirty_at` lost-update from two sides — one fix. R05 auto-promote and R01 promotion-TOCTOU both touch the promotion path but are distinct (policy input vs. row-level atomicity).
- **Working tree is dirty:** an in-progress console refactor (14 files, +96/−392, finishing demo-page consolidation) sits uncommitted on `master`. Per the run decision, fix waves branch off `master` and leave that WIP untouched, staging only files each fix edits.

---

## How this scan was run

- **Scanners:** dual-lens — `code-refactor` (id `agent_code_refactor`) + `bug-hunter` (id `agent_bug_hunter`) from Vibeman's registry, combined into one prompt per unit, 5 findings each.
- **Date:** 2026-07-14. **Project:** Brainiac (`36a8a77d-87b2-491c-8f9a-117ce7bcdbcf`), `C:/Users/mkdol/dolla/brainiac`, Rust workspace (8 crates) + Next.js 15 console.
- **Scope:** full-coverage — all 217 source files, grouped into 32 fresh scan units (18 Rust + 14 console… i.e. 17 Rust R01–R17 + 15 console C01–C15) derived from disk, LOC-balanced. Coverage validated: 217/217 files, 0 gaps, 0 duplicates.
- **Method:** 32 `general-purpose` subagents in 4 waves of 8; each read its unit's files fully (~5–19 files incl. neighbors), wrote one report, replied with terse stats. Orchestrator read only the replies during scanning.
- **Files read by subagents (approx):** ~320 file-reads across the fleet (units + neighbor context).
- **Verification:** findings counted two ways (header sum = bullet count = 160); per-file heading count = 5 for all 32 reports.
