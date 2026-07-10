# Brainiac — Implementation Plan (baseline build)

Goal of this plan: reach a **working baseline where every architecture component
cooperates end-to-end** on the Meridian golden fixtures, in production-quality
Rust, before any feature expansion or benchmarking. `docs/ARCHITECTURE.md` is
the contract; `docs/EVAL.md` defines how quality is measured. Deviations from
the architecture are recorded in §Deviations below — the doc stays authoritative.

## Phase ladder (each phase ends with green tests + one commit)

| # | Phase | Deliverable | Test gate |
|---|---|---|---|
| P0 | Scaffold | Cargo workspace (7 crates), docker-compose (Postgres 17 + pgvector), CI (fmt, clippy -D warnings, tests), docs | `cargo check` clean |
| P1 | Core domain | `brainiac-core`: domain types (§2 of architecture), temporal validity + supersession logic, RRF fusion, eval metrics (NDCG@k, MRR, Recall@k, pairwise & B³ F1) | pure unit tests |
| P2 | Fixtures v1 seed | `fixtures/v1/` Meridian seed (collision sets + near-miss traps, contradictions, temporal chains, seed transcripts w/ gold, retrieval QA incl. RLS-leak cases) + `brainiac-fixtures` loader with referential-integrity validation | loader validates the whole fixture tree |
| P3 | Store | sqlx migrations for schema §2 + RLS policies + SKIP-LOCKED job queue; `brainiac-store` repos with per-request principal (`app.org_id`/`app.user_id`) | integration tests vs dockerized PG (skipped without `DATABASE_URL`) |
| P4 | Retrieval | Hybrid engine: pgvector ANN + Postgres FTS → RRF → 1–2 hop graph expansion → assembly (supersession dedupe, `as_of`); deterministic test embedder; eval `retrieval` profile runs on gold memories | NDCG/temporal/leak metrics produced; **RLS leak = 0** hard gate |
| P5 | Gateway + pipeline | BYOM gateway (provider trait; **Qwen/DashScope adapter** + deterministic MockProvider), workers: ingest → extract → embed → resolve → contradict → promote over the job queue, promotion policy engine w/ audit rows | `pipeline` profile vs fixtures with MockProvider; false-merge = 0 gate |
| P6 | Server | Single binary `brainiac serve|worker|eval`; axum REST (memories, search, review queues, approve/reject), bearer-token principal stub | HTTP smoke tests |
| P7 | CI eval gates | `retrieval` profile in CI, results JSON append-committed to `results/history/` | thresholds of EVAL.md §3.2 wired |

**After baseline** (explicitly out of scope for this plan): embedding/reranker
bake-off (EVAL.md §3.1), MCP server surface, document layer (§8), Next.js
console, OIDC/SAML + SCIM, Helm chart, Apache AGE, reranker stage, decay
scoring. Fixture corpus expansion to the full v1 dimensions (40 transcripts,
~450 gold memories) happens alongside benchmarking — the seed corpus is sized
to exercise every code path, not to saturate the metrics.

## Crate map

```
crates/
├── brainiac-core       # domain types + pure algorithms (no IO): temporal, fusion, metrics, embed trait
├── brainiac-fixtures   # Meridian YAML schemas, loader, integrity validation
├── brainiac-store      # Postgres: migrations, repos, RLS session, job queue
├── brainiac-gateway    # BYOM: ChatProvider trait, Qwen (DashScope), Mock, token budgets
├── brainiac-pipeline   # workers (extract/resolve/contradict/promote/embed) over the queue
├── brainiac-eval       # profiles (retrieval|pipeline), metric aggregation, gates, CLI lib
└── brainiac-server     # the single binary: serve | worker | eval
```

## Deviations from ARCHITECTURE.md (deliberate, revisitable)

1. **Job queue**: v0 ships our own `queue` schema (SKIP LOCKED + visibility
   timeout + archive), API-shaped after pgmq. Rationale: zero extension risk on
   arbitrary Postgres images (pgmq needs the extension installed); swapping to
   real pgmq later is a store-layer change only.
2. **Policy engine**: v0 promotion policies are typed Rust rules stored as data
   (JSON per org) with `policy_rule` audit strings mirroring the Cedar intents
   in §2.5. Cedar lands when the policy surface stabilizes — the evaluation
   seam (`PolicyEngine` trait) is already shaped for it.
3. **Identity**: v0 principal = static bearer-token → (org, user, teams) map
   from config. OIDC/SCIM replaces the resolver behind the same trait.
4. **Embeddings**: v0 default is a deterministic local embedder (hashed
   bag-of-tokens projection) so the whole system + eval harness run with zero
   model downloads; real open models plug in behind `Embedder` for the
   bake-off. Metric numbers with the deterministic embedder are *plumbing
   numbers*, not quality claims.
5. **Blob storage**: raw source text is stored in Postgres (`sources.raw_text`)
   for v0; S3/MinIO lands with the transcript-upload connector.

## Status log

- [x] P0 scaffold
- [x] P1 core domain
- [x] P2 fixtures v1 seed + loader
- [x] P3 store (schema, RLS, queue)
- [x] P4 retrieval + eval `retrieval` profile
- [x] P5 gateway (Qwen + mock) + pipeline workers + `pipeline` profile
- [x] P6 server binary + REST
- [x] MCP agent surface (`brainiac mcp`): stdio JSON-RPC server, tools
      `memory_search` / `memory_context` / `memory_add` / `entity_lookup`;
      identity via `BRAINIAC_MCP_TOKEN` resolved through the same token map as
      REST — an agent can never read more than its operator. Tested in
      `crates/brainiac-server/tests/mcp_pg.rs` (handshake, tool list, RLS leak
      check, entity resolution, ingest).
- [x] Embedding backend seam live: async `Embedder`, `QwenEmbedder`
      (DashScope `text-embedding-v4`, 1024-d, same OpenAI-compatible base as
      the chat provider; `--embedder qwen` / `BRAINIAC_EMBEDDER`). First
      real-model baseline: **NDCG@10 0.876** (semantic 0.811, czech 0.785)
      vs deterministic 0.685 — results/history/.
- [x] Console REST slice: promotion approve/reject + contradiction resolve
      (maintainer-of-owning-team gate), `/v1/graph`, `/v1/analytics`.
- [x] Next.js console scaffold (`console/`): server-only typed API client
      (vitest-covered), reviews/graph/analytics pages with server actions.
      Deliberately unstyled — the visual-identity pass replaces the chrome.
- [ ] P7 CI eval gates wired to thresholds
