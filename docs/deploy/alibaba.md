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

## The RAM budget — measured, not guessed

The 1 GB instance was the thing I expected to break. It doesn't. Booting the
exact `docker-compose.deploy.yml` locally and serving pages:

| Container | Limit | **Measured (idle, serving)** |
|---|---|---|
| postgres (tuned: `shared_buffers=128MB`, `max_connections=40`) | 320 MB | **41 MiB** |
| server + in-process worker (Rust, rustls) | 256 MB | **2 MiB** |
| console (Next.js SSR, `--max-old-space-size=256`) | 320 MB | **50 MiB** |
| **total** | | **≈ 93 MiB** |

That leaves ~900 MB of headroom on the free instance. The limits above are
guard rails, not a tight fit. Still **add 2 GB of swap** (below) — Postgres
grows under a big re-embed, and Next SSR spikes on cold routes — but the
"1 GB is too small" worry turned out to be unfounded.

If it *does* thrash, the cheapest escape is a **Simple Application Server**
(~$3.50/month, 2c/2 GB) — but note that **buying one may forfeit your ECS
free-trial eligibility**, so pick one path and stay on it.

Both images are built and verified: **server 166 MB** (multi-stage → Debian
slim; rustls means no OpenSSL in the runtime layer), **console 481 MB**
(Next standalone).

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
# Fill in: POSTGRES_PASSWORD, QWEN_API_KEY, BRAINIAC_TOKENS, BRAINIAC_API_TOKEN,
# CONSOLE_PASSCODE (+ CONSOLE_SESSION_SECRET), BRAINIAC_CONSOLE_URL
nano .env.deploy

docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d --build
```

Verified locally end to end: console answers on :80, `/analytics` renders
from the live API through the internal Docker network (not the demo
fallback), `/demo` serves token-free, and the pipeline worker starts inside
the server process.

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

> **Fail-fast note.** `docker-compose.deploy.yml` marks `POSTGRES_PASSWORD`,
> `QWEN_API_KEY`, `BRAINIAC_API_TOKEN` and `CONSOLE_PASSCODE` as required
> (`${VAR:?...}`). If any is missing, `up` aborts before starting a container —
> so a half-configured deploy stops loudly instead of silently serving only the
> public pages.

## Auth & the break-glass token

The identity model has two tiers (see `crates/brainiac-server/src/auth.rs`):

- **Env bootstrap tokens** (`BRAINIAC_TOKENS`) — the JSON map key is a bearer
  secret mapped to a principal. These carry **every scope and bypass the scope
  system entirely** (`scopes = None`), and they are **un-revocable and
  un-audited**. They are your **break-glass** credentials — the ones that mint
  everything else.
- **Managed API tokens** (`brk_…`, minted via `POST /v1/tokens`) — stored hashed
  (sha256), **scoped**, **revocable**, and `last_used_at`-tracked. Mintable
  scopes are exactly: `read`, `write`, `kb:read`, `kb:publish`, `lib:read`,
  `lib:propose`, `lib:publish`, `admin` (`admin` implies all).

**The intended flow — don't run day-to-day on the break-glass token:**

1. **Boot** with a strong break-glass secret in `BRAINIAC_TOKENS`, and set
   `BRAINIAC_API_TOKEN` to it for the first start.
2. **Mint a scoped key.** Minting requires the `admin` scope, which the env token
   has. The server's `8600` is on the Docker network only (never the public
   internet — see the security-group note above), so reach it from a throwaway
   curl container that shares the server's network namespace:

   ```bash
   SERVER=$(docker compose -f docker-compose.deploy.yml ps -q server)
   docker run --rm --network "container:$SERVER" curlimages/curl -sS \
     -X POST http://localhost:8600/v1/tokens \
     -H "Authorization: Bearer $BRAINIAC_API_TOKEN" \
     -H 'Content-Type: application/json' \
     -d '{"name":"console","scopes":["read","write","kb:read","kb:publish","lib:read","lib:propose","lib:publish"]}'
   ```

   The `201` response contains `"token":"brk_…"` — the plaintext secret, shown
   **once and never retrievable again**. Copy it now.
3. **Point the console at the scoped key.** Set `BRAINIAC_API_TOKEN=brk_…` in
   `.env.deploy` and `docker compose -f docker-compose.deploy.yml up -d console`.
   The console now runs on a revocable, auditable key instead of break-glass.
4. **Store the break-glass secret offline** (password manager / access-controlled
   `.env.deploy`, which is git-ignored). Use it only to mint, rotate, or revoke
   keys. Revoke a leaked key with `POST /v1/tokens/{id}/revoke`; list keys with
   `GET /v1/tokens`.

**Scope guidance for the console key.** The scopes above (everything except
`admin`) let the console do its knowledge-plane work — reads, writes, KB, and
library — while a leaked console key **cannot mint further keys**. The trade-off:
the console's own key-management (`/keys`), org directory, and blast-radius
preview require `admin` and will `403` under a non-admin key. If you want those
modules to work in-console, mint the console key with `admin` instead — still a
strict upgrade over the env token, because it is revocable and audited. Either
way, key management can always be done with the offline break-glass token via the
curl above.

## Backup & restore

The whole corpus is a single Postgres database in the `brainiac-pg` volume —
back it up with a logical dump, not by copying files out from under a running
server.

**Cadence.** A daily `pg_dump` is plenty at this scale. `scripts/backup.sh`
writes a compressed custom-format archive to `./backups/` (auto-git-ignored) and
prunes dumps older than 14 days (`RETENTION_DAYS`). Wire it into cron on the ECS
box:

```bash
# daily at 03:15, keep 14 days (the script's default)
15 3 * * * cd /home/ubuntu/brainiac && ./scripts/backup.sh >> /var/log/brainiac-backup.log 2>&1
```

Run it by hand the same way (defaults to the deploy stack):

```bash
./scripts/backup.sh                                   # -> ./backups/brainiac-<ts>.dump
COMPOSE_FILE=docker-compose.yml ./scripts/backup.sh   # the local dev stack instead
```

**Restore drill.** Stop the API so nothing holds locks, restore, restart:

```bash
docker compose -f docker-compose.deploy.yml stop server
./scripts/restore.sh backups/brainiac-<ts>.dump --yes
docker compose -f docker-compose.deploy.yml start server
```

`restore.sh` is destructive (`pg_restore --clean --if-exists` drops and recreates
the dumped objects); without `--yes` it prompts before touching anything. Both
scripts take the same `COMPOSE_FILE` / `PG_SERVICE` / `PG_USER` / `PG_DB`
overrides, so the identical drill works against the local stack.

**Volume-snapshot alternative.** For a byte-exact copy (e.g. before a risky
migration), stop the stack for consistency and tar the named volume — confirm
the name first with `docker volume ls`:

```bash
docker compose -f docker-compose.deploy.yml down
docker run --rm -v brainiac_brainiac-pg:/data -v "$PWD/backups":/out alpine \
  tar czf /out/pgdata-$(date +%F).tgz -C /data .
```

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
