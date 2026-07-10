# Golden Fixture Design — Eval Harness & Embedding Bake-off

**Purpose.** One synthetic dataset, three jobs:

1. **CI regression gate** — prompt, model, or retrieval changes must not silently degrade pipeline quality.
2. **Embedding & reranker bake-off rig** — your local open-model benchmark runs on the same fixtures, producing directly comparable numbers.
3. **BYOM provider support matrix** — the full pipeline scored per provider (Azure OpenAI / Vertex Gemini / Anthropic) to publish honest per-provider quality claims.

Everything is synthetic — invented company, invented services — so fixtures can live in the public repo with zero data-leak risk and zero benchmark contamination from real-world corpora.

---

## 1. The synthetic organization: "Meridian"

A fictional fintech with exactly the collision structure the product exists to handle.

### 1.1 Teams (three subgraph starting points)

| Team | Domain | Repos (entity seeds) | Deliberate overlaps |
|---|---|---|---|
| **payments** | Payment processing | `payment-service`, `refund-worker`, `psp-gateway` | Kafka, ArgoCD, "checkout" feature, retry policies |
| **platform** | Infra & delivery | `infra-live`, `deploy-tools`, `otel-collector-config` | Kafka, ArgoCD, OPA policies, retry policies |
| **data** | Analytics & ML | `event-lake`, `feature-store`, `fraud-model` | Kafka, "checkout" feature (funnel analytics), embedding models |

### 1.2 Planted collision sets (entity-resolution ground truth)

12 canonical entities, each referenced under different surface forms across teams:

| Canonical | payments says | platform says | data says | Difficulty |
|---|---|---|---|---|
| payment-service | "payment-service" | "payments API" | "the payments backend" | medium |
| Kafka | "Kafka" | "MSK cluster" | "the event bus" | hard (metonymy) |
| ArgoCD | "Argo" | "ArgoCD" | — | easy |
| checkout feature | "checkout v2" | — | "checkout funnel" | medium |
| retry policy std | "retry backoff rules" | "std-retry policy" | — | hard (concept, not artifact) |
| … | | | | |

**Plus 6 planted near-miss traps** — pairs that look mergeable but are NOT (e.g. `fraud-model` the repo vs "fraud model" the ML artifact; "checkout v2" vs deprecated "checkout v1"). False merges must be penalized as hard as missed merges.

### 1.3 Corpus dimensions (v1 of fixtures)

| Artifact | Count | Notes |
|---|---|---|
| Session transcripts | 40 | 10–60 turns each; Claude Code-style dev sessions, incident debugging, design discussions |
| Docs / READMEs / ADRs | 15 | Secondary ingestion sources |
| Expected memories (gold) | ~450 | Annotated: kind, entities, relations, visibility tier |
| Raw entities | ~140 | → 90 canonical after resolution |
| Merge pairs (positive) | 38 | Across the 12 collision sets |
| Near-miss pairs (negative) | 6 | Trap set |
| Planted contradictions | 12 | See §2.3 |
| Temporal supersession chains | 10 | See §2.4 |
| Retrieval QA items | 120 | See §2.5 |
| Document composition cases | 8 pages | See §2.6 |
| Language slices | EN 85% / CS 15% | Czech slice stresses embedding models on non-English technical prose — a real differentiator between open models |

Fixture data is generated once via LLM-assisted authoring **with human review of every gold label**, then frozen and versioned (`fixtures/v1/`). Regenerating or extending fixtures bumps the fixture version; scores are only comparable within a fixture version.

---

## 2. Fixture file formats

All fixtures are YAML under `fixtures/v1/`. IDs are stable strings (`mem-pay-0042`), never UUIDs, so diffs stay readable.

### 2.1 Transcripts with extraction gold

```yaml
# fixtures/v1/transcripts/pay-incident-007.yaml
id: src-pay-007
team: payments
kind: session_transcript
language: en
turns:
  - role: user
    text: "The refund-worker is timing out against psp-gateway again..."
  - role: assistant
    text: "Looking at the config — the retry backoff caps at 2s which..."
  # ...
gold_memories:
  - id: mem-pay-0042
    kind: pitfall
    content_gist: "refund-worker default 2s retry cap causes timeout storms against psp-gateway under PSP latency spikes"
    entities: [refund-worker, psp-gateway, retry-policy-std]
    relations:
      - {src: refund-worker, rel: depends_on, dst: psp-gateway}
    visibility: team
    must_extract: true          # recall counts against this
  - id: mem-pay-0043
    kind: decision
    content_gist: "raise refund-worker retry cap to 30s with jitter, aligned to std-retry policy"
    entities: [refund-worker, retry-policy-std]
    must_extract: true
distractors:                     # statements that must NOT become memories
  - "user says they'll grab lunch after this"
  - "speculative idea explicitly rejected two turns later"
```

**Scoring extraction:** predicted memories are matched to gold via embedding similarity ≥ τ **and** LLM-judge confirmation of semantic equivalence (judge model is pinned + versioned; judge prompts live in the repo). Precision counts unmatched predictions (including distractor pickups); recall counts unmatched `must_extract` gold items. Entity/relation attachment scored separately as micro-F1.

### 2.2 Entity resolution gold

```yaml
# fixtures/v1/entities/merges.yaml
merge_sets:
  - canonical: kafka
    members: [{team: payments, name: "Kafka"},
              {team: platform, name: "MSK cluster"},
              {team: data,     name: "the event bus"}]
    difficulty: hard
negative_pairs:
  - [{team: data, name: "fraud-model"},        # repo
     {team: data, name: "fraud scoring model"}] # ML artifact — must NOT merge
```

**Metric:** pairwise precision/recall + **B³ (B-cubed) F1** over the clustering, reported overall and per difficulty tier. Negative-pair violations reported separately as `false_merge_count` — CI gate is **zero tolerance on false merges at auto-link confidence** (wrong merges silently corrupt the graph; missed merges just wait in the review queue).

### 2.3 Contradiction gold

```yaml
# fixtures/v1/contradictions/cases.yaml
- id: con-003
  memory_a: mem-plat-0107   # "std-retry: cap 2s, 3 attempts" (older)
  memory_b: mem-pay-0043    # "cap 30s with jitter"           (newer)
  expected: resolved_supersede    # b supersedes a
  supersede_direction: b_over_a
- id: con-009
  memory_a: mem-data-0071   # "use bge-m3 for search embeddings"
  memory_b: mem-data-0088   # "use nomic-embed for clustering"
  expected: resolved_coexist      # different scopes — NOT a real contradiction
```

Detection recall/precision, plus **resolution accuracy** (supersede vs coexist vs dismiss). Coexist cases are the trap: an over-eager detector that flags every tension trains reviewers to ignore the queue.

### 2.4 Temporal QA ("as-of" correctness)

```yaml
# fixtures/v1/temporal/asof.yaml
- id: tmp-004
  question: "What is the retry cap for refund-worker?"
  as_of: 2026-03-01
  expected_memory: mem-plat-0107   # old policy still valid then
- id: tmp-005
  question: "What is the retry cap for refund-worker?"
  as_of: 2026-06-15
  expected_memory: mem-pay-0043    # after supersession
```

Metric: exact-hit rate on the temporally correct memory at rank 1, and absence of the superseded memory in top-3 for current-time queries.

### 2.5 Retrieval QA (the bake-off core)

120 queries with **graded relevance** (0–3) against gold memory ids, stratified by capability:

| Stratum | n | What it stresses |
|---|---|---|
| semantic | 35 | Paraphrased conceptual queries — pure embedding quality |
| exact-identifier | 20 | `psp-gateway`, error codes, repo names — where vectors fail and BM25/hybrid must save you |
| cross-team graph | 20 | Answer lives in *another team's* subgraph, reachable only via canonical-entity hop (the product's thesis — a flat vector baseline should measurably lose here) |
| temporal | 15 | From §2.4 |
| negative | 15 | No relevant memory exists — measures refusal quality / score calibration; top-1 score above threshold = penalty |
| Czech | 15 | CS queries against EN memories and vice versa — cross-lingual retrieval |

```yaml
- id: qa-062
  stratum: cross_team_graph
  query: "why do our checkout payment retries sometimes hammer the PSP?"
  asking_as: {team: data, user: analyst-1}     # RLS context of the query!
  relevant:
    - {memory: mem-pay-0042, grade: 3}
    - {memory: mem-plat-0107, grade: 2}
  forbidden: []                                 # see leak tests below
```

**Metrics:** NDCG@10, Recall@5, MRR — overall and per stratum. Latency p50/p95 recorded per configuration.

**RLS leak tests (hard invariant):** 10 additional queries where `asking_as` lacks access to the best answer. `forbidden` lists memory ids that must never appear at any rank. **Any leak = build failure**, not a score deduction.

### 2.6 Document composition gold

```yaml
# fixtures/v1/documents/kafka-page.yaml
- id: doc-kafka
  doc_kind: entity_page
  visibility: org
  bindings: {entities: [kafka], kinds: [decision, pitfall, howto]}
  must_cover:              # claim coverage — gist list the page must contain
    - "MSK cluster is the managed deployment"
    - "std-retry policy applies to consumers"
    - "checkout funnel events flow through it"
  must_cite: true          # every claim traceable to a memory id annotation
  forbidden_memories:      # team-private facts that must NOT surface in an org page
    - mem-pay-0055
  staleness_case:
    supersede: {old: mem-plat-0107, new: mem-pay-0043}
    expect: page marked dirty AND recomposed revision reflects new fact
```

**Metrics:**
- **Coverage** — fraction of `must_cover` claims present (LLM-judge matched).
- **Hallucination rate** — claims in the composed page with no supporting memory id in `composed_from`; gate: 0 for `auto_published`, ≤ small ε for review-flagged revisions.
- **Leak rate** — `forbidden_memories` content appearing in the page: **zero tolerance, build failure** (this verifies the §8.2 visibility-capping invariant end-to-end).
- **Staleness propagation** — supersession → dirty-mark → recompose loop completes and the new revision reflects the change.
- **Pin preservation** — pinned sections byte-identical across regeneration.

---

## 3. Harness architecture & CLI

Rust crate `omem-eval`, sharing the core's retrieval and pipeline code (no reimplementation drift):

```
omem-eval run --fixtures fixtures/v1 --profile <profile> \
              --provider azure-openai:gpt-4o \
              --embedding bge-m3 --rerank bge-reranker-v2-m3 \
              --out results/run-2026-07-10.json --junit
```

| Profile | Stages exercised | Runtime | LLM cost | When |
|---|---|---|---|---|
| `retrieval` | gold memories → embed → QA (§2.5, §2.4) | minutes | none (local models only) | every PR; **the embedding bake-off profile** |
| `pipeline` | ingest → extract → resolve → contradict → promote, scored against §2.1–2.3 | ~1 h | moderate | nightly, per provider |
| `docs` | compose pipeline against §2.6 | ~15 min | small | nightly |
| `full` | everything, including end-to-end (raw transcripts in → QA answered out) | hours | full | pre-release, per provider (support matrix) |

Key property of `retrieval`: it starts from **gold memories**, not extracted ones — isolating embedding/reranker quality from extraction noise. The `full` profile then measures the compounded system, and the delta between the two tells you where quality is being lost.

### 3.1 Embedding bake-off protocol

Grid over your local candidates × rerank on/off, on the `retrieval` profile:

| Axis | Values |
|---|---|
| Embedding model | bge-m3, gte-large-en-v1.5, gte-Qwen2-1.5B-instruct, nomic-embed-text-v1.5, jina-embeddings-v3, multilingual-e5-large |
| Reranker | none, bge-reranker-v2-m3, jina-reranker-v2 |
| Fusion | vector-only (baseline), hybrid RRF |

Decision table output per configuration: NDCG@10 overall + per stratum (watch **exact-identifier** — it justifies hybrid; **Czech** — it will eliminate English-only models; **cross-team graph** — it validates the product thesis), index size, embed throughput (docs/s CPU vs GPU), retrieval p95. Expect the winner to differ between "best NDCG" and "best NDCG per CPU-millisecond" — pick for the self-hosted-on-modest-hardware story, and let rerank close the gap.

### 3.2 CI gates (fixture v1 initial thresholds — recalibrate after first stable baseline)

| Metric | Gate |
|---|---|
| False merges (auto-link) | = 0 |
| RLS leaks (retrieval + docs) | = 0 |
| Doc hallucination (auto-published) | = 0 |
| Extraction F1 | ≥ baseline − 2 pts |
| B³ F1 (entity resolution) | ≥ baseline − 2 pts |
| NDCG@10 overall | ≥ baseline − 1 pt |
| NDCG@10 cross-team stratum | ≥ flat-vector baseline + 5 pts (thesis check) |
| Temporal rank-1 accuracy | ≥ baseline − 2 pts |
| Retrieval p95 (hybrid+rerank, reference hardware) | ≤ 400 ms |

Results JSON is append-committed to `results/history/` so score trajectories are diffable in Git — the same discipline the product itself preaches.

### 3.3 LLM-as-judge hygiene

- Judge model pinned per fixture version; judge prompts versioned in-repo; judge never shares a provider with the system-under-test in `pipeline`/`full` runs (avoid self-preference bias).
- 10% of judge verdicts double-checked by a second judge model; disagreement rate reported — if it drifts > 5%, human re-adjudication of the sample before trusting the run.
- Human spot-check protocol: every fixture-version release includes a signed-off manual review of 50 random judge decisions.

---

## 4. Authoring plan & sequencing

1. **Week 1:** hand-author the 12 collision sets, contradiction cases, and 3 seed transcripts end-to-end — these define the difficulty calibration.
2. **Week 1–2:** LLM-expand to full transcript corpus from seed patterns; human review pass on all gold labels (est. 2–3 person-days; this is the highest-leverage manual work in the project).
3. **Week 2:** implement `retrieval` profile first → unblocks the embedding bake-off before any pipeline code exists (gold memories can be embedded directly).
4. Then `pipeline`, `docs`, `full` profiles as the corresponding system components land.

The fixture suite is also your best demo asset: Meridian is a fully explorable sample org for screenshots, sales demos, and the docker-compose trial seed data — three uses for one dataset.
