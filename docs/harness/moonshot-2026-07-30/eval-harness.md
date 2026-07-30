# Moonshot scan — eval-harness

> Total: 2
> Files read: 22

## 1. Tenant Eval — every customer org gets its own golden set, its own gates, and a signed no-leak attestation

- **Severity**: Critical
- **Category**: platform-surface
- **File**: `crates/brainiac-eval/src/retrieval_profile.rs:69` (`run(store, fx, …)` — the whole
  profile is parameterized on a YAML tree, not on an org) · anchors:
  `crates/brainiac-eval/src/seed.rs:80` (`seed_gold`), `crates/brainiac-eval/src/grid.rs:37`
  (`TRUNCATE_SQL`), `crates/brainiac-server/src/main.rs:1303` (`run_eval`, DESTRUCTIVE),
  `crates/brainiac-fixtures/src/loader.rs:41` (`load`), `results/baseline.json` (ONE global
  baseline), `fixtures/v1/retrieval/leak.yaml`, `crates/brainiac-store/src/feedback.rs:23`
- **Scenario**: A prospect's security review asks the only question that matters for an org-memory
  server: *"prove that your retrieval never shows my contractor another team's private
  memory — on my data, not your demo company."* Today the honest answer is "we have ten
  hand-written leak probes against a synthetic fintech called Meridian." After this, the answer is
  a command: the customer's own Brainiac instance enumerates every non-org-visible memory in
  *their* corpus, synthesizes the hardest possible probe for each (the memory's own content as the
  query), fires it under every principal that must NOT see it, and emits a dated, signed
  attestation — *N memories × M unauthorized askers = 0 leaks at any rank* — as a first-class
  artifact in the console and over REST. The same machinery, once it can read gold from a live
  tenant instead of a YAML tree, gives each org a private baseline: a maintainer flipping the
  embedder, the reranker, the extraction model, or the graph-decay constant sees *their* NDCG
  and *their* refusal rate move before it reaches their agents, gated by *their* committed
  numbers. Gold labels arrive for free from signals the product already collects —
  `feedback` verdicts (`brainiac-store/src/feedback.rs`, already a ranking input via
  `FEEDBACK_WEIGHT`), resolved disputes, and supersession decisions — so the golden set grows
  with usage instead of with vendor person-days. Above the single tenant sits the platform play:
  aggregate-only telemetry (score distributions, stratum mix, leak-probe counts — never content)
  makes Brainiac the one party that can publish a real quality index for org memory, and each
  customer's own report becomes the renewal artifact.
- **Root cause**: `Fixtures` is a concrete struct read off disk (`loader.rs:41`), every profile
  takes `fx: &Fixtures` by value-of-type, `seed.rs` writes the fixture org into the database, and
  the CLI path truncates 20-odd tables (`grid.rs:37`) before it can measure anything. The harness
  therefore has exactly one possible subject: a synthetic org that the harness itself creates. Two
  consequences cap the product. (a) Every quality and safety claim is a claim about Meridian —
  `fixtures/bank/README.md` is explicit that even the 1,200-memory scale corpus is "not a
  retrieval-quality benchmark," and the committed `results/baseline.json` is calibrated on
  `deterministic-bow-v1`, a toy bag-of-words embedder, so CI defends numbers no customer will ever
  observe. (b) The zero-leak invariant — the product's loudest promise and the one it gates
  hardest (`report.rs:171` `gate_failures`) — is verified against a 10-case hand-authored suite,
  while the real corpus's leak surface is combinatorial in memories × users and grows daily.
- **Impact**: 10 hand-authored leak probes → thousands of automatically derived ones per org, at
  every deploy, with zero authoring cost — the leak gate stops being a sample and becomes a
  proof. Quality gating moves from 1 global baseline on a fake embedder to 1 baseline per tenant
  per config on the real one, which is what makes model/embedder upgrades shippable to paying
  orgs at all (today an upgrade is un-gateable for the customer: the vendor's gate says nothing
  about their corpus). Gold-label supply flips from human-authored (EVAL.md §4 budgets 2–3
  person-days per fixture version and freezes at v1) to usage-generated, which is the difference
  between a benchmark that ages and one that compounds. And it converts the harness from an
  internal CI cost center into a billable trust surface plus the only credible cross-org
  benchmark in the category.
- **Fix sketch**: The first slice needs no labels, no capture, and no new fixture authoring — it
  is the leak attestation, and it is achievable in one pass.
  1. **Split gold-source from profile.** Introduce `trait GoldSource` in
     `brainiac-eval/src/lib.rs` exposing exactly what the profiles consume today —
     `qa_queries()`, `temporal_cases()`, `leak_cases()`, `memory_id_map()`,
     `principal_for(user)`. `impl GoldSource for Fixtures` is mechanical (see the four `fx.`
     field reads in `retrieval_profile::run` and `seed::principal_for_user`), so the existing
     tests in `tests/retrieval_pg.rs` keep passing unchanged.
  2. **`TenantGold` reads the live store.** Second impl, backed by `brainiac-store`: enumerate
     memories whose `visibility` is `team`/`private`, and for each, derive leak cases by pairing
     it with principals that lack access (users outside `team_members` for its team, plus
     cross-project askers now that project RLS isolation has shipped). Query text = the memory's
     own `content` — the maximally adversarial probe, and free.
  3. **A read-only eval path.** `run_eval` in `brainiac-server/src/main.rs:1303` must gain a
     non-destructive branch: no `TRUNCATE_SQL`, no `seed_gold`, `--org <slug>` instead of
     `--fixtures`. Reuse the existing leak loop and `RetrievalReport::gate_failures` verbatim —
     the hard gate is already the right shape; it has only ever been fed the wrong subject.
  4. **Attestation artifact + surface.** Emit `results/attest/<org>-<date>.json` (probe count,
     asker count, leaks, embedder/reranker tags, commit sha) and expose it as
     `GET /v1/quality/attestation` plus a console panel next to the audit module. This is the
     slice that sells.
  5. **Then per-tenant baselines.** Generalize `gates::Baseline` from one file to a row keyed by
     `(org_id, embedding_model, reranker)`, written by the same `--write-baseline` flag; the
     mismatch short-circuits at `gates.rs:92`/`gates.rs:104` already encode the correct
     "never compare across configs" discipline and carry over unchanged.
  6. **Then mined QA gold.** Capture retrievals (query, principal, as_of, returned ids/ranks/
     scores — no such log exists today; `document_reads` from migration 0025 is the nearest
     precedent), join to `feedback` verdicts to produce graded relevance, and route candidate
     gold items through the reviews module's existing bulk-keyboard lane for one-keystroke
     confirmation. Stratify mined items by the §2.5 taxonomy so per-stratum gates keep working.

## 2. Calibrated trust — make the harness the source of the runtime's "I don't know"

- **Severity**: Critical
- **Category**: agent-feedback-loop
- **File**: `crates/brainiac-eval/src/report.rs:115` (`DiagHit { score, grade }` — the
  score↔truth pairs are already collected, then thrown away) · anchors:
  `crates/brainiac-eval/src/gates.rs:145` (a refusal gate defending 0.0),
  `results/baseline.json` (no `negative_empty_rate` at all → 0.0),
  `crates/brainiac-core/src/scoring.rs:1` (`blended_score`, RRF-scale, uncalibrated),
  `crates/brainiac-store/src/retrieval.rs:146` (`RetrievalHit::score`, no threshold anywhere in
  the file), `crates/brainiac-eval/src/retrieval_profile.rs:176`,
  `crates/brainiac-server/src/mcp.rs:1101` (score never reaches the agent)
- **Scenario**: An agent asks Brainiac "what's our retry policy for the settlement worker?" for a
  service the org has never written a word about. Today it receives a confident, well-formed,
  provenance-carrying list of the *nearest* memories — because nothing in the retrieval path can
  distinguish "here is the answer" from "here is the closest thing I had." After this, every hit
  carries a **calibrated probability of being relevant**, learned by the eval harness from its own
  gold grades, and the response can say `abstain: no org knowledge above 0.4 confidence` instead
  of filling the char budget with plausible noise. The agent's prompt changes from *"here is what
  the org knows"* to *"the org knows this at 0.9, is unsure about that at 0.5, and has nothing on
  the third thing"* — which is the difference between memory an agent can act on and memory an
  agent must double-check. Downstream, every abstention is a labelled hole in the corpus: the
  abstain stream becomes the highest-signal ingestion queue the product could have (the drift
  profile already names this shape — `ClaimVerdict::Unmatched` is "a HARVEST candidate, not
  drift"), and the review queue gets fed by real unanswered questions instead of by whatever
  happened to be ingested.
- **Root cause**: the ranking key is deliberately rank-based and non-probabilistic —
  `blended_score` fuses RRF ranks plus tiebreak-scale nudges (`RECENCY_WEIGHT = 1e-4`), so a
  score is *only* comparable within one result list and means nothing across queries. There is
  consequently no threshold in `retrieval.rs`, no abstain path, and no confidence field on
  `RetrievalHit`; `mcp.rs` never surfaces the score at all. The measurement side already knows
  this is a hole and cannot close it: the negative stratum exists (`fixtures/v1/retrieval/qa.yaml`,
  `stratum: negative`), `retrieval_profile.rs:176` flags every negative query that returns hits as
  a violation, and `gates.rs:145` gates refusal against the baseline — but the committed baseline
  has no `negative_empty_rate`, so it defaults to **0.0**, and gates.rs's own comment concedes
  that 0.0 "is also every committed baseline's current value." Translated: the system refuses **0%**
  of out-of-scope queries, the harness measures it, the gate defends it, and nothing can improve
  it, because "empty result set" is the only refusal mechanism the engine has and it fires only
  when literally no candidate exists. The calibration data needed to fix this is generated on every
  single eval run and discarded: `QueryDiagnostic.got[]` holds `(score, grade)` for every hit of
  every graded query across six strata.
- **Impact**: 0% → a measured, gated refusal rate on out-of-scope queries, which is the difference
  between an org memory server and a plausible-sounding one; for the agent, the class of failure
  that costs the most (confidently wrong org knowledge, silently followed) becomes an explicit gap
  it can route around or ask about. Adds one genuinely new axis to the harness — **calibration
  error (ECE / reliability curve)** alongside NDCG — which is the metric that decides whether a
  confidence number can be trusted at all, and which no amount of NDCG tuning substitutes for. It
  also upgrades every existing surface for free: `memory_context` can spend its char budget only
  on trustworthy entries, the reranker/embedder bake-off gains a "which config is best *calibrated*"
  column (a different winner than "best NDCG" as often as not), and per-stratum calibration tells
  the product where it is weak in a way a single aggregate never can.
- **Fix sketch**: The instrument comes first — same eval-before-production doctrine the drift
  profile was built under (`drift_profile.rs:1`, "eval-first by decree").
  1. **Fit, from data already in hand.** New `brainiac-eval/src/calibration.rs`: consume the
     `RetrievalDiagnostics` a retrieval run already produces, build `(score, relevant)` pairs from
     `DiagHit.score` and `DiagHit.grade >= 2`, and fit a monotone map score → P(relevant)
     (isotonic regression, or Platt on `logit`; isotonic suits a rank-derived score better and
     needs no distributional assumption). Emit `results/calibration.json` keyed by
     `(embedding_model, reranker, fixture_version)` — the exact key discipline `gates::Baseline`
     already enforces, so cross-config contamination is refused by construction.
  2. **Report it, gate it.** Add `expected_calibration_error` and a binned reliability curve to
     `RetrievalReport`, plus a `calibrated_abstain_rate`. `gates::regression_failures` gains an ECE
     ceiling next to the existing deltas. Fit on a held-out split of the QA set (or leave-one-
     stratum-out) so the numbers are not self-congratulatory.
  3. **Ship it into the runtime.** `Calibration` loads in `brainiac-core` beside `scoring.rs`
     (pure, deterministic, Postgres-free — same shape as `recency_decay`); `RetrievalHit` gains
     `confidence: Option<f64>`; `search_reranked` attaches it. Absent calibration file = `None`
     everywhere = byte-identical behavior to today, which is how this lands without a flag day.
  4. **Abstain.** `RetrievalRequest` gains `min_confidence: Option<f64>`; below it, hits are
     withheld and the response carries an explicit abstention reason. Expose `confidence` and the
     abstention on the REST envelope and in the MCP tools (`memory_context` filters its budget by
     it, `memory_search` reports it) — the MCP tool contract is versioned already, so this is an
     additive field.
  5. **Ratchet.** Re-baseline `negative_empty_rate` with the threshold live. The gate at
     `gates.rs:145` needs no code change — it has been waiting since it shipped for a number
     worth defending.
  6. **Then close the loop.** Route abstentions into the harvest queue as unanswered-question
     records; the questions an org's agents ask and Brainiac cannot answer are the single best
     ingestion signal the product will ever have, and nothing captures them today.
