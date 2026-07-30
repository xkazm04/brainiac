# Zero-cost deploy: self-host pgvector, stay in Singapore, and the trial is one-shot

`docs/deploy/alibaba.md` is a verified end-to-end free deploy recipe. The one
decision that shapes the whole architecture: **managed Postgres (ApsaraDB RDS)
is the ONLY component that would cost money.** RDS supports pgvector but has no
always-free tier. The corpus is under 1 GB, so self-host
`pgvector/pgvector:pg17` in a container instead.

**Shape:** one small VM, `docker-compose.deploy.yml`, three containers —
postgres (RLS + queue + vectors), server running `serve --with-worker` on
:8600 (in-process worker saves a whole Tokio runtime and connection pool on a
1 GB box), console (Next.js standalone) on :80. Migrations run automatically at
boot via the embedded sqlx migrator. Measured idle+serving RAM is ~93 MiB total
(postgres 41, server 2, console 50) — the compose limits are guard rails, not a
tight fit. Still add 2 GB swap: Postgres grows during a big re-embed and Next
SSR spikes on cold routes.

**Region must be Singapore (ap-southeast-1)** for two independent reasons: ICP
filing applies only to mainland China regions, and the provider's free Qwen
quota is Singapore-only. Keep compute and LLM in the same region.

**Gotchas that cost hours:**

1. **The ECS free trial is ONE-SHOT.** Buying *any* compute instance — even a
   cheap "test box" — before claiming it forfeits new-compute-user eligibility.
   Pick one path and stay on it.
2. **Credits ≠ free tier.** New-account credit pools expire in 60 days and
   "solution trials" cap at 168 hours; neither substitutes for the 12-month
   trial.
3. The free model quota is Singapore-only, 90 days, 1M tokens per model,
   real-time inference only (no batch/fine-tune), and does not renew.
4. Virtual credit cards are rejected at signup.
5. Don't build on Function Compute's "permanent free tier" — it was cancelled.
   Don't substitute AnalyticDB/Tair/Lindorm for Postgres either: none is
   pgvector, so RLS + the transactional queue + plain SQL wouldn't port.

**Offline fallback if a quota runs dry mid-demo:** `BRAINIAC_EMBEDDER=
deterministic` (local hashed bag-of-words, no API — also the default when
nothing is set) plus `worker --mock` / `serve --with-worker --mock` run
everything except real LLM and embedding calls with zero external deps. Cost:
roughly 0.19 NDCG (0.685 vs 0.876 with a real embedding model). Budget for the
gap; don't quote deterministic-embedder numbers as product quality.
