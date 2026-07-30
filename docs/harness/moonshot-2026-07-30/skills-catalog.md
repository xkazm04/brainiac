# Moonshot scan — skills-catalog

> Total: 2
> Files read: 14

Context reality check (the map listed one file; here is what actually exists):
`console/app/console/modules/skills/` — `Module.tsx` (prefetches detail for up to
`DETAIL_PREFETCH_CAP = 100` skills), `SkillsCatalog.tsx` (208 lines, the whole shelf),
`Skeleton.tsx`, `skills-data.ts` (demo fixtures). Backend: `crates/brainiac-store/src/library/skills.rs`
(310 lines — insert/publish/list/`current_published_version`/`propose_skill`),
`crates/brainiac-store/src/library/usage.rs` (the pulse + `health_signals`),
`crates/brainiac-server/src/library.rs:294-426` (`skills_list` / `skill_get` / `skill_download`),
`crates/brainiac-server/src/mcp.rs:1912-2029` (`skill_search` / `skill_fetch` / `skill_report_usage`),
`crates/brainiac-core/src/library.rs:200-279` (`Skill`, `SkillVersion`, `LibraryUsageEvent`),
`migrations/0028_library_substrate.sql:109-160`, `migrations/0032_skill_proposals.sql`.

---

## 1. Proving grounds: the first artifact in the product whose effect is measured, not counted

- **Severity**: Critical
- **Category**: proven-capability-loop
- **File**: `crates/brainiac-store/src/library/usage.rs:13` (`record_usage`) · `crates/brainiac-core/src/library.rs:257` (`LibraryUsageEvent`) · `crates/brainiac-store/src/library/skills.rs:170` (`current_published_version`) · `console/app/console/modules/skills/SkillsCatalog.tsx:160` (shelf ranked by raw pulse) · `crates/brainiac-eval/src/gates.rs:38` (`Baseline`) · `docs/LIBRARY-PLAN.md:135` (follow-up #4, explicitly unbuilt)

- **Scenario**: A maintainer opens the skills catalog and every card carries a
  number no other knowledge product can print: *"review-migrations — 118 trials,
  first-try pass 41% → 78%, +37 pts vs. no skill (p<0.01). v2.1 is in canary at
  20% and leads v2.0 by 11 pts."* An agent that fetches a skill is, without
  knowing it, in a trial: it gets a deterministically-assigned published variant,
  runs the procedure, and reports the outcome it can actually observe (tests
  green / gate passed / abandoned mid-way). Three weeks later the candidate
  version has beaten the incumbent on real work, and a named human promotes it
  with one click on evidence rather than taste. A skill that measurably *hurts*
  outcomes goes red and gets retired — and the retirement is defensible, because
  the org can point at the trials. The org's know-how stops being a folder of
  confident markdown and becomes a body of **proven** procedures that get better
  every quarter under their own telemetry.

- **Root cause**: the Library's anti-rot mechanism (LIBRARY-PLAN L5) is a
  *popularity counter*. `LibraryUsageEvent` has exactly three variants —
  `Fetch | Check | Apply` — and `record_usage`'s signature can express "someone
  ran this" but has no field, anywhere, for *how it went*. The catalog's ranking
  (`SkillsCatalog.tsx:160`: `totalUses(details[b.id]) - totalUses(details[a.id])`)
  and the health signal (`usage.rs:82`, `skills_dormant`) are therefore both
  proxies for attention, not for value: the loudest skill wins the shelf even if
  it is wrong. The second, harder cap is structural: `skills.current_version` is
  a single `uuid` (`0028_library_substrate.sql:110`), and both serve paths join
  through it (`current_published_version`, `skill_fetch`, `skill_download`), so
  **two versions of a skill can never be in flight at once** — no comparison,
  no canary, no trial can even be expressed. Meanwhile the product ships a real
  eval harness with baseline-relative regression gates
  (`brainiac-eval/src/gates.rs`) whose seven profiles all point at retrieval,
  extraction, drift, and docs — and never once at the Library. Memories have a
  verdict loop (`memory_feedback` → dispute triage → the sweep's
  "≥2 independent helpful verdicts" miner); skills, the layer agents *execute*,
  have none. The mission sentence is "an eval harness that proves every claim,"
  and the one artifact class the org hands agents to *act* on is the one class
  that proves nothing.

- **Impact**: converts the entire skill catalog from unfalsifiable to measured.
  Concretely: (a) every published skill gains a success rate and a lift estimate
  vs. the no-skill baseline, so "should we write a skill for X" becomes an
  answerable question instead of a taste argument; (b) skill versions can improve
  *continuously* — a maintainer's edit is a hypothesis the system tests on real
  work within days instead of a permanent unreviewed change; (c) deprecation
  becomes evidence-backed, which is the difference between a catalog of 30 trusted
  skills and a catalog of 300 nobody trusts; (d) it makes "verified on N tasks" a
  maturity stamp the org can put in front of an auditor. Strategically it is the
  only path to a *marketplace-grade* artifact: nobody adopts another org's skill
  on vibes, but a skill that says "+37 pts across 118 trials" is a tradeable
  good. This is the one moonshot that upgrades the product's category claim from
  "governed knowledge store" to "the system that proves org know-how works."

- **Fix sketch**, smallest-proving-slice first:
  1. **Outcome as a first-class signal (no serving change).** New table
     `skill_outcomes (id, org_id, skill_id, version, team_id, verdict, task_fingerprint, evidence jsonb, occurred_at)`
     — deliberately a sibling of `library_usage_events`, not a column on it, so
     the "no leaderboard" schema promise stays intact by *shape*: **no user
     column**, verified by the same schema-shape test 0028 already has
     (`0028_library_substrate.sql:149`). New core enum `SkillOutcome
     { Succeeded, Failed, Abandoned }` beside `LibraryUsageEvent` in
     `brainiac-core/src/library.rs`, with the same parse/as_str round-trip test
     pinned to the migration CHECK. Reported through one new MCP tool
     `skill_report_outcome { slug, semver, verdict, note? }` next to
     `skill_report_usage` (`mcp.rs:1989`) and `POST /v1/library/skills/{slug}/outcome`
     under `SCOPE_LIB_READ`, recorded warn-only in its own tx exactly like
     `record_library_usage_quietly` — vital signs must never cost an agent its
     answer. Store fn `record_outcome` + `outcome_rates(skill_id) -> Vec<(version, n, succeeded)>`
     in `library/usage.rs`. Surface it on the card next to `uses > 0 ? …uses / 30d`
     in `SkillsCatalog.tsx:83`. **This slice alone proves the loop** and is
     independently shippable.
  2. **Trials — two live versions.** New table `skill_trials (skill_id, baseline_version,
     candidate_version, split_pct, opened_by, opened_at, closed_at, decision)`.
     Replace the single-pointer serve with `serve_version_for(conn, skill_id,
     assignment_key) -> Option<SkillVersion>` in `skills.rs`, where the key is a
     stable hash of (skill_id, principal.team_ids.first(), repo fingerprint) so a
     given working context sees ONE variant for the whole session and the
     `Fetch` event already recorded at `library.rs:404` / `mcp.rs:1966` carries
     the served `semver` (it already does — `Some(&version.semver)`). Both arms
     must be `published_by IS NOT NULL`; a draft is still never served, so
     Never-list #2 and the named-human gate are untouched — a trial is a choice
     between two things a human signed.
  3. **A gate, so nothing is promoted on noise.** New `skills_profile.rs` in
     `brainiac-eval` reusing the `Baseline` / soft-delta machinery of `gates.rs`:
     a candidate arm needs a minimum trial count and a lift above a committed
     delta before the console offers promotion. Promotion itself stays
     `publish_skill_version(conn, version_id, published_by)` — a named human,
     unchanged.
  4. **Rank and retire on truth.** `SkillsCatalog.tsx`'s `ranked` sort keys on
     lift-then-pulse; `health_signals` (`usage.rs:62`) gains `skills_harmful`
     (published, ≥N trials, success rate below the no-skill baseline) as an
     attention item beside `skills_dormant` — the existing age-guard discipline
     (`rule_is_dormant`: adopted yesterday ≠ dead) applies verbatim to trial
     counts.

---

## 2. The capability graph: stop serving skills, start serving capability

- **Severity**: High
- **Category**: platform-surface
- **File**: `crates/brainiac-core/src/library.rs:218` (`SkillVersion.manifest: serde_json::Value`) · `crates/brainiac-server/src/mcp.rs:1920-1932` (`skill_search`'s ILIKE) · `crates/brainiac-server/src/library.rs:161` (`skill_view` — projects 6 scalar fields, drops the manifest) · `crates/brainiac-store/src/library/skills.rs:132` (`list_skills` — `ORDER BY slug`, a flat shelf) · `migrations/0028_library_substrate.sql:126` (`skill_versions` — no edge table anywhere)

- **Scenario**: An agent lands in a service it has never seen and makes **one**
  call: `skill_plan { task: "add a Stripe-style provider to payments", stack: "rust" }`.
  Back comes a dependency-ordered **capability pack**: the `add-a-data-provider`
  skill, the two prerequisite skills its manifest declares (`configure-a-secret`,
  `write-a-provider-contract-test`), the 6 *mandatory* adopted standards for the
  stacks those procedures touch, the governed memories the procedures cite as
  context, deduped and fitted to a token budget — with one usage event recorded
  per included artifact so the pulse still tells the truth. The agent did not
  browse, guess, or assemble; it *acquired the org's capability for that task*.
  On the maintainer side the same edges render as a graph with visible holes:
  *"`deploy-a-service` requires `rotate-credentials`, which nobody has written —
  31% of packs this month came back with an unmet requirement."* The catalog
  stops being a shelf and becomes the org's **capability surface**, where each
  new skill raises the value of every skill it attaches to.

- **Root cause**: a skill is a *leaf with no edges*, by three independent
  omissions. (1) `SkillVersion.manifest` is untyped `serde_json::Value` and
  nothing in the workspace reads it — `skill_propose` writes a three-key blob
  (`mcp.rs`/`library.rs:697`: name/summary/domain) and `skill_view`
  (`library.rs:161`) drops it entirely from the list projection, so the one field
  in the schema that *could* express structure is inert. (2) Migration 0028 has
  no skill→skill or skill→standard relation of any kind: `skills`,
  `skill_versions`, and a usage log, full stop. Composition is therefore
  unrepresentable, not merely unimplemented. (3) Discovery is name-matching:
  `skill_search` is `ILIKE` over name/slug/description/domain with `LIMIT 10`
  (its own comment concedes "if the catalog ever outgrows this, the fix is an
  embedding") — in a product whose crown jewel is hybrid vector+FTS+graph
  retrieval, the *skill* surface is the one place that retrieval was never
  pointed at. The net effect: all assembly work — which skills, in what order,
  under which rules, with what context — is pushed onto every agent in every
  session, and the org's aggregate know-how is capped at "one markdown file at a
  time." That cap is why the catalog reads as a list to browse rather than the
  product's payload.

- **Impact**: 10x on the *unit of delivery*. Today an agent's best case is one
  procedure it happened to name-match; after this it is the transitive closure of
  everything the org knows about the task, in order, with the binding rules
  attached — which is the difference between a wiki page and onboarding. Three
  compounding effects: **network** — the Nth skill gains value from the N-1
  before it, so the catalog's worth grows superlinearly instead of linearly and
  a 40-skill library beats four 10-skill libraries; **coverage as a product
  surface** — unmet `requires` edges are the first mechanically-derived answer to
  "what know-how is the org missing," which is exactly the question a
  standards/skills program cannot answer today; **portability** — a
  dependency-closed pack is the natural export unit for any future exchange
  (a skill that declares what it needs can be evaluated by a stranger; a bare
  markdown file cannot). It also retires an embarrassing asymmetry: memories get
  graph expansion and entity anchors, skills get `ILIKE`.

- **Fix sketch**, in order:
  1. **Type the manifest and validate it at the gate.** `SkillManifest { name,
     summary, domain, stacks: Vec<String>, requires: Vec<String>, standards:
     Vec<String>, tools: Vec<String> }` in `brainiac-core/src/library.rs` beside
     `SkillVersion`, with `SkillVersion::manifest_typed()` tolerating today's
     three-key blobs (absent arrays = empty, so every existing row stays valid).
     `publish_skill_version` (`skills.rs:79`) gains the only new refusal:
     a dangling `requires` slug or a cycle in the closure **cannot be published**
     — the named-human gate is where a broken edge must die, not the serve path.
  2. **Materialize the edges.** `skill_edges (org_id, skill_id, kind CHECK
     (kind IN ('requires','standard')), target_slug, version_id)` written from
     the manifest inside the same publish transaction — a projection of signed
     content, never a second source of truth (the same discipline
     `standards_page` chose: projected, never re-authored). Store fns
     `closure_of(conn, skill_id) -> Vec<Skill>` (recursive CTE, depth-capped)
     and `unmet_requires(conn) -> Vec<(String, String)>`.
  3. **The pack surface.** MCP `skill_plan { task, stack? }` + `GET
     /v1/library/capability-pack?task=&stack=` under `SCOPE_LIB_READ`: seed the
     candidate skills through the existing hybrid retrieval path (replacing
     `skill_search`'s ILIKE — the fix its own comment asks for), expand via
     `closure_of`, join `list_standards(&mut tx, stack, Some(Adopted))` filtered
     to `Enforcement::Mandatory`, attach the memory anchors with the same
     composer `memory_context` already uses, topologically order, budget by
     tokens, and loop `record_usage(..., LibraryUsageEvent::Fetch, ...)` over
     every included artifact so finding #1's telemetry and this one reinforce
     rather than fight. Only published versions enter a pack — `skill_fetch`'s
     refusal, unchanged.
  4. **Console.** `SkillsCatalog.tsx` gains a prerequisite chip row on the
     expanded card (it already renders "how an agent pulls it" — the pack call
     joins it), and a coverage panel listing unmet requires + orphan skills;
     `health_signals` gains `skills_unmet_requires` as an attention item.
  5. **Smallest proving slice**: steps 1–2 plus `skill_plan` returning the
     dependency closure of one *named* skill (no retrieval seeding yet). That
     proves composition end to end against a real bundle, and the retrieval
     seeding in step 3 becomes a pure upgrade of the seed set.
