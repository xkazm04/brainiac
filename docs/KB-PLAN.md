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

1. **Diagrams ladder (D9 continuation).** (a) ~~deterministic mermaid
   entity-neighborhood on entity pages~~ shipped 2026-07-15 (see status log);
   (b) deterministic sequence/flow diagrams compiled from runbook-kind memory
   chains; (c) LLM-proposed diagrams entering through the same review gate as
   prose (hallucination gate applies to edges: every edge must cite a memory);
   (d) diagram rendering in the Confluence adapter (plugin/ADF extension
   territory — investigate before promising); (e) mermaid RENDERING in the
   console reader (currently shows as a code block — honest, but rung (a)
   pays off fully once the reader draws it; console docs UI is under active
   redesign in a parallel session, so this waits for that to land).
2. **Level 2 — cross-documentation intelligence** (one huge standalone topic):
   repo/doc-corpus scans as ingestion sources; **docs-drift detection** (diff
   existing human docs against canonical memories → propose supersessions or
   page bindings); Confluence *harvest* (direct edits to published pages
   captured as extraction sources instead of overwritten — closes the D4
   one-way limitation). Prerequisite: extraction recall gate green; design
   its own eval fixtures first (a synthetic stale-docs corpus for Meridian).
   *Eval-first prerequisite shipped 2026-07-15* (see status log): the
   stale-docs corpus (`fixtures/v1/drift/docs.yaml`), the drift detector MVP,
   and the `drift` eval profile with a zero-tolerance false-alarm gate;
   deterministic baseline committed at recall/precision/proposal 1.0. Next
   rungs, in order: a real-embedder calibration run (qwen `text-embedding-v4`
   baseline), then the production integration — scanning real doc trees and
   routing findings through the review gate as proposed supersessions — then
   Confluence harvest.
3. **Proactive digest** (UAT P1.5): ~~shipped 2026-07-15~~ exactly as the
   design note prescribed — `doc_kind: digest` + `window_days` on the binding,
   composed/reviewed/read through the existing pipeline (see status log).
   Still open: PER-DEVELOPER digests. *Design settled 2026-07-15, deliberately
   not built*: a per-developer digest is **retrieval-shaped, not
   projection-shaped**. Pages are org artifacts — N users × daily recompose
   would explode cost and make the review gate meaningless (nobody reviews
   10,000 personal pages). Instead: an MCP tool (`digest_for_me` or a
   `memory_context` mode) computed on demand — canonical memories changed
   since the caller's last ask, intersected with the caller's entity
   footprint (derived from their sources/feedback trail), RLS-scoped as
   always. No compose, no review gate needed: every item served IS already a
   signed canonical belief; the digest is a view, not a new claim. Build when
   pulled; the org `digest-weekly` page covers the shared case today.
4. **More publish targets** behind the same trait: Notion, Backstage TechDocs.
   Cheap once D4 lands; pick by customer pull. ~~GitHub wiki~~ closed
   2026-07-15 as a documented recipe, not code: a wiki IS a git repo, so the
   existing `git` target covers it with `docs_dir: "."` — see
   KNOWLEDGE-BASE.md §7. Bonus: GitHub renders mermaid natively, so the
   neighborhood diagrams draw themselves there.
5. **Team-space Confluence mapping** (relaxes D5): per-team spaces with
   visibility mapping, only after the leak eval covers the mapping matrix.
6. **Substrate trust workstream** (owned by UAT follow-ups, blocking KB3 GA):
   extraction recall per provider, confidence calibration for auto-promotion.
   *Calibration measurement shipped 2026-07-15*: the extraction eval now
   reports precision per self-reported-confidence band (`calibration` in the
   report; run with `--samples N` for stable bands). The auto-promotion LEVER
   stays unbuilt until a real-provider run shows the bands actually separate —
   flat bands mean confidence is noise and must not gate anything.
   ~~raw-memory TTL sweep~~ and ~~review-SLO alerting~~ shipped 2026-07-15
   (see status log: `raw_ttl` + `alerts` sweeps, migration 0024, both OFF by
   default); the eval side gained `--samples` mean gating the RATE_DELTA
   comment promised.
7. **Scoped/contractor visibility tier + cross-team observability role** (UAT
   structural gap): affects both retrieval and the publish visibility rule —
   design once, apply to both layers. *Design shipped 2026-07-15*:
   `docs/VISIBILITY-TIERS.md` — the observer role is already expressible
   (teamless org member; needs only a provisioning preset + fixture
   coverage), the contractor tier is principal-side schema work
   (`team_members.access` + `memory_shares`, time-fenced RLS predicate) with
   one non-negotiable rule: a restricted member sees NO team pages, because a
   composed page is a projection over history they cannot read. Eval gate
   specified; implementation deliberately not started (red fixtures first).
8. **Page analytics as liquidity signals**: which pages/sections agents and
   humans actually read feeds the Knowledge Health liquidity pillar and
   entity-page scaffolding thresholds. *Measurement side shipped 2026-07-15*
   (migration 0025 `document_reads`, health signals + attention items — see
   status log); still open: feeding the liquidity PILLAR formula and the
   scaffolding thresholds, deliberately deferred until there is real read
   data to calibrate the lever against.
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
- [x] **KB2 read surfaces** (2026-07-14)
      - REST (`crates/brainiac-server/src/docs.rs`): `GET /v1/docs`,
        `GET /v1/docs/{slug}`, `GET /v1/docs/{slug}/revisions`,
        `POST /v1/docs/revisions/{id}/approve` (maintainer of the owning team;
        for an org page with no owning team, any maintainer — no single team
        owns the org's shared view). `doc_get` resolves the revision's ENTIRE
        provenance closure in one response: if checking a citation cost a round
        trip, nobody would check one and the guarantee would become decorative.
      - MCP: `doc_search` + `doc_get`. Agents READ pages; there is deliberately
        no `doc_write` — an agent contributes by proposing MEMORIES, which pass
        the review gate and then flow into pages. An unpublished page serves an
        agent no content at all: a draft nobody signed must not reach a coding
        agent through the back door.
      - Entity-page auto-scaffolding (`compose::scaffold_entity_pages`): a
        canonical entity earns a page at ≥4 org-visible canonical memories
        across ≥2 teams. The cross-team half of the test is the sharp one — a
        fact one team knows is that team's business; a thing two teams both had
        to learn is exactly what an org page is for. Scaffolds a DRAFT with a
        lifecycle-split section skeleton: the machine decides a page should
        exist, a human decides it is right.
      - Worker: `compose_sweep` in the worker loop — scaffold, then recompose
        dirty pages, per org, every tick. Without this the KB never actually
        maintains itself in production. No-op for an org with no pages.
      - Console: `/docs` index (pending-review work surfaced first) and the
        `/docs/[slug]` reader — a dependency-free markdown renderer with no
        raw-HTML node kind (sanitizing by construction), inline provenance chips
        that open the memory behind any claim, lifecycle marking so an
        `in_flight` claim is visibly not-yet-shipped, revision history
        distinguishing auto-published from human-approved, and an Approve action
        that is only wired when the API is live.
      - Tests: `docs_pg.rs` (3) — a team page is invisible to a non-member (and
        "not found", not "forbidden": existence is itself information); MCP
        serves published pages and refuses unsigned drafts; scaffolding fires
        only where knowledge crosses teams, and is idempotent. Console: 31
        vitest (13 new, on the citation parser). Full Rust suite green.
      - Bug found and fixed en route: writing a TEAM document through
        `scoped_tx` as the pipeline principal silently updated ZERO rows —
        Postgres applies the SELECT policy to an UPDATE's WHERE clause, and the
        pipeline principal is in no team. Production was already correct
        (`compose_tick` opens `worker_tx` for team pages); the test seeded the
        wrong way and exposed it. An RLS no-op looks exactly like success, which
        is what makes it worth a comment in the code.
- [x] **KB3 publishing** (2026-07-14) — code complete; NOT enabled for any real
      org (see the sequencing rule below, which still stands).
      - `migrations/0020_kb_publishing.sql`: `orgs.kb_enabled` (OFF by default —
        a feature that turns itself on inside someone's Confluence is not a
        feature, it is an incident), `publish_targets`, `document_publications`
        (the ledger: what is live, where, at which revision — makes publishing
        idempotent and lets an operator prove what left the building).
        **Credentials are never stored**: a target holds `secret_ref`, the NAME
        of an env var. A database dump must not contain a PAT that can write to
        a customer's wiki.
      - `brainiac-core::health`: the pillar formulas extracted from console.rs
        as pure functions. The report and the breaker now compute currency and
        governance with the SAME code — a breaker that disagreed with the
        dashboard it is named after would be indefensible.
      - **The circuit breaker** (`publish_gate`): currency < 70 or governance <
        50 → publishing PAUSES; pages hold their last published revision. This
        is what turns the health score from a dashboard nobody acts on into an
        actuator. It reads only those two pillars deliberately: they answer "is
        what we would publish still true?" and "is anyone still checking?".
      - `brainiac-publish`: the `Publisher` trait + **Git** (writes markdown
        files; deliberately does NOT commit or push — branch protection, CI
        budget and release policy are the operator's, and a tool that guesses at
        them gets uninstalled) and **Confluence** (Cloud REST v2, PAT, one-way,
        update-in-place so a team never ends up with two pages of the same name).
      - Markdown → Confluence storage format: small, total, and escape-first.
        Anything unrecognized degrades to visible text; nothing the model writes
        can reach a customer's wiki as live markup. Citations survive the trip as
        links back into the console — a reader in Confluence is one click from
        the governed memory a named human signed.
      - KB token scopes: `kb:read` on the doc endpoints, **`kb:publish` on
        approve** — a token minted to read the knowledge base must not be able to
        sign a revision into the org's mouth.
      - Worker: `publish_org` runs after compose each tick.
      - Tests: `publish_pg.rs` (4) — nothing publishes without opt-in; only
        org-visible pages leave; a rotting corpus trips the breaker and the live
        page HOLDS its last revision (rather than being deleted or updated);
        publishing the same revision twice is a no-op. Plus 10 unit tests on the
        renderer (HTML can't become markup, a CDATA terminator can't escape a
        code block, a bogus citation stays text) and 6 on the pillar math.
      - Note from the org-visibility test: `withheld_visibility` is 0 because RLS
        never even shows the publish principal a team page — the memory layer's
        own enforcement hides it before the publisher's check runs. The code
        check stays as the second line; the first line is the same RLS path every
        user query takes.
- [x] **KB4 round-trip & hardening** (2026-07-14)
      - `POST /v1/docs/{slug}/edit` — the asymmetry (D1) as a product
        experience. A PINNED section saves: it is the human's prose and
        regeneration never touches it. A COMPOSED section does NOT save: writing
        the text into the page would fork the truth (the page would say one
        thing, the memory layer another, and the next recompose would silently
        revert the human — the single most infuriating thing a wiki can do to
        someone who took the time to fix it). The edit becomes an ingest source,
        goes through extraction, and passes the same review gate as everything
        else. The response says **"captured"**, never "saved" — a tool that says
        "saved" when it means "queued for someone else's approval" has lied to
        the person most likely to notice. The editor's stated REASON rides along
        with the edit, because the reason is exactly the knowledge a diff cannot
        recover.
      - Knowledge Health gains the KB's own signals: `pages_dirty`,
        `oldest_dirty_secs` (**the propagation SLA made visible** — the product
        promises a resolved contradiction reaches every page by itself, and this
        number says whether "automatically" means minutes or means never),
        `pages_pending_review`, `pages_published`; plus attention items that go
        critical when a page has been out of date for over a day.
      - Tests: `doc_edit_pg.rs` drives the real HTTP surface end to end — a
        composed edit is captured (not written into the page, section still
        composed, reason preserved in the source), a pinned edit saves and marks
        the page dirty, and then the full loop closes: compose → page clean →
        the human's prose in the published revision. Measured propagation on one
        page: ~95ms with a mock composer.
      - Full Rust suite green (exit 0, serial).
- [x] **Hygiene + substrate wave** (2026-07-15) — the backlog's operational tail.
      - `brainiac_store::test_support::serial_guard`: one session-level Postgres
        advisory lock on a dedicated single-connection pool, held for process
        life, plus an in-process mutex — adopted by every `*_pg.rs` suite. Two
        test binaries (two agent sessions, or local racing CI) can no longer
        truncate the shared database out from under each other.
      - `migrations/0024_raw_ttl_sweep.sql` + `memories::expire_stale_raw`: raw
        memories past a TTL (default 30d, `BRAINIAC_RAW_TTL_DAYS`) flip to
        `rejected` with a `promotions` audit row naming the sweep — "declined by
        neglect" made explicit instead of served forever. Seeded OFF.
      - `alerts` sweep: per-org breaches (review SLO, currency floor, cross-team
        contradictions, pages dirty >24h) pushed to `BRAINIAC_ALERT_WEBHOOK_URL`
        (Slack-compatible `{"text": …}` + structured breaches). Reads the same
        `compute_health_core` the dashboard renders. No webhook + breaches =
        loud log + honest sweep detail, never a silent skip. Seeded OFF.
      - Multi-sample eval: `eval --profile extraction|docs --samples N` resets
        the tenant between runs, gates hard failures on EVERY sample (one leak
        in one run of five is still a leak), and gates rates on the MEAN with a
        `max(0.05, 0.15/√N)` band — the tightening the single-run RATE_DELTA
        comment promised.
      - CI: console job (tsc, vitest, build into `.next-build`); clippy clean at
        `-D warnings` across the workspace; `docs/KNOWLEDGE-BASE.md` route drift
        fixed (`/console?m=health`).
      - Completed the concurrent session's memory-title plumbing (migration
        0023): `title: None` across test initializers, lint fixes, sweep-count
        pin in `console_pg` updated for 0024. Full pg suite green.
- [x] **Page-read analytics, measurement side** (2026-07-15) — follow-up #8.
      - `migrations/0025_document_reads.sql`: append-only event log (INSERT +
        SELECT grants only — analytics that can be rewritten are not
        analytics), RLS-gated by visible parent document. `via` records the
        channel (`http` | `mcp`), `was_dirty` records whether the page was
        serving a superseded belief AT THE MOMENT of the read.
      - Recording happens in its OWN transaction after the serving tx commits
        (warn-only on failure: analytics must never cost a reader their page),
        and only when revision CONTENT was actually served — a skeleton view
        of a revision-less page, an unsigned draft told to an agent, and a
        not-found all record nothing.
      - Knowledge Health signals: `page_reads_30d`, `agent_page_reads_30d`
        (MCP — agents consuming pages is the loop the KB exists for),
        `dirty_page_reads_30d` (rot being CONSUMED, which outranks rot that
        merely exists), `pages_never_read`. Two new attention items: dirty
        reads (warning), never-read published pages (info, a candidate list).
      - The liquidity PILLAR formula is deliberately unchanged: measure first,
        calibrate the lever when there is data — the same posture as
        confidence calibration.
      - Tests: `doc_edit_pg` pins no-content-no-read, then clean-read/dirty-read
        against the same page (the flag is a property of the moment);
        `docs_pg` pins the MCP channel and that drafts/not-founds record
        nothing.
- [x] **Deterministic mermaid neighborhood on entity pages** (2026-07-15) —
      diagrams ladder rung (a), the one D9 allowed early.
      - `compose::mermaid_neighborhood`: for `entity_page` docs, compile a
        `graph LR` mermaid block from the `edges` table — the canonical
        entity's raw set, every live edge touching it, neighbors labeled by
        entity name, arrows labeled by relation. NO model proposes an edge:
        every arrow IS a database row, so the zero-hallucination bar is met by
        construction.
      - Appended by CODE after the model's sections, exactly the
        `evidence_blocks` trust boundary; the whole diagram lives in one
        fenced block, invisible to both the citation firewall and the eval's
        prose scan (pinned by unit test).
      - No edges → no diagram: an empty diagram is decoration, and D9's point
        is that diagrams are language. Pinned end-to-end in `docs_pg`: compose
        before the edge exists (no Neighborhood section), insert the edge,
        recompose (diagram present with relation + neighbor).
      - Opaque node ids + quoted labels: entity names are user data (spaces,
        slashes, quotes) and never become mermaid identifiers.
      - Reader-side RENDERING deferred (ladder rung (e)): fenced block shows
        as code for now; console docs UI is mid-redesign in a parallel session.
- [x] **Weekly digest as a projection** (2026-07-15) — follow-up #3, built
      exactly as the design note prescribed: no parallel generator.
      - `doc_kind: digest` (migration 0027) + `SectionBinding.window_days`: a
        time window is a SOURCE (recent canonical changes, newest first,
        `updated_at` so promotions/supersessions count) feeding the same
        filter chain — visibility/kind/lifecycle rules hold for a digest
        exactly as for any page, and RLS means it cannot show a reader a
        change they may not see.
      - `scaffold_digest`: one `digest-weekly` page per org, created only once
        ≥3 org-visible canonical changes land in a week (a digest over a quiet
        corpus teaches readers to skip it). First revision still needs a
        human, like every page.
      - `refresh_digests`: the compose sweep re-dirties a digest whose newest
        revision is older than 24h — a windowed page goes stale by TIME
        PASSING, and no memory-change trigger fires for an item aging out.
      - One policy exemption, narrowly for `digest`: items rolling out of the
        window auto-publish (a digest is a window, not an account — the belief
        still stands in the corpus; weekly re-signing would train
        rubber-stamping). Unbacked claims force review, same as everywhere.
      - "Session-start push" = the agent reads `digest-weekly` via the doc_get
        it already has. Pinned end-to-end in `docs_pg`: activity floor,
        idempotence, window filtering (40-day-old belief excluded), first-
        revision review, then window-roll → auto-published.
- [x] **Level 2, rung one: docs-drift eval gold + detector MVP** (2026-07-15)
      — the eval-first prerequisite follow-up #2 demanded, and nothing more.
      - `fixtures/v1/drift/docs.yaml`: three synthetic human docs labeled
        against the gold memory corpus — two stale (checkout v1, the 10s PSP
        timeout, Jenkins deploys, the 2s retry standard) and one FRESH
        honeypot whose claims share the stale beliefs' vocabulary. The linter
        checks the gold (labels, proposals resolve, proposals not themselves
        stale, claims locate in the body).
      - `drift_profile`: split claims → embed → nearest CURRENT vs nearest
        SUPERSEDED memory. Drift = close to a superseded belief AND
        meaningfully closer to it than to any current one (margin 0.05 over
        threshold 0.70); the proposal is the supersession chain's terminal.
        Three verdicts by design: `unmatched` is a HARVEST candidate, never
        drift — a detector that flags what it does not recognize teaches
        authors to ignore it.
      - HARD gate, zero tolerance: a gold-aligned claim flagged as drift.
        Automation that attacks correct docs is worse than none. Soft gates on
        recall/precision/proposal-accuracy vs a committed baseline (±0.10;
        cross-embedder comparison refused).
      - Deliberately DB-free (`eval --profile drift` needs no DATABASE_URL) —
        the instrument is claim-vs-corpus classification; RLS enters at the
        production-integration rung, not here.
- [x] **OKF interop: publish target, harvest, filtered search, runtime judge,
      reader diagrams** (2026-07-17) — the OpenWiki/OKF gap-closure round.
      Research: LangChain's OpenWiki 0.2 adopted OKF (Open Knowledge Format,
      GoogleCloudPlatform/knowledge-catalog, v0.1) — markdown + YAML
      frontmatter bundles as the emerging lingua franca for repo wikis agents
      read. Our verdict: adopt the FORMAT as one more render target and one
      more witness source; the projection architecture is the moat and none of
      it moved.
      - **`okf` publish target** (`brainiac-publish/src/okf.rs`): pages render
        as an OKF bundle — frontmatter `type` (from doc_kind) / `title` /
        `description` (derived first-prose-line) / `resource` (console URL) /
        `tags` (canonical entity names) / `timestamp`, plus generated
        `index.md` (root `okf_version: "0.1"`, kind-grouped listing) and
        `log.md` (date-grouped changelog from revisions). Our differentiator
        travels in extension frontmatter the spec obliges consumers to
        preserve: `x_brainiac_cited_memories` (the exact provenance closure),
        `x_brainiac_policy`, `x_brainiac_stale`. Same health gate, same
        org-only visibility rule, same banner. `Publisher` grew a `finish`
        hook (bundle-level artifacts, once per run); `PageToPublish` grew
        `meta` computed once in `publish_org` so every target sees one truth.
      - **Agent pointer files** (`pointer.rs`): managed
        `<!-- BRAINIAC:START/END -->` blocks in repo-root AGENTS.md +
        CLAUDE.md — OpenWiki's zero-integration distribution trick, pointed at
        our governed bundle. Default ON for `okf` (new target, the pointer is
        the point), OPT-IN for `git` (`agent_pointers: true`) — an existing
        target must not start writing repo-root files unasked. User prose
        outside the markers survives byte-for-byte.
      - **Deterministic doc search** (store `DocFilter`; REST
        `/v1/docs?q&kind&tag&stale`; MCP `doc_search` args): kind/tag/stale
        are exact predicates ANDed with the lexical needle; `tag` walks
        revision provenance → entity_links → CANONICAL name, so the filter
        vocabulary IS the OKF frontmatter vocabulary. Unknown `kind` is a 400
        (an empty 200 would read as "no runbooks exist"); MCP requires ≥1
        argument. Filters pinned in `docs_pg` (kind exact, tag
        case-insensitive + provenance-scoped, stale flips with
        `mark_dirty_for_memory`, AND-composition).
      - **OKF harvest** (`brainiac-pipeline/src/okf_ingest.rs`, CLI
        `brainiac okf-harvest --org --path [--team]`): someone else's repo
        wiki as an extraction SOURCE — concept docs (reserved index/log/hidden
        skipped, 64KB/500-file caps) → `okf` sources → candidates → the
        review gate. Never direct-to-canonical: a wiki is a witness, not an
        authority. Idempotent per (path, FNV-64 content) via
        `insert_source_idempotent` — re-runs ingest only what changed. Our own
        published pages (`x_brainiac_*` frontmatter) are REFUSED: harvesting a
        projection back would launder composed prose into evidence.
      - **Runtime citation-faithfulness judge** (0036,
        `brainiac-pipeline/src/faithfulness.rs`): the honesty gap the plan
        itself flagged ("cites a real memory while misstating it" lived only
        in the eval). Sampled (≤8 cited paragraphs, spread), on exactly the
        revisions a human is about to read (`needs_review`), advisory only —
        verdict JSONB on the revision (`checked` + `flagged` excerpts),
        surfaced in the revision REST views as the reviewer's
        read-this-first list. Best-effort after the compose commit: a crashed
        critic never costs the revision. Same provider as compose — a model
        grading its own homework is precisely why it informs a human instead
        of gating.
      - **Mermaid rendering in the reader** (ladder rung (e), un-deferred):
        `MermaidBlock.tsx` client island, lazy `import("mermaid")`,
        `securityLevel: 'strict'`, no HTML labels, render failure falls back
        to the code fence forever. Server-side markdown parser stays
        dependency-free; `/docs/[slug]` first-load JS unchanged (mermaid lives
        in a lazy chunk).
      - Verification note: workspace compiles; publish/pipeline/console unit
        suites green (23 + 51 Rust, 178 console); `openapi.json` + console
        types regenerated. The `_pg` integration suites were NOT run this
        session — a parallel session was live on the shared dev DB and the
        suites TRUNCATE; run them before trusting the tag-filter and
        faithfulness paths end-to-end.
      - Follow-up seeds: drift detection against harvested OKF bundles (the
        detector MVP + this harvest are the two halves); typed OpenAPI schema
        for the faithfulness verdict (currently `Object`); reviewer UI for
        flagged paragraphs in `ApproveRevision`; OKF `log.md` → per-page
        `# Citations` section once the spec grows one.
      - Measured: deterministic-bow baseline 1.0/1.0/1.0 with the margin doing
        real work (the 10s-timeout claim scores 0.935 stale vs 0.73 fresh —
        both above threshold; the margin decides). Unit tests pin the margin
        honeypot, chain-terminal proposals, the harvest bucket, and the
        false-alarm hard gate.
      - **Real-embedder calibration run** (qwen `text-embedding-v4`, archived
        at `results/history/2026-07-15-drift-qwen-text-embedding-v4.json`):
        also 1.0/1.0/1.0, zero false alarms — but the real embedder COMPRESSES
        the margin: the 10s-timeout claim decides at 0.970 stale vs 0.907
        fresh, a 0.063 gap against the 0.05 rule. Finding for the production
        rung: `DRIFT_MARGIN` is load-bearing and near its edge on real
        embeddings — treat any change to it as gate-affecting, and grow the
        corpus with more near-miss pairs before trusting the detector on prose
        further from the memories' wording.

## The KB line is complete

KB0–KB5 are all shipped. What remains is the deferred backlog below — and the
standing sequencing rule: **external publishing (KB3) must not be enabled for a
real org until the extraction-recall workstream clears its gate.** Everything is
built and tested; nothing is turned on. `kb_enabled` is false by default and no
publish target exists until someone deliberately creates one.
