# Brainiac — Library Layer Plan (v0.6 line)

Goal: implement the third module group of the product — the **Library**: coding
standards per tech stack and skills for LLM development, as governed,
versioned, provenance-carrying artifacts with a programmatic distribution
surface for coding agents. This plan extends `docs/PLAN.md` (baseline,
complete) and `docs/KB-PLAN.md` (KB line, complete) and follows their
conventions: `docs/ARCHITECTURE.md` stays the contract, each phase ends with
green tests + one commit, deviations are recorded here.

Design settled 2026-07-15 (session discussion). The one-line thesis:

> Memories are the descriptive layer (what is true), the knowledge base is the
> compiled layer (what we know, assembled), and the Library is the **normative
> layer** (what we should do). A Library artifact is a governed, versioned rule
> or skill whose provenance points back into canonical memories — and its
> anti-rot mechanism is not composition but **telemetry**: adoption and
> divergence per rule, usage per skill. A standard nobody follows and a skill
> nobody invokes go visibly red, exactly as a stale page does.

## Settled design decisions

| # | Decision | Rationale / boundary |
|---|---|---|
| L1 | **The rule is the atom, not the standards document.** A Standard is one rule: scope (tech stack) → category → rule, with a one-sentence statement (mirrors memory `content`), rationale, good/bad examples in `detail_md` form, enforcement level (`mandatory \| recommended \| experimental`), lifecycle (`proposed \| adopted \| deprecated`), and provenance links to the memories/incidents that motivated it. Deep-linkable, individually versioned, individually measurable. A "stack overview" is a projection over rules, not a stored document. | A monolithic standards doc is the wiki failure mode again: unmeasurable, staleness invisible. Per-rule telemetry (L5) is impossible without per-rule identity. Rejected: standards as free-form KB pages — normative artifacts need typed fields (enforcement, lifecycle) that prose cannot gate on. |
| L2 | **One intake, two sources.** Passive mining (sweeps over memories, feedback, unmatched divergences) and active proposal (agents pushing patterns during development) are both *source kinds* feeding the same candidate → triage → named-human promotion gate that governs everything else in the product. Neither source can create an adopted standard; only the gate can. | Resolves the "aggregate-and-triage vs. agents-propose" question by refusing it: the architecture cost of supporting both is one enum variant, and the governance story stays single. Active proposal is deliberately LAST (L3) — it is the noisy channel and needs the dedup corpus the other phases build. |
| L3 | **Distribution before contribution.** Phase order is: artifacts + read API + usage telemetry (LB1) → console surfaces (LB2) → passive mining (LB3) → active proposal (LB4). | The user-visible value ("agents pull org standards and skills programmatically") ships with zero new extraction risk, and telemetry starts accumulating on day one — mining and dedup both need it. Extraction recall on curated transcripts is ~0.46 (KB-PLAN D8); pointing generators at the library before the triage instrument exists repeats the mistake KB avoided. |
| L4 | **Skills adopt the open agent-skill bundle format** (a `SKILL.md` manifest + resources — the format Claude Code and compatible agents already load), not a bespoke schema. Brainiac stores, versions, governs, and serves the bundle; it does not invent the format. | The consumers are coding agents that already speak this format; a bespoke one adds a translation layer with no information gain. Versioned as semver bundles; content-addressed blobs in the store, exportable through the existing `Publisher` git target later. |
| L5 | **Telemetry is the anti-rot mechanism.** `library_usage_events` records skill fetches/invocations and standard checks, **aggregated per team, never per person** (see Never-list). Per-rule adoption + open-divergence counts and per-skill usage feed Knowledge Health as library signals; a dead standard or unused skill surfaces as a deprecation candidate automatically. | The KB's anti-rot is recomposition; a normative layer cannot recompose its way to honesty — the only test of a rule is whether practice follows it. This is what turns the Library from a folder of opinions into an instrument. |
| L6 | **Divergence is the front door.** `practice_divergences` already computes `recommended_standard` per divergence (migration 0016, console standards board). Ratifying a divergence creates a **standard candidate** in Library triage carrying the divergence as provenance; an adopted rule back-references its divergences so future sweeps read as enforcement signal, not new discovery. | The standards board shipped in the org-intelligence milestone is a proto-Library surface: keep it as the *detector* view and let the Library own the *artifact*. No second divergence pipeline. |
| L7 | **Library scoping at token level**: scopes `lib:read`, `lib:propose`, `lib:publish` on API tokens (same model as `kb:*`, KB-PLAN D6). No org capability flag in v1: nothing in the Library leaves the building (no external publish target), so there is no blast radius to gate. | `lib:read` is what an agent's token carries; `lib:propose` stays unminted until LB4; `lib:publish` (ratify/adopt/deprecate) is the maintainer scope — a token minted to read standards must not be able to decree one. |
| L8 | **Standards may later render as composed KB pages** (a "TypeScript standards" page projected from adopted rules, publishable through the existing KB pipeline including Confluence). Deferred — nearly free once both layers exist, and it inherits every KB gate unchanged. | Cherry-on-top tier, same as the KB diagrams ladder. Not v1 because it multiplies surfaces before the artifact layer has telemetry. |

## The Never-list (product commitments, testable)

1. **Never a leaderboard.** Usage telemetry aggregates per team; no endpoint,
   export, or console surface attributes standard-compliance or skill usage to
   an individual. Telemetry that can rank people gets gamed and then feared,
   and the signal dies with the trust.
2. **No agent writes a standard or skill directly.** Agents propose; the gate
   decides. Same asymmetry as pages (KB-PLAN D1), same reason.
3. **No auto-enforcement in v1.** The Library informs (agents fetch rules and
   self-check); it does not block merges or fail builds. Enforcement hooks are
   a follow-up with its own consent design — a rule engine that gates CI is a
   different product with a different failure mode.
4. **No unattributed rules.** Every adopted rule carries either provenance
   (memories, incidents, divergences) or an explicit `decreed` marker naming
   the human who ratified it without evidence. A rule that cannot say why it
   exists cannot ask anyone to follow it.

## Architecture deltas (fold into ARCHITECTURE.md during LB0)

1. New §10 "Library layer": the normative-layer thesis, the artifact model
   (L1, L4), the one-intake rule (L2), telemetry aggregation rule (L5), and
   the divergence bridge (L6).
2. §2.3: note that `detail_md` (KB-PLAN D3) is reused verbatim as the
   example-payload format on standard versions — no new rich-text mechanism.
3. Token scopes table gains `lib:read` / `lib:propose` / `lib:publish` (L7).
4. §9 roadmap: add the Library line, pointing here.

## Phase ladder

| # | Phase | Deliverable | Test gate |
|---|---|---|---|
| LB0 | Substrate | Migration 0028: `standards`, `standard_versions`, `standard_provenance`, `skills`, `skill_versions`, `library_usage_events` + RLS + `brainiac_app` grants; core domain types (`Standard`, `Rule` lifecycle/enforcement enums, `SkillBundle`); store repos under the caller's RLS transaction; the L6 bridge — ratify a divergence → standard candidate with provenance; ARCHITECTURE deltas folded in | `store_pg` suite extended: RLS isolation per org on every new table; bridge test — ratifying a Meridian divergence yields exactly one candidate carrying the divergence id as provenance; full workspace green |
| LB1 | Distribution + telemetry | REST: `GET /v1/library/standards?stack=`, `GET /v1/library/standards/{id}`, `GET /v1/library/skills`, `GET /v1/library/skills/{slug}` (+ `/download` bundle), `POST /v1/library/usage`; MCP tools: `standards_for(stack, category?)`, `skill_search(query)`, `skill_fetch(slug, version?)`, `skill_report_usage`; token scopes wired (`lib:read` on reads, `lib:publish` on adopt/deprecate/ratify); usage events aggregated per team at write time | new `library_pg` integration suite: a non-member token cannot read another org's rules ("not found", not "forbidden"); a `lib:read` token cannot adopt; usage events carry team, never principal id (schema-level test); MCP tools serve only `adopted` rules and published skill versions to agents |
| LB2 | Console surfaces | New Library station in the console (two modules under the existing `(modules)` structure): **standards** — tree rail stack ▸ category ▸ rule, rule detail (statement, rationale, examples, provenance chips opening the memories behind the rule, adoption + divergence sparkline, lifecycle chip, version history), triage queue reusing the promotions-review pattern; **skills** — faceted catalog (domain, maturity, usage-ranked), skill detail (rendered manifest, versions, per-team usage graph, download/API snippet); nav + routes registry + station module on home | console vitest: routes registry consistency, triage state machine, tree building from flat rule lists; demo fallback for both modules so `/demo` can tour the Library |
| LB3 | Passive mining | `library_sweep` job kind on the existing SKIP-LOCKED queue: candidates from (a) unmatched practice divergences, (b) corrective-feedback clusters in `memory_feedback`, (c) resolved contradictions whose resolution states a convention; dedup against existing rules AND previously rejected candidates (rejection is knowledge); candidates land in LB2 triage | new `library` eval profile over Meridian: seeded divergences/feedback yield candidates with measured precision (soft-gated vs. baseline like every profile); **a rejected candidate never reappears within the dedup window — hard gate**; sweep is a no-op on an org with no signal |
| LB4 | Active contribution | `lib:propose` goes live: MCP `standard_propose` / `pattern_report` (or `memory_add` with a `practice` kind — decide against fixtures), rate-limited per token, deduped against corpus + open candidates before touching triage; agent session flow: fetch `standards_for(stack)` at start, report divergence at end | `library_pg` extended: a proposal is a candidate, never an adopted rule; rate limit enforced; duplicate proposal collapses onto the open candidate (no triage spam); eval: proposal→candidate precision measured on scripted agent sessions |
| LB5 | Public surface (runs FIRST, alongside LB0) | `/library` explainer page in the `/kb` mold: same visual system (band hues, SectionRail, deterministic SVG figures), same two data-file rules — **honesty** (every capability stamped `shipped \| built_off \| in_progress \| roadmap`, pinned to THIS status log by tests that fail the build on drift in either direction) and **audience** (no repo coordinates in visitor-facing text); wired into the public nav + middleware allow-list | console vitest: the honesty-guard suite for library-data mirrors the KB one; every stamp traceable to this status log |

Sequencing rules:

- **LB5 ships first** (it is this session's deliverable together with this
  plan) with everything except the divergence substrate stamped `roadmap` —
  the page must not describe LB0–LB4 as anything but intent until their
  status-log entries flip.
- **LB4 does not ship until LB3's dedup gate is green.** Active proposal
  without working dedup floods triage and burns maintainer trust — the KB
  learned this lesson at the extraction layer; the Library inherits it at the
  intake layer.

## Crate/dir map (delta)

```
crates/
├── brainiac-core       # + Standard/Skill domain types, enforcement + lifecycle enums
├── brainiac-store      # + library.rs: standards/skills/usage repos, RLS
├── brainiac-pipeline   # + library_sweep job kind (LB3), candidate dedup
├── brainiac-eval       # + library profile (LB3/LB4 gates)
└── brainiac-server     # + /v1/library REST, MCP standards_for/skill_* tools, lib:* scopes
console/
├── app/library/        # NEW (LB5): public explainer — layout + page
├── src/library/        # NEW (LB5): Library.tsx, library-data.ts (+ honesty tests), illustrations
└── app/console/...     # LB2: standards + skills modules under the modules structure
fixtures/v1/library/    # LB3: mining gold (seeded divergences/feedback → expected candidates)
migrations/             # 0028 library substrate (0025-0027 were taken by parallel workstreams)
```

## Deviations from ARCHITECTURE.md (deliberate, revisitable)

1. **Queue**: `library_sweep` rides the existing v0 SKIP-LOCKED `queue`
   schema (same deviation #1 as PLAN.md and KB-PLAN.md).
2. **Policy**: candidate promotion uses the typed-Rust `PolicyEngine`, not
   Cedar (same deviation #2).
3. **Skill storage**: bundles live in Postgres as content-addressed blobs in
   v1, not object storage — same store, same RLS, same backup story; revisit
   if bundles outgrow row-size comfort.

## Follow-ups & next directions (deferred backlog — future sessions start here)

Ordered roughly by leverage; none block the phase ladder.

1. ~~**Standards as composed KB pages (L8).**~~ **DONE 2026-07-15** — but
   *projected*, not *composed*; see the status log. The one design change
   worth carrying forward: a normative artifact must never be re-worded by a
   model, so this page is the first `doc_kind` that calls no LLM at all.
2. ~~**Library pillar in Knowledge Health.**~~ **DONE 2026-07-15** — as
   signals + attention items, deliberately NOT as a fifth pillar in the
   composite (see the status log for why).
3. **Enforcement hooks (relaxes Never #3).** Opt-in CI annotation (report,
   then warn, never silently block at first); needs its own consent design and
   a false-positive budget before any org turns it on.
4. **Skill eval harness.** Scripted tasks per skill measuring whether the
   skill actually improves agent outcomes; "verified on N tasks" as a maturity
   stamp on the catalog card.
5. **Cross-org / public library exchange.** Publishing selected standards or
   skills outside the org (the first Library feature with a blast radius —
   this is where an org capability flag and the leak eval enter).
6. **Per-rule adoption measurement from repo signal.** Static scan of org
   repos against machine-checkable rules — Level-2 territory (KB-PLAN
   follow-up 2 is the same class of work); design its own fixtures first.
7. **Skill composition from memories.** Mining runbook-kind memory chains
   into *draft* skills (the generative sibling of LB3); gated on the same
   extraction-recall workstream as everything generative.

## Status log

- [x] **Plan settled** (2026-07-15) — this document; design discussion in
      session (normative-layer thesis, rule-as-atom, one-intake/two-sources,
      distribution-first sequencing).
- [x] **LB5 public surface** (2026-07-15)
      - `/library` explainer: `console/app/library/` + `console/src/library/`
        — the seven moves (drift, third layer, anatomy, life-of-a-rule rail,
        agent surface, refusals, ladder), deterministic SVG figures, theta as
        the Library's band.
      - Honesty guard (`library-data.test.ts`): every stamp pinned to THIS
        status log; audience rule (no repo coordinates); only the drift
        detector may claim to run; no flow into an adopted rule without a
        named human; the never-a-leaderboard promise is a test.
      - Wired into the landing-page public nav and the middleware allow-list;
        context map updated. Console vitest suite green (102), `tsc` clean,
        production build prerenders `/library` static.
- [x] **LB0 substrate** (2026-07-15)
      - `migrations/0028_library_substrate.sql` (0025–0027 were claimed by
        parallel workstreams mid-session — document_reads, feedback notes,
        digest kind): `standards`, `standard_versions`, `standard_provenance`,
        `skills`, `skill_versions`, `library_usage_events`, org-scoped RLS +
        `brainiac_app` grants on all six.
      - The two schema-owned invariants: **attribution** — a DEFERRED
        constraint trigger refuses any standard leaving `proposed` without
        provenance rows or `decreed_by` (checked at commit, so same-tx
        provenance counts; `adopt_standard` forces it IMMEDIATE to fail at the
        statement); **no leaderboard** — `library_usage_events` has a team
        column and NO user column, verified by a schema-shape test.
      - `brainiac-core`: `Enforcement`, `StandardLifecycle`,
        `StandardProvenanceKind`, `SkillMaturity`, `LibraryArtifactKind`,
        `LibraryUsageEvent` (+ round-trip tests pinned to the migration's
        CHECK lists), `Standard`, `StandardProvenance`, `Skill`,
        `SkillVersion`.
      - `brainiac-store::library`: insert/get/list standards (serve path
        filters `adopted` only), provenance, adopt (evidence or decree —
        the database refuses otherwise), deprecate (one-way, records the
        retiring human), skills + draft versions + `publish_skill_version`
        (a named human; drafts are never served), `record_usage` (cannot
        take a user id), `usage_by_team`.
      - **The L6 bridge** `ratify_divergence`: divergence → exactly one
        `proposed` standard carrying the divergence as provenance
        (idempotent by provenance, slug-collision-safe, RLS-invisible
        divergences answer "not found").
      - ARCHITECTURE.md §10 added (thesis, invariants, bridge, scopes).
      - Gate: `store_pg` extended with 2 integration tests — the cross-org
        isolation matrix over all six tables incl. WITH CHECK smuggling
        refusal, and the bridge/attribution/serve-path story. Both green on
        live Postgres.
- [x] **LB1 distribution + telemetry** (2026-07-15)
      - REST (`crates/brainiac-server/src/library.rs`): `GET
        /v1/library/standards` (adopted by default; `?stack=`,
        `?lifecycle=proposed|adopted|deprecated|all`), `GET
        /v1/library/standards/{id}` (with the provenance behind the rule),
        `POST .../adopt` (409 with a decree hint when the schema refuses an
        evidence-free rule), `POST .../deprecate`, `POST
        /v1/library/divergences/{id}/ratify` (the L6 bridge over HTTP,
        idempotent), `GET /v1/library/skills(/{slug})(/download)`, `POST
        /v1/library/usage`. All registered in OpenAPI; `openapi.json`
        regenerated (54 paths).
      - Scopes wired: `lib:read` on every read + usage report; `lib:publish`
        on adopt/deprecate/ratify. `lib:propose` stays unminted (LB4).
      - MCP tools: `standards_for` (adopted rules only — a proposal never
        reaches an agent as policy; serving records the fetch per rule),
        `skill_search` (published only), `skill_fetch` (published bundle or
        nothing — same refusal as unsigned pages), `skill_report_usage`
        (check/apply only; fetches are counted server-side so an agent cannot
        inflate them). Usage recording is warn-only in its own tx — vital
        signs never cost an agent its answer.
      - Team attribution at every serving site: `principal.team_ids.first()`;
        the schema still cannot name a person.
      - **Refactor (post-LB0, per session feedback):** no god files.
        `brainiac-store::library` split into `standards/bridge/skills/usage`
        modules (33–230 LOC); Library domain types moved out of `types.rs`
        into `brainiac-core::library` (283 LOC; `types.rs` back to 880); the
        `/library` page decomposed into `sections/` + `figures/` +
        `primitives.tsx` with `Library.tsx` a 51-line running order.
      - Gate: new `library_pg` suite (2 tests over real HTTP + MCP JSON-RPC):
        cross-org reads are "not found" never "forbidden"; a `lib:read` token
        cannot ratify or adopt (403); adopting an evidence-free rule is 409
        until decreed; the divergence bridge is idempotent over HTTP; a draft
        skill serves nothing anywhere; usage events carry the caller's team
        on every path. Page stamps flipped (LB1, Adopt/Serve stations,
        atom/provenance/skills properties, agent surface) with the honesty
        guard updated to pin the new truth in both directions.
- [x] **LB2 console surfaces** (2026-07-15)
      - LB2 server delta first: rule detail now carries its provenance, the
        per-team pulse (`usage_named` — team names resolved, still no person
        to resolve), and version history; skill detail carries versions
        (drafts visible, marked, never served) + pulse. OpenAPI regenerated;
        console types generated from it (`gen:api`), so the surfaces cannot
        drift from the handlers.
      - **Standards board** (`console/app/console/modules/standards/`): the
        flat rules compile into a stack ▸ category ▸ rule tree (`tree.ts`,
        pure + tested; proposals float to the top of every branch), rule
        detail with lifecycle/enforcement/decree chips, verbatim examples,
        provenance chips (drift vs memory), pulse bars, version history; the
        gate's controls (`triage.ts` pure state machine + `TriageControls`)
        mirror the backend exactly — adopt on proposed, retire on adopted,
        deprecated terminal, and an evidence-free adopt re-offers as an
        explicit signed decree with the consequence spelled out (the 409 path
        made humane).
      - **Skills catalog** (`console/app/console/modules/skills/`): cards
        ranked by pulse, drafts plainly marked unservable, per-team usage
        bars, version history, and the exact MCP/REST call an agent makes.
      - Both modules live/demo via `withDemoFallback` behind `DemoBanner`;
        details prefetched in one bounded burst (cap 100) so rule-hopping is
        instant; a demo board never mounts the gate.
      - Wiring: third nav group **library** in the routes registry + chrome;
        the divergence board relabeled **drift** (the Library owns the word
        "standards" for the artifact; the board is the detector); module
        bands standards=theta, skills=beta; console dispatcher entries; demo
        tour gains both modules; home page gains station 06 ("Judgment ships
        to the agents", quantized-wave figure + library station module).
      - Gate: console vitest 116 green — `tree.test.ts` (grouping, triage
        float, empty library), triage state-machine matrix (transitions
        mirror the backend; decree exactly when evidence is absent;
        deprecated is terminal), `routes.test.ts` registry consistency
        (unique segments/labels, groups non-empty, bands resolvable, library
        group hosts exactly standards+skills, public/private boundary).
        `tsc` clean, production build green, `/demo` + `/library` smoke-
        tested on a prod server.
- [x] **LB3 passive mining** (2026-07-15)
      - `migrations/0029_library_mining.sql`: `rejected` joins the standards
        lifecycle (proposed → rejected, terminal, KEPT — rejection is
        knowledge); the attribution trigger narrows to adopted/deprecated so
        an evidence-free candidate can be refused; the `library` sweep joins
        `sweep_schedules` (disabled, weekly — nothing mines by surprise).
      - `brainiac-pipeline::library_sweep`: three deterministic miners per
        org — (a) unclaimed practice divergences, (b) reinforced practices
        (canonical `pattern`/`pitfall` with ≥2 independent helpful verdicts),
        (c) convention-settling supersessions (canonical `decision`/`pattern`
        winners). Everything lands `proposed` with the signal as provenance;
        candidates carry the org's own words verbatim. Cross-org on the admin
        pool, one transaction per org, wired as sweep kind `library`
        (window: BRAINIAC_LIBRARY_DEDUP_DAYS, default 90).
      - **The dedup rule**: a signal is skipped when any standard carries it
        as provenance — unconditionally for live standards, and inside the
        window for rejected ones; past the window a signal may return as a
        NEW dated candidate (the rejected row is never resurrected). Bridge
        fix en route: `propose_from_divergence`'s idempotency shortcut now
        ignores rejected claims, so a deliberate human re-ratify (and the
        sweep, past the window) mints a fresh candidate instead of silently
        returning the rejection.
      - Reject surfaces: `POST /v1/library/standards/{id}/reject`
        (lib:publish), console gate gains "reject — and remember", triage
        state machine + tree updated (rejected is terminal and sinks to the
        bottom of every branch, visible as the dedup memory), standards
        module gains the mining SweepControl.
      - **Deviation (deliberate, revisitable):** the planned `library` eval
        profile is a deterministic Postgres gate instead
        (`library_sweep_pg.rs`). The miners are heuristic, not LLM — an eval
        profile measures model variance that does not exist here, and
        synthetic gold would make "precision" 1.0 by construction. The
        profile becomes real work when mining goes generative (follow-up 7).
      - Gate: `library_sweep_pg` — seeded signals yield exactly the expected
        candidates with their provenance; below-threshold and wrong-kind
        signals yield nothing; a re-run creates nothing; **a rejected
        candidate never reappears within the window (hard)** and returns
        dated past it; a quiet org is a no-op. `library_pg` extended with the
        reject endpoint's scope/terminality story. Full suites green.
- [x] **LB4 active contribution** (2026-07-15)
      - `migrations/0030_library_proposals.sql`: `standards.origin`
        (`human | sweep | agent`) — triage receives machine-authored
        candidates at machine speed, and a maintainer weighing trust must see
        who is asking without archaeology. A column, not a convention;
        best-effort backfill (authorless rev-1 ⇒ sweep).
      - Decided against `memory_add` with a practice kind: a dedicated
        `standard_propose` channel keeps the typed fields (stack, category,
        statement, examples) that flattening through extraction would lose.
      - `brainiac-store::library::proposals`: the noisy channel tamed in
        order — (1) rate limit per author (counted from the corpus's own
        rev-1 authors, no counter to drift; BRAINIAC_LIB_PROPOSE_PER_HOUR,
        default 5), (2) dedup by slug OR verbatim statement against the WHOLE
        corpus, any lifecycle — a duplicate collapses onto the existing
        standard and the agent is told what the org already decided (an
        adopted rule to follow, an open candidate, or a rejection to
        respect), (3) optional evidence memory validated under the caller's
        RLS; without evidence the rule can only ever be adopted by decree
        (schema-enforced since LB0).
      - Surfaces: REST `POST /v1/library/standards/propose` (`lib:propose` —
        the third scope, now minted; 429 with a humane message on budget,
        input caps: a proposal is a rule, not an essay) + MCP
        `standard_propose` (same store funnel, so REST and MCP cannot drift;
        outcome notes coach the agent: follow the adopted rule, respect the
        rejection, keep the sixth idea for the session summary). Origin chips
        on the console rule detail ("mined by the sweep" / "proposed by an
        agent").
      - **Deviation (deliberate, same family as LB3's):** "proposal→candidate
        precision on scripted agent sessions" is not measurable yet — the
        proposal channel is a funnel, not a generator; precision belongs to
        the agents' judgment, which no fixture can score deterministically.
        The gate instead proves the funnel's guarantees end-to-end.
      - Gate (`library_pg` +1 test, real HTTP + MCP): a lib:read token cannot
        propose; a proposal is a `proposed`, origin=agent candidate — never
        an adopted rule; duplicates collapse onto the open candidate (one row
        for ten agents); a rejected idea comes back `duplicate/rejected` with
        the rejection told; bogus evidence is 404, nothing created; the
        hourly budget closes the flood gate at 429 (duplicates and refusals
        never burn budget — an earlier draft of the test proved that by
        accident); MCP gets the same dedup verdicts. Full suites green.

- [x] **Follow-up 2 — the Library's signals go red on their own** (2026-07-15)
      - `brainiac-core::health`: `LIBRARY_DORMANT_DAYS` (30),
        `LIBRARY_GATE_SLO_SECS` (14d — a rule proposal is a policy question,
        not a memory promotion; pretending an org settles policy in 48h would
        make the number a lie that gets ignored), and `rule_is_dormant`, whose
        whole job is the age guard: **a rule adopted yesterday with no uses is
        NEW, not dead** — flagging it would teach maintainers the signal cries
        wolf, and then it is worth nothing when a rule really is dead.
      - `brainiac-store::library::health_signals`: one round trip for
        `standards_adopted / at_gate / oldest_gate_secs / dormant`,
        `skills_published / dormant`, RLS-scoped like every number in the
        report.
      - Report: three new attention items — dormant rules (warning), the gate
        queue aging past its SLO (warning; info while inside it), dormant
        skills (info). Console: a "library on top" panel whose border goes
        magenta when a rule has gone quiet.
      - **Deliberately NOT a fifth pillar.** Two reasons, both honesty: the
        four-pillar composite is a number orgs track week over week and
        silently redefining it the day someone enables mining would break
        every trend line it is compared against; and there is no calibration
        data yet — the same posture the page-read signals took. Going red IS
        the promise; it needs no weight in a composite. A test pins the
        composite at four pillars so a future edit cannot drift it.
      - Gate: `library_pg` — a long-adopted untouched rule is dormant and
        raises its item; a rule adopted TODAY and unused is not; the gate's
        SLA is visible; the composite is still four pillars.
- [x] **Follow-up 1 (L8) — standards render as a KB page** (2026-07-15)
      - `migrations/0031_standards_pages.sql` + `DocKind::StandardsPage` +
        `SectionBinding.stack`: a per-stack page riding the whole document
        layer — dirty-marking, revisions, the review gate, the health
        breaker, the Confluence target — unchanged.
      - **The design decision, and it is the whole feature:** it is
        *projected*, never *composed*. Every other page hands memories to a
        model and asks for prose; a rule's statement is one sentence a named
        human ratified, and asking a model to re-word it would fork the org's
        own commitment — the page would ask people to follow something subtly
        different from what the gate approved. Same principle as `detail_md`
        being copied verbatim (KB-PLAN D3), pointed at the layer where it
        matters most. So: no LLM, no temperature, no citation firewall to
        police. The plan said "composed"; the plan was wrong.
      - Consequences that fall out for free: the render is deterministic, so a
        revision diff means *the org's judgment changed* and never *the model
        phrased it differently today*; `composed_from` is the rules' evidence
        closure, so the dependency index still marks the page dirty when the
        evidence moves and a Confluence reader is one click from the signed
        memory; a decreed rule says "decreed" on the page, in the open.
      - `library::mark_standards_pages_dirty` + hooks in adopt/deprecate: the
        normative layer's mirror of `mark_dirty_for_memory`. Retire a rule and
        its page goes stale by itself — otherwise the wiki keeps publishing a
        rule the org retired.
      - `scaffold_standards_pages` (≥3 adopted rules earn a stack a page,
        born a DRAFT and born dirty), wired into the compose sweep.
      - Gate (`standards_page_pg`): the ratified sentence reaches the page
        byte-identical; a PROPOSAL never does; a second render is
        byte-identical; scaffolding fires at the threshold and is idempotent;
        retiring a rule marks the page dirty and the rule leaves; a decreed
        rule says so.

## The Library ladder is complete

LB0–LB5 all shipped 2026-07-15, in one line of sessions: the public surface
first (honesty-guard tests pinning every stamp), then substrate →
distribution → console → mining → proposals, each phase gated before the
next — plus follow-ups 1 and 2, which closed the loop (a dead rule now raises
its own hand, and adopted rules reach the wiki people already read). Every
stamp on `/library` is `shipped`.

**What is deliberately NOT built, and what would have to be true first:**

- **#3 enforcement hooks** — relaxes Never #3 (*no silent enforcement*). This
  is a product decision, not a task: a rule engine that gates CI is a
  different product with a different failure mode, and it needs its own
  consent design + a false-positive budget agreed before an org turns it on.
  Do not slip it in as a convenience.
- **#5 cross-org / public library exchange** — the first Library feature with
  a blast radius. Needs an org capability flag (`kb_enabled`'s sibling) and a
  leak-eval matrix over the sharing rules before a single rule leaves a
  building.
- **#6 per-rule adoption from repo scan** and **#7 generative skill mining** —
  both gated on the extraction-recall workstream, same standing rule that
  governed KB3. Pointing a generator at repos multiplies input noise before
  the instrument is fixed.
- **#4 skill eval harness** — needs a scripted agent-task runner to score
  "did this skill improve the outcome" honestly. Without one, any number it
  printed would be theatre, which is the one thing this product does not ship.
