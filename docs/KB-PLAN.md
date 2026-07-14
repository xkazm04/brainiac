# Brainiac — Knowledge Base Layer Plan (v0.5 line)

Goal: implement the auto-managed document layer of `docs/ARCHITECTURE.md` §8 —
**the wiki that cannot rot** — through external publishing (Confluence), in
production-quality Rust, gated by the eval metrics already specified in
`docs/EVAL.md` §2.6. This plan extends `docs/PLAN.md` (baseline, complete) and
follows its conventions: ARCHITECTURE.md stays the contract, each phase ends
with green tests + one commit, deviations are recorded here.

Design settled 2026-07-14 (session discussion). The one-line thesis:

> The memory layer and the knowledge base layer are separate, and the
> relationship is **asymmetric**: memory is the only source of truth; a KB page
> is a compiled projection over canonical memories; the only path from a page
> back into truth is the extraction pipeline. No peer stores, no bidirectional
> sync — that asymmetry is the anti-rot mechanism and it reuses the product's
> core differentiator (nothing becomes canonical without a named human).

## Settled design decisions

| # | Decision | Rationale / boundary |
|---|---|---|
| D1 | **Projection with asymmetry.** Pages regenerate from canonical memories (dirty-marking via `document_dependencies`). Human edits to composed sections re-enter through extraction as candidate memories; only pinned sections are directly ownable prose. | §8.3 as written. Rejected: "two layers influencing each other" as peer stores — bidirectional sync is the classic wiki failure mode. |
| D2 | **Lifecycle facet on memories**: `shipped \| in_flight \| proposed`. Extraction populates it; pages render the split ("in product" vs "on its way"). | Docs describing unshipped features are a top doc-rot failure; the temporal machinery (`valid_from`, as-of) already half-supports this. Schema change → do it **before** pages exist. |
| D3 | **Structured memory payloads**: optional `detail_md` (code block / table / config snippet) preserved by extraction alongside the distilled one-sentence `content`. | Extraction currently flattens everything to a single sentence — a hard quality ceiling for composed pages. Extraction-schema change → fixtures + gold updates → do it before composition exists. Recall guard: past prompt rewrites have regressed recall (commit a007953); every prompt change runs the per-provider extraction eval. |
| D4 | **Publisher interface, not Atlassian code.** One `Publisher` trait; targets: Git `docs/` (ARCHITECTURE stage-7 export) and Confluence (PAT). Confluence is **one-way push, render target only** — generated banner, provenance links back to the console, direct edits overwritten (harvesting them is Level 2, deferred). | Flips the pitch: Confluence stops being the incumbent to beat and becomes a surface we keep honest. Markdown→ADF/storage conversion is owned by the adapter. |
| D5 | **Publish visibility rule (v1): `org`-visible canonical memories only** may compose into externally published pages. Team/private knowledge renders in the console only. | External publish exits RLS entirely; EVAL §2.6 leak tolerance is zero. Team-space mapping is a follow-up, not v1. |
| D6 | **KB scoping at token level**: scopes `kb:read`, `kb:compose`, `kb:publish` on API tokens; KB layer is an org-level capability flag (optional but recommended). | Fits the existing principal/token model (`0003_api_tokens.sql`); single-team orgs (where UAT showed the product is pure cost) don't pay for a layer they won't use. |
| D7 | **Health-gated publishing.** Knowledge Health (already live: `/v1/analytics/knowledge-health`, snapshots in `0014`) is the **circuit breaker** for auto-publish: currency or governance pillar below threshold → external sync pauses, pages hold last revision with a "verification pending" stamp. | An auto-synced wiki is an amplifier; UAT's central negative finding is the backlog being served as truth with nothing going red. This turns the scorecard from report into actuator. |
| D8 | **Contribution Level 1 only** in this plan: the dev/agent session → extraction → memories → dirty pages → recompose loop, plus agent *read* access to pages over MCP. Agents never write pages directly. Level 2 (repo scans, cross-documentation intelligence) is deferred wholesale (see Follow-ups). | Extraction recall on curated transcripts is ~0.46; pointing the extractor at whole repos multiplies input noise before the instrument is fixed. |
| D9 | **Diagrams deferred**, with one cheap exception allowed inside KB2: a *deterministic* mermaid block on entity pages compiled from the entity/edge graph (no LLM, zero hallucination risk). Everything LLM-authored: follow-up. | Cherry on top by design; the deterministic projection is the only piece cheap and safe enough to justify early. |

## Architecture deltas (fold into ARCHITECTURE.md during KB0)

§8 is the base; these amend it. KB0 updates ARCHITECTURE.md so the contract
stays authoritative before code lands:

1. §2.3 memories: add `lifecycle` enum (D2) and `detail_md` (D3); extraction
   contract gains both fields (optional, never required for promotion).
2. §8.2 publish step: generalize "exported to Git" to the `Publisher` trait
   (D4) and add the health circuit breaker (D7) between policy and publish.
3. §8 new subsection: external publishing visibility rule (D5) and KB token
   scopes (D6).
4. §9: move document layer from "v0.5 (deferred)" to "in progress — see
   docs/KB-PLAN.md".

## Phase ladder

| # | Phase | Deliverable | Test gate |
|---|---|---|---|
| KB0 | Substrate prerequisites | `lifecycle` + `detail_md` on memories (migration 0015) end-to-end: types, extraction prompt + repair, store, retrieval assembly, fixtures/gold updated; ARCHITECTURE.md deltas folded in; **Knowledge Health console page** (the API is currently invisible) | extraction eval per provider: recall/precision within gate vs baseline (no regression from schema change); console page renders live API |
| KB1 | Document layer core | Migration 0017 (0016 is taken by `practice_divergences`): `documents`, `document_sections`, `document_revisions`, `document_dependencies` + RLS; `compose` job kind on the existing SKIP-LOCKED queue; dirty-marking worker; composition (binding → canonical-only retrieval as synthetic visibility-capped principal → BYOM prose with `[m:uuid]` citations); diff + policy (typed rules behind `PolicyEngine`, `auto_published` vs `needs_review`); revision review via the promotions queue UI pattern | new `docs` eval profile (EVAL §2.6) with `fixtures/v1/documents/` gold: coverage, **hallucination = 0 for auto-published**, **leak = 0 (build failure)**, staleness-propagation, pin-preservation |
| KB2 | Read surfaces | Console page reader: markdown renderer (sanitized), per-claim provenance popovers from `[m:uuid]`, revision history + diff view; page editor aware of pinned vs composed; MCP `doc_get(slug, as_of?)` + `doc_search(query)`; `entity_page` auto-scaffolding (≥ N canonical memories across ≥ 2 teams); optional: deterministic mermaid entity-neighborhood block (D9) | console vitest + `mcp_pg` tests incl. RLS leak check on `doc_get`; scaffolding validated on Meridian |
| KB3 | Publishing | `Publisher` trait + Git target (semver, `docs/` in org repo) + Confluence adapter (PAT, markdown→ADF, one-way, generated banner + backlinks); KB token scopes + org capability flag; **health circuit breaker** wired (pillar thresholds → pause + "verification pending" stamp); publish path covered by the leak eval (org-only rule D5) | `docs` profile extended with publish-path leak cases = 0; circuit breaker integration test (degrade health on Meridian → sync pauses) |
| KB5 | Tell the world (runs alongside KB1–KB2, not after) | The KB layer is the product's answer to a problem every org has, and it is currently invisible outside the repo: **(a)** a dedicated public page presenting the knowledge base (projection-not-peer, lifecycle split, health-gated publishing, one-way Confluence), **(b)** the landing/pitch page updated so the KB is a named capability rather than only a competitor teardown, **(c)** `README.md` — the KB layer in the feature list and quickstart, **(d)** feature documentation (`docs/` — how a team enables the KB, what a composed page is, what it will never do). Honesty rule inherited from `pitch-data.ts:1-19`: **nothing may be presented as shipped that is not shipped** — unbuilt phases are described as roadmap, in the same voice as the existing "where we lose" section | console vitest green; every claim on the public pages traceable to a merged phase in this plan (a reviewer can check each one against the status log) |
| KB4 | Round-trip & hardening | Human edit flow: composed-section edit → extraction → candidate memories → "your change was captured as N proposed updates" → recompose on landing; contradiction-resolution → dirty → recompose SLA measured; Knowledge Health gains a docs pillar signal (dirty-page backlog, stale published revisions) | `full` profile run incl. docs; staleness-propagation latency recorded in results/history/ |

Sequencing rule carried over from UAT: **KB3 external publishing does not ship
to a real org until the extraction-recall workstream (tracked in
`uat/runs/2026-07-13-l2-real/report.md` §next-development) clears its gate.**
Composed pages inherit substrate trust; publishing amplifies whatever is wrong.

## Crate/dir map (delta)

```
crates/
├── brainiac-core       # + lifecycle enum, detail_md on Memory; doc domain types
├── brainiac-store      # + documents/sections/revisions/dependencies repos, RLS
├── brainiac-pipeline   # + compose worker, dirty-marking, edit-reingestion source kind
├── brainiac-publish    # NEW: Publisher trait, git + confluence adapters, md→ADF
├── brainiac-eval       # + docs profile (EVAL §2.6), publish-leak cases
└── brainiac-server     # + /v1/docs REST, MCP doc_get/doc_search, kb:* scopes, breaker
console/                # + knowledge-health page (KB0), docs reader/editor (KB2)
fixtures/v1/documents/  # NEW: composition gold (EVAL §2.6 example is the template)
migrations/             # 0015 memory facets (done), 0017 document layer, 0018 kb scopes
                        # (0016 belongs to practice_divergences — a parallel workstream)
```

## Deviations from ARCHITECTURE.md (deliberate, revisitable)

1. **Queue**: compose jobs ride the existing v0 SKIP-LOCKED `queue` schema, not
   a pgmq `compose` queue (same deviation #1 as PLAN.md).
2. **Policy**: revision publish policy uses the typed-Rust `PolicyEngine`, not
   Cedar (same deviation #2 as PLAN.md).
3. **Confluence before "one Git connector"**: ARCHITECTURE defers connectors,
   but publishing (outbound) is not a connector (inbound); the publisher trait
   keeps it a leaf dependency.

## Follow-ups & next directions (deferred backlog — future sessions start here)

Ordered roughly by leverage; none block the phase ladder.

1. **Diagrams ladder (D9 continuation).** (a) deterministic mermaid
   entity-neighborhood on entity pages — allowed in KB2; (b) deterministic
   sequence/flow diagrams compiled from runbook-kind memory chains; (c)
   LLM-proposed diagrams entering through the same review gate as prose
   (hallucination gate applies to edges: every edge must cite a memory);
   (d) diagram rendering in the Confluence adapter (plugin/ADF extension
   territory — investigate before promising).
2. **Level 2 — cross-documentation intelligence** (one huge standalone topic):
   repo/doc-corpus scans as ingestion sources; **docs-drift detection** (diff
   existing human docs against canonical memories → propose supersessions or
   page bindings); Confluence *harvest* (direct edits to published pages
   captured as extraction sources instead of overwritten — closes the D4
   one-way limitation). Prerequisite: extraction recall gate green; design
   its own eval fixtures first (a synthetic stale-docs corpus for Meridian).
3. **Proactive digest** (UAT P1.5): session-start / scheduled push of canonical
   changes touching the developer's entities. Note for the designer: a digest
   is *also a projection* — model it as a `doc_kind: digest` with a
   time-windowed binding and reuse the compose pipeline rather than building a
   parallel generator.
4. **More publish targets** behind the same trait: Notion, GitHub wiki,
   Backstage TechDocs. Cheap once D4 lands; pick by customer pull.
5. **Team-space Confluence mapping** (relaxes D5): per-team spaces with
   visibility mapping, only after the leak eval covers the mapping matrix.
6. **Substrate trust workstream** (owned by UAT follow-ups, blocking KB3 GA):
   extraction recall per provider, confidence calibration for auto-promotion,
   raw-memory TTL sweep, review-SLO alerting.
7. **Scoped/contractor visibility tier + cross-team observability role** (UAT
   structural gap): affects both retrieval and the publish visibility rule —
   design once, apply to both layers.
8. **Page analytics as liquidity signals**: which pages/sections agents and
   humans actually read feeds the Knowledge Health liquidity pillar and
   entity-page scaffolding thresholds.
9. **Automated decay scoring** (ARCHITECTURE deferred list): `memory_feedback`
   volume is the input; pages give it a new output — visibly aging sections.

## Status log

- [x] **KB0 substrate prerequisites** (2026-07-14)
      - `migrations/0015_memory_facets.sql`: `lifecycle` (checked enum, default
        `shipped`) + `detail_md` on `memories`, `(org_id, lifecycle)` index.
      - `brainiac-core`: `Lifecycle` enum; `Memory.lifecycle` / `.detail_md`.
      - `brainiac-store`: `NewMemory` + insert + `MEMORY_COLUMNS` + row mapping.
      - `brainiac-pipeline`: extraction prompt carries both facets as OPTIONAL
        fields (V1 prompt otherwise untouched — the recall scar in
        `extract.rs` is why); facet firewall coerces an unknown lifecycle to
        `shipped` rather than dropping the memory; `detail_md` is redacted
        through the same secret firewall as `content` and clipped to 2k chars.
        Five new unit tests cover the firewall.
      - `brainiac-fixtures` + `fixtures/v1/memories/gold.yaml`: both facets in
        the gold schema, with exercise cases (`mem-pay-0043` carries the retry
        config as `detail_md`; `mem-pay-0044` is an `in_flight` decision that is
        canonical and valid yet describes nothing in production).
      - Console: `/health` — the Knowledge Health leadership report (score,
        grade, trend line, four pillars, ranked attention list, corpus signals),
        live API with demo fallback behind `DemoBanner`; added to the nav.
      - `docs/ARCHITECTURE.md`: §2.3 facets, §8.2 health gate, new §8.5
        (Publisher trait, one-way/org-only external publish, KB scopes), §9
        marks the doc layer in progress.
      - **Extraction eval (the KB0 gate) — PASSED**, on real qwen-max +
        `text-embedding-v4` (`results/kb0-extraction.json`): recall 0.381,
        precision 0.727, F1 0.500 vs baseline 0.4167 / 0.8058 / 0.5229. Every
        rate is inside the `RATE_DELTA` = 0.15 gate, and recall sits mid-band
        of the documented identical-config spread (0.25–0.54, mean 0.42). Read
        honestly: this is ONE noisy sample, so it demonstrates no *detectable*
        regression rather than proving the facets are free. It is the same
        standard of evidence the V1 prompt itself stands on. If a later
        multi-sample run shows the facets costing recall, the fix is to move
        them OUT of the extraction prompt into a second cheap pass over the
        already-extracted memory — the schema and firewall do not change.
      - Postgres integration suites green against a real DB (store_pg,
        pipeline_pg, reembed_pg): migration 0015 applies and the facets survive
        the insert→read path under RLS.
      - Pre-existing and unrelated: `clippy -D warnings` fails on two
        `unwrap()`s in `store_pg.rs` on HEAD (present before this work).
- [~] **KB1 document layer core** (2026-07-14) — code complete, one gate open.
      - `migrations/0017_document_layer.sql`: `documents` / `document_sections`
        / `document_revisions` / `document_dependencies`, RLS + `brainiac_app`
        grants. (NB: 0016 and 0018 belong to parallel workstreams —
        `practice_divergences`, `sweep_schedules`. Next free number is 0019.)
      - `brainiac-core`: `DocKind`, `DocStatus`, `SectionMode`,
        `SectionBinding` (entities + kinds + **lifecycle** + query),
        `RevisionPolicy`, `Document`, `DocumentSection`, `DocumentRevision`.
      - `brainiac-store::documents`: repo + `mark_dirty_for_memory` — the
        anti-rot call, hooked into `governance::set_memory_status` and
        `apply_supersession` so NO governance path can change a memory's
        standing and forget the pages built on it.
      - `brainiac-pipeline::compose` + `worker::compose_tick`: dirty-page work
        list → visibility-capped retrieval → cited prose → policy → revision.
        Three firewalls: **visibility cap** (org pages compose as a principal
        with no team memberships, so RLS itself makes team-private memories
        unreachable), **citation firewall** (invented `[m:uuid]`s are stripped;
        an unbacked paragraph blocks auto-publish), **deterministic evidence**
        (`detail_md` is copied verbatim, never re-typed by the model).
      - Policy: a page's FIRST revision always needs a human (nothing publishes
        itself into existence); an additive recompose that keeps every
        previously published claim auto-publishes; a dropped claim goes to
        review (supersession working vs retrieval silently losing knowledge is
        a human's call).
      - `brainiac-gateway`: `Stage::Compose` + `BRAINIAC_MODEL_COMPOSE`.
      - Tests: 8 unit (firewalls) + **6 Postgres integration** — org page
        cannot see a team-private memory; a resolved contradiction propagates
        to the page with nobody editing it; pinned prose survives regeneration;
        first-revision-needs-a-human then additive auto-publish; `detail_md`
        reaches the page verbatim; a fabricated citation cannot auto-publish.
        Full workspace suite green.
- [x] **KB1 gate CLOSED — the `docs` eval profile** (2026-07-14)
      - `fixtures/v1/documents/pages.yaml`: composition gold per EVAL §2.6. Two
        pages over the Meridian corpus — an org `entity_page` for psp-gateway
        whose corpus deliberately contains team-private and personal-private
        landmines, and a topic page whose only claim is `in_flight` (so the
        page must MARK it unshipped, not state it as architecture).
      - `brainiac-fixtures`: doc gold schema + referential validation. The leak
        list gets its own rule: a typo'd memory id there would make the
        zero-tolerance gate pass VACUOUSLY — checking that a memory which does
        not exist never appears — which is worse than having no gate, because it
        reports a safety it never verified.
      - `brainiac-eval::docs_profile` + `brainiac eval --profile docs`
        (real provider required — a mock composer cites perfectly by
        construction). Three findings are absolute build failures, not scores:
        a leaked forbidden memory, an altered pinned section, a page that
        failed to pick up a superseding belief. Coverage and hallucination rate
        are soft-gated at RATE_DELTA 0.15 against `results/docs-baseline.json`.
      - Leak detection is belt-and-braces: a forbidden memory fails the run if
        its id is in the provenance closure OR if its content appears in the
        prose semantically — a model can leak a fact by paraphrasing it without
        ever citing it, and id-checking alone would call that page clean.
      - **RESULT on real qwen-max** (`results/kb1-docs.json`): coverage 1.0
        (3/3 claims), hallucination 0.0, unshipped correctly marked 1/1,
        **leaks 0, pin violations 0, staleness failures 0, auto-published
        hallucinations 0**. Gate run against the committed baseline exits 0.
        Across two runs hallucination varied 0.0–0.06 (one uncited sentence) —
        and in both, `auto_published_hallucinations` stayed 0, i.e. the citation
        firewall held the revision in review rather than publishing it. That is
        the design working, not luck.
      - Two real bugs the profile found, both now fixed:
        1. `documents` had no RLS DELETE policy (migration 0019) — every delete
           silently affected zero rows, the classic RLS failure mode.
        2. The profile's first version counted a human's PINNED prose as
           uncited model output, reporting a 0.133 hallucination rate for a page
           whose model output was fully cited. A metric that blames the wrong
           author is worse than no metric — it sends someone hunting a
           hallucination that never happened. Regression test added.
      - Known flakiness (pre-existing, not introduced here): the Postgres test
        binaries share one database and each truncates, so `cargo test
        --workspace` can race across binaries. `-- --test-threads=1` is green
        (29/29). Worth fixing properly — per-binary schemas or a shared lock.
- [ ] KB5 public surfaces (KB page, pitch, README, docs/KNOWLEDGE-BASE.md) —
      landed alongside KB1; honesty guard tests pin every public claim to this
      status log.
- [ ] KB2 read surfaces (console reader/editor, MCP doc tools, scaffolding)
- [ ] KB3 publishing (publisher trait, Git + Confluence, scopes, breaker)
- [ ] KB4 round-trip & hardening (edit reingestion, propagation SLA, docs
      health signals)
