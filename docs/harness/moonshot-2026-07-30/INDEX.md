# Moonshot Architect scan — Brainiac, 2026-07-30

> Strategic-ambition scan: what would make Brainiac category-defining, and what caps its ceiling today.
> 25 parallel Opus subagents (one per context in `context-map.json`), 2 findings each, dispatched in
> rolling waves of 8. HEAD `f6e2e75`. Read-only — no repo file was modified outside this directory.

Every scanner was given a **prior-art suppression list** compiled from the `/perfect` vault (51
shipped directions, 6 accepted-unbuilt, 4 explicit user rejections) plus earlier Vibeman campaigns
(~51 closed bug/refactor findings). Contexts with heavy prior art additionally got a targeted
"aim past this" instruction. Zero findings restate prior art.

---

## Totals

| | Critical | High | Medium | Low | **Total** |
|---|---:|---:|---:|---:|---:|
| Across 25 contexts | 30 | 20 | 0 | 0 | **50** |
| Share | 60% | 40% | — | — | 100% |

Counts verified two ways: 50 `- **Severity**:` bullets across 25 reports; 25 × `> Total: 2` headers.

For this lens **severity = strategic priority**, not defect urgency. `Critical` = the product's
ceiling is capped without it. The distribution is top-heavy by construction: each scanner was asked
for only its two best ideas, so the sample is already the 96th percentile of what it found.

---

## Per-context breakdown

| Context | C | H | Report |
|---|---:|---:|---|
| analytics-dashboard | 1 | 1 | [analytics-dashboard.md](./analytics-dashboard.md) |
| api-keys | 1 | 1 | [api-keys.md](./api-keys.md) |
| auth-middleware | 1 | 1 | [auth-middleware.md](./auth-middleware.md) |
| console-shell | 1 | 1 | [console-shell.md](./console-shell.md) |
| database-schema | 1 | 1 | [database-schema.md](./database-schema.md) |
| demo-marketing | 2 | 0 | [demo-marketing.md](./demo-marketing.md) |
| deploy-config | 1 | 1 | [deploy-config.md](./deploy-config.md) |
| document-wiki | 1 | 1 | [document-wiki.md](./document-wiki.md) |
| eval-harness | 2 | 0 | [eval-harness.md](./eval-harness.md) |
| extraction-pipeline | 2 | 0 | [extraction-pipeline.md](./extraction-pipeline.md) |
| graph-visualization | 1 | 1 | [graph-visualization.md](./graph-visualization.md) |
| kb-publishing | 1 | 1 | [kb-publishing.md](./kb-publishing.md) |
| knowledge-health | 1 | 1 | [knowledge-health.md](./knowledge-health.md) |
| library-core | 1 | 1 | [library-core.md](./library-core.md) |
| library-landing | 1 | 1 | [library-landing.md](./library-landing.md) |
| llm-gateway | 1 | 1 | [llm-gateway.md](./llm-gateway.md) |
| onboarding | 1 | 1 | [onboarding.md](./onboarding.md) |
| org-directory | 1 | 1 | [org-directory.md](./org-directory.md) |
| practice-divergence-ui | 1 | 1 | [practice-divergence-ui.md](./practice-divergence-ui.md) |
| projects | 1 | 1 | [projects.md](./projects.md) |
| server-core | 1 | 1 | [server-core.md](./server-core.md) |
| shared-core | 2 | 0 | [shared-core.md](./shared-core.md) |
| skills-catalog | 1 | 1 | [skills-catalog.md](./skills-catalog.md) |
| standards-governance | 1 | 1 | [standards-governance.md](./standards-governance.md) |
| sweep-pipeline | 2 | 0 | [sweep-pipeline.md](./sweep-pipeline.md) |

---

## The finding that 5 independent scanners converged on

`analytics-dashboard`, `sweep-pipeline`, `console-shell`, `knowledge-health` and `api-keys` each
independently discovered the same absence and each proposed building on it:

> **Nothing anywhere records what agents asked for.** `retrieval::search` / `search_reranked` have
> zero writes and zero tracing; all three call chokepoints (`http.rs:369`, `mcp.rs:827`,
> `mcp.rs:1084`) return JSON and forget it. `mcp.rs:863` even computes `withheld_contested` and
> discards it. Pages have `document_reads`, standards and skills have `library_usage_events` — the
> **memory**, the primary artifact, has nothing.

Consequences the scanners drew from it, independently: no answered-rate, coverage, or ROI number is
*computable* (analytics#1); gap-mining "what the org doesn't know" is impossible (sweep#1); a
resolved contradiction can't reach the agents that acted on the wrong belief (api-keys#2); health
can't rank maintenance by consequence (knowledge-health#2). **One append-only `retrieval_events`
table + a served-memories join is the shared substrate under four separate moonshots.** Build once.

---

## Triage themes

| # | Theme | Findings | Why it's a wave, not scattered fixes |
|---|---|---:|---|
| **A** | **Reach the moment of use** — the product knows things but can't get them into the agent's hands at the keystroke | 7 | server-core#1 (`brainiac hook pre-edit` interposer), server-core#2 (stateful MCP session — `capabilities` is `{"tools":{}}`, `serve_stdio` can never speak first), console-shell#1 (no view is linkable — zero of ~14 modules can produce a deep link), console-shell#2 (one SSE event stream replaces 4 pollers), demo-marketing#2 (per-request-auth remote MCP → try-before-install), graph-visualization#1 (agent-callable impact/path/owner queries), api-keys#2 (recall notices). All share one root: delivery depends on the agent *choosing to ask*, and `mcp.rs:468` literally pleads with it to. |
| **B** | **Truth by evidence, not by clock** — "current" means the TTL hasn't expired | 6 | knowledge-health#1 (verification ledger + falsification prover), document-wiki#1 (pages stamped true-as-of-`<sha>`, merge = rot event), database-schema#2 (`memory_anchors` to path/symbol/blob_sha), extraction-pipeline#2 (repo as ground truth, grounding verdict clamps `valid_to`), graph-visualization#2 (`edges.valid_from`/`valid_to` exist with zero writers *and* zero readers), document-wiki#2 (runbooks that execute — `DocKind::Runbook` has zero behavior anywhere). Code change is currently 100% unobserved; staleness is a constant in `MemoryKind::default_ttl_days` that already *actuates* (publish pauses at currency<70). |
| **C** | **Measure demand and outcome** — the product is inventory-only | 6 | analytics#1 (Knowledge P&L), sweep#1 (Demand Ledger + `gaps` sweep), skills-catalog#1 (first artifact whose *effect* is measured), knowledge-health#2 (consequence-ranked debt with forecast payoff), api-keys#1 (agent registry with a track record), auth-middleware#1 (actor trust graph → self-calibrating retrieval). Built on the shared substrate above. `provenance.actor_kind`/`actor_id` exist since `0001_init.sql:52` but the only writer hardcodes `pipeline/extract-worker`, so every memory in every org has the same author. |
| **D** | **Norms with legitimacy, conformance and convergence** — the differentiated idea, half-wired | 6 | library-core#1 (conformance receipts — replace the touch-counter with verdict-bearing evidence, and grade the *rule*), standards#1 (ratification record — `standard_versions` has **no rev-2 writer**, so rewording a rule means retiring it and losing its history), standards#2 (signed effective-dated rulebook editions + grandfathering), practice-divergence#1 (divergence identity/lifecycle → org Alignment Index), practice-divergence#2 (conformance + agent as enforcement surface), sweep#2 (Convergence Ledger — does an adopted standard actually change practice?). |
| **E** | **Beyond the tenant — network effects** | 7 | database-schema#1 (the `visibility` enum literally tops out at `'org'` — knowledge above the tenant is *unrepresentable*, not unimplemented), kb-publishing#1 (signed memory-grade OKF = peering; today the harvester discards any bundle carrying `x_brainiac_*`, so no two Brainiacs can read each other), library-core#2 (portable forkable norm packs), library-landing#1 (public shelf of citable standards), auth-middleware#2 (attenuated delegation → cross-org guest grants), org-directory#2 (identity above org, knowledge survives departure), analytics#2 (k-anonymous cross-org Knowledge Index — `snapshot_all_orgs` already computes comparable scores then files each where only its tenant sees it). |
| **F** | **Provable trust — make the claims structural** | 6 | shared-core#1 (sealed data plane: RLS on 26 of 41 tables, `pool()` has 43 call sites, `scoped_tx`/`worker_tx` share a type so a worker's org-wide escape is compile-compatible with a user path), shared-core#2 (redaction as an enforced boundary — `sources.raw_text` stores transcripts verbatim; `contains_secret` has zero callers), eval#1 (per-tenant golden sets + auto-derived leak probes = signed no-leak attestation), eval#2 (calibrated abstention — the runtime has no score threshold and never exposes confidence), demo-marketing#1 (attested claim ledger — a marketing page that fails CI), library-landing#2 (every public claim bound to a running prover). |
| **G** | **Economics and sovereignty of inference** | 4 | llm-gateway#1 (Sovereign Brain — governance labels already exist as free SFT ground truth; distil + serve locally through the existing `openai-compat` door), llm-gateway#2 (quality contracts at the chokepoint — outcome-graded model report cards), deploy-config#2 (per-org spend ledger + budget governor at the single `Resilience::send` chokepoint; `app-cost.json` is hand-typed with nulls), extraction-pipeline#1 (extraction quality is a `const &str` and recall is 0.458 — learn from the org's own approve/reject labels, tournament-gated). |
| **H** | **Adoption, cold start and org structure** | 8 | onboarding#1 (flip the whitelist from precondition to consequence — `request_approve` hard-409s unless the repo is pre-registered, so all growth is top-down), onboarding#2 (first-hour value from tacit config, zero LLM calls via `manual.rs`), deploy-config#1 (`brainiac.yaml` — the instance as a GitOps artifact; the product lives its own thesis), org-directory#1 (org chart as review engine — teams gate every governance action but **no endpoint creates a team, user or membership**), projects#1 (Concordance Engine — `divergence.rs` already asks the LLM whether N projects agree and throws the "yes" away), projects#2 (fork/archive/merge a project's brain), kb-publishing#2 (reconciliation — publishers are write-only, human edits to published pages are silently destroyed), skills-catalog#2 (capability graph; unmet `requires` edges = the first mechanical answer to "what know-how is the org missing"). |

---

## Bug side-catches (NOT moonshots — real defects found while verifying premises)

These are cheap, independent of any theme, and three of them are honesty defects on surfaces the
product uses to earn trust. Worth fixing regardless of which themes get picked.

1. **`divergence::scan_org` DELETEs and re-mints every divergence with a fresh UUID each sweep**
   (`divergence.rs:242`). Because `library_sweep::blocked()` dedupes mined candidates on
   `standard_provenance.ref_id` = that id, **every sweep mints a duplicate standard candidate for
   the same practice**, and every ratified standard's divergence provenance is a dangling pointer.
2. **The pitch page's proof artifact is the wrong policy.** `pitch-data.ts:132` presents
   `memories_read` as the checkable evidence; it is a stale hand-copy of `0001_init.sql:252`.
   `0002_worker_read.sql:14` replaced that policy (adds `deleted_at IS NULL` + a worker bypass).
   The one code block offered publicly as evidence is not the policy that runs.
3. **`/library` prose contradicts its own data.** `library-data.ts` marks the ladder/properties
   `shipped`, while `sections/StatusLadder.tsx:15`, `Finale.tsx:36` and `Anatomy.tsx:26-29` still
   tell visitors it is "almost all roadmap". The honesty guard (`library-data.test.ts`) flattens
   only the data module, so it structurally cannot see section prose.
4. **The disabled "ratify as standard" button is a one-column fix.** `POST
   /v1/library/divergences/{id}/ratify` and `library::ratify_divergence` both exist; the blocker is
   that `console.rs:2982` never selects `id`.
5. **`list_pending(pool)` takes no `org_id`** (`onboard.rs`), so an unclaimed pairing request is
   addressed to nobody and visible to every admin on the deployment.
6. **CI baselines are calibrated on `deterministic-bow-v1`**, a toy bag-of-words embedder — so
   today's eval gates defend numbers no customer ever observes.

---

## Suggested wave split (if the backlog is worked rather than banked)

Ordered so each wave's substrate unblocks the next, rather than by theme appeal.

| Wave | Content | Why here |
|---|---|---|
| 1 | **Bug side-catches 1–5** | Hours, not days. #1 is actively corrupting the library candidate stream every sweep; #2/#3 are public credibility. No dependencies. |
| 2 | **Theme C substrate** — `retrieval_events` + served-memories join, `provenance.actor_id` actually written | The single most-depended-on missing table in the scan; 4 other moonshots sit on it. Nothing else in the backlog unlocks as much per line. |
| 3 | **Theme A** — pre-edit hook + linkable console addresses | Turns knowledge into delivery at the keystroke. #1 is the highest-ceiling single finding in the scan. Wave 2 makes its effect measurable. |
| 4 | **Theme B** — code anchoring (`memory_anchors`, doc `verified_at`) | Converts staleness from a calendar constant into evidence. Independent of 2–3 but shares the repo-read seam with extraction#2. |
| 5 | **Theme F** — sealed data plane + tenancy proof | The security posture the "zero permission leaks" claim already rests on. RLS gaps on `api_tokens`/`identities`/`projects`/`users` are the honest starting point. |
| 6 | **Theme D**, then **G**, then **E/H** | D completes the differentiated loop; G changes unit economics; E/H are the platform bets and want a deliberate product decision first. |

---

## How this scan was run

- **Scanner**: `moonshot-architect` (`src/lib/prompts/registry/agents/moonshot-architect.ts`), role
  prompt reproduced in [`_SCAN-BRIEF.md`](./_SCAN-BRIEF.md).
- **Model**: Opus per subagent (project convention: moonshot/design work is never run on a smaller model).
- **Scope**: all 25 contexts from `context-map.json` (258 files), 2 findings each, read-only.
- **Date / HEAD**: 2026-07-30 / `f6e2e75` on `master`, working tree dirty with unrelated in-flight
  deploy/telemetry/provider work (nothing staged by this scan).
- **Files read**: ~380 across all subagents (11–34 per context).
- **Prior-coverage handling**: suppression list from the `/perfect` vault + prior Vibeman campaigns,
  injected verbatim into every subagent prompt above the role prompt.
- **Verification**: findings counted two ways (severity bullets = 50; per-report header totals = 50).

### Map drift found during the scan (fix before the next scan)

- `skills-catalog` files are at `console/app/console/modules/skills/` (4 files:
  `Module.tsx`, `SkillsCatalog.tsx`, `Skeleton.tsx`, `skills-data.ts`) — the map says
  `console/src/app/...` and lists 1.
- `graph-visualization` and `skills-catalog` both carry empty `file_paths` (entry points only).
- `coverage.mjs` mis-parses this repo's map shape: top-level `contexts[]` with an empty `groups: []`
  takes the groups branch and reports "no contexts found". One-line fix (`j.groups?.length`).
