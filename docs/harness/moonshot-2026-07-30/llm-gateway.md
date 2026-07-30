# Moonshot scan — llm-gateway

> Total: 2
> Files read: 13

## 1. Sovereign Brain — the org's reviewed corpus trains the model that reads it, and inference moves in-house

- **Severity**: Critical
- **Category**: data-sovereignty-flywheel
- **File**: `crates/brainiac-gateway/src/providers/openai.rs:490` (`openai_compat_from_env` — the
  already-existing Ollama/vLLM door), `crates/brainiac-gateway/src/lib.rs:122`
  (`provider_from_spec`), `crates/brainiac-pipeline/src/extract.rs:487` (`extract_once` — the
  teacher call whose input/output are thrown away), `migrations/0001_init.sql:51`
  (`provenance.model_ref`) + `:168` (`promotions`), `crates/brainiac-eval/src/gates.rs:1`
  (the parity gate that already exists for embedders).
- **Scenario**: A bank installs Brainiac. Day 1 it runs in *teacher mode*: extraction goes to a
  frontier hosted model, and every exchange (chunk in → JSON memories out) is retained as a
  training pair, labelled downstream for free by the governance loop that already exists — a
  memory a human promoted to `canonical` is a positive, a `rejected` one or a `wrong` feedback
  verdict is a negative. Day 60 the org has ~50k reviewer-graded pairs *in its own dialect*
  (its repo names, its service topology, its pitfalls). `brainiac distill export` emits that
  as JSONL; a small open model (Qwen3-4B / 8B class) is tuned on it and served on the bank's own
  GPU behind the existing `openai-compat` base-URL seam. Before it can carry traffic it must beat
  a gate: meridian's `extraction`/`contradiction`/`docs` profiles, student vs teacher on the same
  golden fixtures, within a committed delta — the exact mechanism `gates.rs` already applies to
  embedders. It passes, an operator flips `BRAINIAC_MODEL_EXTRACT=openai-compat:brainiac-extract-v3`,
  and from that hour **no proprietary transcript, no line of the bank's code, and no dollar leaves
  the building for the highest-volume stage of the pipeline** — while the model keeps getting
  better every month the org reviews knowledge, because review *is* the training signal.
- **Root cause**: The gateway is a stateless pass-through. It can *call* a local model today
  (base-URL swap), but nothing in the product ever produces a local model worth calling, because
  the single most valuable byproduct of every pipeline run — the teacher's (prompt, completion)
  pair, joined to whether a human later trusted the result — is discarded the moment
  `extract_once` parses it. `pipeline_runs` keeps only counters (`llm_calls`, `repairs`,
  `parse_failures`; migration 0010) and `provenance.model_ref` keeps a string. So the org's
  accumulating judgment can never become model weights, the external API stays load-bearing
  forever, and "we send your transcripts to Alibaba" stays an enterprise blocker for the one
  category of customer (regulated, IP-sensitive, agent-heavy) that needs an org memory server most.
- **Impact**: Three step-changes at once. (a) **Economics**: extraction is the dominant call
  volume (one call per chunk, plus up to 2 repair calls at `MAX_REPAIR_ATTEMPTS`); moving it to
  amortized local GPU collapses the product's only variable cost by roughly an order of magnitude
  and turns gross margin from usage-linked to fixed — the difference between a per-seat SaaS and
  an appliance you can sell to a bank for a flat licence. (b) **Data posture**: a genuine
  air-gappable mode ("your knowledge never trains someone else's model, and never leaves your
  VPC") which no hosted memory product can match; this is a procurement door-opener, not a
  feature bullet. (c) **Compounding quality**: an org-tuned extractor should out-perform a
  frontier general model *on that org's own idiom* — the 36% hard-failure rate on distilled
  statements and the stochastic parse failures documented in `extract.rs:420-431` are precisely
  the failure class that in-domain tuning removes.
- **Fix sketch**: Five slices, each shippable and each valuable alone.
  1. **Retain the exchanges.** New table `llm_exchanges (id, org_id, stage, model_ref,
     prompt_hash, system_prompt_version, user_text, response_text, tokens_in/out, run_id,
     created_at, retain_until)` plus `llm_exchange_memories (exchange_id, memory_id)`. Write it
     from a new `gateway::exchanges` sink that `providers::mod::meter` already sits in front of —
     `meter` is the one chokepoint every provider goes through, so this is one function, not six.
     Off by default (`BRAINIAC_RETAIN_EXCHANGES`), org-scoped + RLS like every other table,
     redaction via the existing `brainiac_core::redact`. Link memory ids where `extract.rs`
     already knows them (`extracted.memory_ids`, `worker.rs:625`).
  2. **Label for free.** A view joining `llm_exchange_memories → memories.status →
     promotions.policy_decision / reviewer_id → memory_feedback.verdict`. No new human work: the
     promotion review queue and MCP `memory_feedback` tool already produce the labels.
  3. **Export.** `brainiac distill export --stage extract --min-reviewed 5000` → JSONL of
     (system prompt version, chunk, gold JSON = the *post-review* memory set, not the raw
     completion). Rejected memories become negatives / "do not extract" examples.
  4. **Prove parity before trusting it.** A `student` axis on the existing eval grid
     (`brainiac-eval/src/grid.rs`, `extraction_profile.rs:547` already reads
     `for_stage(Stage::Extract).model_ref()`), plus a `Baseline`-style hard gate: the student
     may not regress extraction F1 / contradiction precision beyond a committed delta. This is
     the promotion mechanism — the flip is gated on evidence, exactly like the embedder swap.
  5. **Shadow before flip.** A `ShadowProvider` decorator implementing `ChatProvider` that calls
     the teacher authoritatively and the student in the background, recording agreement into
     `llm_exchanges`. Zero risk, real production evidence, and it is ~60 lines given
     `Arc<dyn ChatProvider>` composition in `ProviderRouter`.
  Fine-tuning itself is a script + a recipe (LoRA on the exported JSONL), not Rust — the moonshot
  is that this repo *owns the corpus, the gate, and the switch*, which is the hard part.
  Boundary note: the sibling spend-ledger context owns money accounting; `llm_exchanges` here is
  a training/eval corpus with outcome attribution, and should read the ledger rather than restate it.

## 2. Quality contracts at the chokepoint — callers declare the outcome, the gateway learns which model meets it

- **Severity**: High
- **Category**: outcome-learned-routing
- **File**: `crates/brainiac-gateway/src/lib.rs:64` (`ChatProvider` — the byte-in/byte-out trait
  that is the ceiling), `crates/brainiac-gateway/src/lib.rs:237` (`ProviderRouter::for_stage`),
  `crates/brainiac-gateway/src/providers/mod.rs:71` (`meter`, which passes `name: None` so even
  telemetry can't tell the stages apart), `crates/brainiac-pipeline/src/extract.rs:507-551`
  (the repair ladder, hardcoded at one call site), `crates/brainiac-pipeline/src/contradict.rs:91`,
  `resolve.rs:147`, `compose.rs:370`, `faithfulness.rs:241` (five more call sites with no ladder
  at all), `migrations/0001_init.sql:168` + `0004_memory_feedback.sql:5` (the outcome labels).
- **Scenario**: Call sites stop naming models and start naming *requirements*. `run_extract` asks
  the gateway for `Contract::<ExtractionOutput> { schema, validator: parse_extraction,
  target_accept_rate: 0.95, budget: Cheap }`. The gateway owns the rest: try the cheapest arm that
  its own measured history says clears that contract for this stage **and this source kind**,
  validate the typed result in-process, repair or escalate to a stronger arm on failure, and stamp
  the winning arm on the response. Behind it, a per-(org, stage, source_kind, prompt_version)
  scoreboard is fed not by a benchmark but by *what humans did next*: `provenance.model_ref` →
  `memories.status` → `promotions.policy_decision` → `memory_feedback.verdict`. Six months in, an
  operator opens a console page that says: for `session_transcript` extraction, arm
  `qwen-plus + prompt v2` yields 0.71 canonical-memories-per-call at 1/8 the cost of
  `anthropic:opus`, whose 0.74 is inside noise — so cheap wins, automatically, with the escalation
  arm reserved for the source kinds where it measurably doesn't. And because the scoreboard is
  outcome-shaped and multi-tenant, its anonymized aggregate becomes a **public model report card
  for org-knowledge extraction, graded by tens of thousands of real reviewer decisions** — a
  ranking no benchmark vendor can produce, published as a Brainiac artifact and used as the
  cold-start prior for every new install.
- **Root cause**: `ChatProvider::complete` returns a `String`. The gateway therefore knows only
  whether HTTP said 200 — never whether the inference was *correct*. Every notion of "good output"
  lives above it and is duplicated or missing: `extract.rs` has a two-attempt repair ladder and an
  empty-result escape hatch; `contradict`/`resolve`/`compose`/`faithfulness`/`divergence` have
  nothing comparable. Routing is a static env string (`BRAINIAC_MODEL_*`), so model choice is a
  human guess frozen at deploy time, and `meter(provider, model, None, …)` drops the stage on the
  floor — the product cannot answer "which model is actually earning its money on extraction?"
  even retrospectively, despite having the richest ground-truth labels in the industry sitting in
  `promotions` and `memory_feedback`. That is the ceiling: the chokepoint that sees every inference
  is the one component that learns nothing from any of them.
- **Impact**: 10x on three axes. **Quality**: escalation-on-validation-failure turns today's
  hard failure ("unparseable after 2 repairs" → dead-lettered job) into a second-model rescue,
  and extends the repair discipline that only extraction has to the five other call sites,
  including `compose`, where an unbacked sentence is a hallucination the eval fails on.
  **Cost**: the ladder means the strong model is paid for only on the calls that need it — for a
  workload where most chunks are easy, that is a large majority of extraction spend avoided while
  *raising* the accept rate, because savings fund escalation where it matters. **Category**:
  outcome-graded model report cards are a defensible public asset and a genuine network effect —
  every org's review clicks improve the routing prior for every other org, so the product gets
  better at choosing models the more customers it has, and new installs inherit six months of
  measured judgment on their first job.
- **Fix sketch**: Additive, arm by arm.
  1. **Stage-aware telemetry first** (one hour, immediate value): thread `Stage` into
     `providers::mod::meter` so `name` stops being `None` — `for_stage` already knows the stage and
     every provider already calls `meter`. Everything below needs this key.
  2. **Typed contract seam**: add `Contract<T> { json_schema: Option<Value>, validate: fn(&str)
     -> Result<T, String>, repair_hint: &'static str, max_repairs: u8 }` and a
     `ProviderRouter::execute<T>(stage, req, contract) -> Result<Verified<T>>`. Move `extract.rs`'s
     loop (`extract.rs:507-551`, including the final-attempt empty-result hatch) into it verbatim
     as the first implementation, then have `extract_once` call it — behavior-identical, proven by
     the existing `extraction_profile` eval, and instantly reusable by contradict/resolve/compose.
  3. **Attempt ladder**: `Arm { provider_spec, prompt_version, temperature }`, `BRAINIAC_LADDER_EXTRACT
     = "qwen:qwen-plus,qwen:qwen-max,anthropic:…"`. Escalate when the contract fails, never when
     it passes. `provider_from_spec` already parses every spec form needed, and `ProviderRouter`
     already holds `Arc<dyn ChatProvider>` so arms mix providers freely.
  4. **Outcome attribution**: a `model_arm` column on `pipeline_runs`/provenance-adjacent writes
     (or reuse `llm_exchanges` from finding 1 if that lands first), plus a materialized rollup
     `arm_outcomes (org_id, stage, source_kind, arm, calls, validated, canonical, rejected,
     wrong_verdicts)` refreshed by the existing sweep scheduler (`brainiac-server/src/sweeps.rs`).
  5. **Close the loop**: pick the default arm from `arm_outcomes` when a per-(stage, source_kind)
     cell has enough volume, epsilon-exploring the next arm up; env stays an override so an
     operator can always pin. Ship the console report-card view off the same rollup, and only then
     consider the anonymized cross-org aggregate — the local scoreboard is the product, the global
     one is the moat.
