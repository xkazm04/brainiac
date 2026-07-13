# Shipping Brainiac free on the Alibaba stack

**The verdict: yes — the whole product runs at $0 for a hackathon, and $0
infrastructure for 12 months.** One caveat that decides the architecture:
**managed Postgres is the only thing that would cost money.** ApsaraDB RDS
for PostgreSQL *does* support pgvector (PG 14–17, HNSW + IVFFlat), but it has
no always-free tier — a managed-DB architecture (RDS + SAE + Function
Compute) runs ~$30+/month. Self-hosting `pgvector/pgvector:pg17` in a
container on a free-trial VM sidesteps that entirely, and our corpus is
under a gigabyte.

So: **one ECS free-trial instance, docker-compose, three containers.**

```
ECS free-trial instance · Singapore (ap-southeast-1)
└── docker compose -f docker-compose.deploy.yml
    ├── postgres   pgvector/pgvector:pg17   (RLS, queue schema, vectors)
    ├── server     brainiac serve --with-worker   :8600
    │                └── egress → DashScope Singapore (Qwen LLM + embeddings)
    └── console    Next.js standalone             :80
```

## Why this shape

| Decision | Reason |
|---|---|
| **ECS free trial**, not Function Compute | FC's permanent free tier was **cancelled in Dec 2022**; new users get a small one-off allowance. It's also a poor fit for a long-lived pipeline worker. ECS gives 12 months free (1 vCPU / 1 GB individual; 2c/2 GB enterprise). |
| **Self-hosted Postgres**, not RDS | RDS PG supports pgvector but isn't free. Our data is <1 GB and lives fine on the instance's system disk. |
| **Not AnalyticDB / Tair / Lindorm** | AnalyticDB has a documented free trial and a vector engine, but it's MPP-shaped and **isn't pgvector** — our `vector` columns, RLS policies and transactional `queue` schema wouldn't port cleanly. Tair/Lindorm would cost us SQL, RLS and transactions. Not worth it. |
| **Singapore (ap-southeast-1)** | Two reasons: **ICP filing applies only to mainland regions** (deploy outside mainland and it's a non-issue), and **Model Studio's free quota is Singapore-only**. Keep compute and LLM in the same region. |
| **`serve --with-worker`** | The free instance is **1 GB of RAM**. Running the pipeline as a task inside the API process instead of a second container saves a whole Tokio runtime and connection pool. |

## The RAM budget (the real constraint)

1 GB, honestly accounted:

| Container | Limit | Note |
|---|---|---|
| postgres | 320 MB | tuned: `shared_buffers=128MB`, `max_connections=40` (stock PG wants ~400 MB+) |
| server + worker | 256 MB | one Rust process; rustls, no OpenSSL |
| console (Next.js SSR) | 320 MB | `--max-old-space-size=256` |
| host + docker | ~100 MB | |

That fits, but with little headroom — **add 2 GB of swap** (below). If it
thrashes anyway, the cheapest escape is a **Simple Application Server**
(~$3.50/month, 2c/2 GB) — but note that **buying one may forfeit your ECS
free-trial eligibility**, so pick one path and stay on it.

## Costs after the free window

- **Infrastructure:** free for 12 months (ECS trial). After that, a 2c/2 GB
  box is a few dollars a month.
- **Qwen (Model Studio):** **1M free tokens per model**, valid **90 days**
  from activation, Singapore/international endpoint, real-time inference
  only (no batch/fine-tuning). It does **not** renew. After that it's
  pay-per-token, and it's cheap: `text-embedding-v4` is ~$0.07/1M tokens and
  `qwen-plus` is in the cents-per-demo range.
- Our whole eval corpus (82 memories, 83 queries) costs well under 100k
  tokens to embed and run — the free quota is not a constraint at hackathon
  scale.

## Recipe

### 1. Account + instance

- Sign up at **alibabacloud.com** (international), *not* aliyun.com.
- A **real credit card is required** (Visa/Mastercard/Amex/JCB — virtual
  cards are rejected). A temporary authorization hold is placed; no charge.
- Claim the **ECS free trial** — 12 months. **Do not buy any ECS or Simple
  Application Server instance first**: any purchase forfeits "new compute
  user" eligibility, and the trial is one-shot.
- Region: **Singapore (ap-southeast-1)**. Image: Ubuntu 22.04/24.04.
- Security group: open **80** (console) and **22** (ssh). Leave 8600 closed —
  the console reaches the server over the Docker network, not the internet.
- Check whether the trial bundles public bandwidth; it's often billed
  separately from the instance.

### 2. Host prep

```bash
# Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER && newgrp docker

# Swap — the 1 GB instance needs it
sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile
sudo mkswap /swapfile && sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

### 3. Deploy

```bash
git clone https://github.com/xkazm04/brainiac && cd brainiac
cp .env.deploy.example .env.deploy
# Fill in: POSTGRES_PASSWORD, QWEN_API_KEY, BRAINIAC_TOKENS, BRAINIAC_API_TOKEN
nano .env.deploy

docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d --build
```

The Rust image builds in ~10 minutes on a 1-core box (release LTO). If that's
painful, build locally and push to **ACR** (Alibaba Container Registry — a
free personal edition exists) or just `docker save | ssh … docker load`.

Migrations run automatically at server boot (embedded sqlx migrator). The
console comes up on port 80.

### 4. Seed something to show

```bash
# Meridian fixtures + a real retrieval run, straight into the live DB
docker compose -f docker-compose.deploy.yml exec server \
  brainiac eval --profile retrieval --embedder qwen --out /tmp/run.json
```

That populates the org and gives you honest numbers to point at (NDCG@10
0.876 with `text-embedding-v4` — see `results/history/`).

## Gotchas that cost hours

1. **The free trial is one-shot.** Buying *any* compute instance before
   claiming it forfeits eligibility. Don't "just spin up a test box."
2. **Credits ≠ free tier.** New-account credit pools expire in **60 days**,
   and "Solution Trials" cap at **168 hours** — neither substitutes for the
   12-month ECS trial.
3. **Model Studio free quota is Singapore-only** and **90 days**. If your
   ECS is in another region your LLM calls still work, but you're paying
   cross-region latency for no reason.
4. **ICP filing** is a mainland-China thing. Singapore = not applicable.
5. **Virtual credit cards are rejected** at signup.
6. **Judges want proof.** Keep the ECS console screenshot and a public URL;
   `/demo` is a static, token-free page that works even if the API is down.

## If the API key is the only thing you have

Everything except the LLM/embedding calls runs with **zero external
dependencies**: `BRAINIAC_EMBEDDER=deterministic` uses a local hashed
bag-of-words embedder (no API), and `brainiac worker --mock` runs the
pipeline with a deterministic mock provider. The product still demonstrates
governance, retrieval, the graph and the console — it just scores ~0.19 NDCG
lower (0.685 vs 0.876). That's the offline fallback if a quota runs dry
mid-demo.

## Unverified

The research behind this doc could not confirm from primary sources: exact
RDS PostgreSQL pricing / whether a new-user RDS trial exists; SAE / ACK
Serverless / ECI free tiers (assume paid); the current new-account credit
amount. None of them change the recommendation — they're all alternatives to
a path that is already free.
