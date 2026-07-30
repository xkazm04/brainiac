# The test environment

Until now "test" was advice: `.env.example` documents `BRAINIAC_ENV=test` and
`docs/EVAL.md` describes the fixture corpus, but there was no host you could
point at and call the test environment. `docker-compose.test.yml` is that host.

It runs the **same three containers as production** — Postgres + `serve
--with-worker` + the console — with two differences that are the entire point:

1. **`BRAINIAC_ENV=test`**, so Sentry initializes (production-shaped error
   reporting) and every event is tagged `environment=test`.
2. **Everything is named and ported so it collides with nothing**, so a test
   stack, the local dev stack and even a prod stack can share one box.

| | local dev (`docker-compose.yml`) | **test** (`docker-compose.test.yml`) | production (`docker-compose.deploy.yml`) |
|---|---|---|---|
| compose project | `brainiac` | **`brainiac-test`** | `brainiac` |
| Postgres | `5433` | **`127.0.0.1:5434`** | not published |
| server | run from cargo, `8600` | **`127.0.0.1:8601`** | not published (`expose` only) |
| console | `3100` (`npm run dev`) | **`3200`** | `80` |
| pg volume | `brainiac-pg` | **`brainiac-pg-test`** | `brainiac-pg` |
| images | n/a | **built from source** | pulled from GHCR |
| `BRAINIAC_ENV` | `local` (Sentry off) | **`test`** | `production` |
| LightTrack | optional local sink | **the designed home** | skipped by policy |

The images are **built, not pulled** — the inverse of production. A test host
exists to run the commit you are working on, which usually predates any tag.

## Run it

```bash
cp .env.test.example .env.test        # then fill it in — see below
docker compose -f docker-compose.test.yml --env-file .env.test up -d --build
./scripts/smoke-test-env.sh
```

Tear down, keeping the test corpus:

```bash
docker compose -f docker-compose.test.yml --env-file .env.test down
```

…or including it (`-v` destroys `brainiac-pg-test`, and only that volume — the
dev and prod volumes have different names):

```bash
docker compose -f docker-compose.test.yml --env-file .env.test down -v
```

## Configure it — `.env.test`

`.env.test.example` is the full contract; each variable says where its value
comes from. The short version:

| Variable | Source | Note |
|---|---|---|
| `POSTGRES_PASSWORD` | generate (`openssl rand -base64 24`) | required; container-local |
| `BRAINIAC_TOKENS` | generate | required. Give test its **own** org/user UUIDs — a test run that writes memories must not be able to land in the production org's namespace |
| `BRAINIAC_API_TOKEN` | the map above | required; the console's server-side bearer, and what `smoke-test-env.sh` reads |
| `CONSOLE_PASSCODE` | generate | required; keeps the gate fail-closed |
| `CONSOLE_SESSION_SECRET` | generate | **required** — without it the session cookie is keyed on the passcode itself |
| `QWEN_API_KEY` (or another provider's) | vendor dashboard | use a *separate* key from production where the vendor allows it, so test traffic is its own line on the bill and can be revoked alone |
| `SENTRY_DSN`, `NEXT_PUBLIC_SENTRY_DSN` | Sentry project settings | optional; unset ⇒ no client initializes |
| `LIGHTTRACK_URL`, `LIGHTTRACK_KEY` | your LightTrack instance | optional; unset ⇒ the gateway records nothing |

If CI brings this stack up, the same names go into the CI secret store and the
job writes `.env.test` from them.

Don't want to spend tokens at all? `BRAINIAC_EMBEDDER=deterministic` needs no
API key (~0.19 NDCG lower — fine for plumbing and smoke work, not for eval).

## Verify it — the smoke check

```bash
./scripts/smoke-test-env.sh
```

Every check hits an endpoint that exists in the source
(`crates/brainiac-server/src/http.rs`, `console/middleware.ts`):

| # | Request | Expect | What it actually proves |
|---|---|---|---|
| 1 | `GET /health` | `200 {"status":"ok"}` | the axum process is up. The body is asserted, not just the status, so a proxy answering 200 cannot fake it |
| 2 | `GET /v1/queue/health` *(no token)* | `401` | the auth layer is on. A 200 here would mean tokens are bypassed |
| 3 | `GET /v1/queue/health` *(admin bearer)* | `200` with `queue` / `ready` / `in_flight` | the deep one: the token resolved, Postgres is reachable, `sqlx::migrate!` ran, and `queue.jobs` is readable. This is the check that the *data plane* works, not just the process |
| 4 | `GET /openapi.json` | `200` | the OpenAPI document the console's generated types come from |
| 5 | `GET <console>/` | `200` | the Next standalone runtime is serving the public pitch |
| 6 | `GET <console>/api/status` | `401` | the console gate fails **closed**: `/api/*` returns 401 JSON rather than an HTML login page, exactly as in production |

It waits up to `SMOKE_RETRIES` (60s) for `/health` before failing, prints
`ok`/`FAIL` per check, and exits non-zero if any failed — so it drops straight
into CI. Point it anywhere:

```bash
API_URL=http://127.0.0.1:8600 CONSOLE_URL=http://127.0.0.1:3100 ./scripts/smoke-test-env.sh
```

The one-liner equivalent of check 1, if you just want a pulse:

```bash
curl -fsS http://127.0.0.1:8601/health
```

## What you'd actually use it for

- **Pre-release rehearsal** of the exact compose topology production runs,
  including first boot: `migrate()` against an empty volume, then
  `scripts/first-boot.sh` to get off the break-glass token.
- **Sentry and LightTrack with real traffic.** Production skips LightTrack by
  policy and you do not want to learn your DSN is wrong from a production
  incident.
- **Provider swaps** (`docs/deploy/providers.md`) — point test at a different
  `BRAINIAC_PROVIDER` or per-stage `BRAINIAC_MODEL_*` and compare before
  moving production.
- **Migration and restore rehearsal.** Postgres is published on
  `127.0.0.1:5434`, so the backup scripts work against it verbatim:
  `COMPOSE_FILE=docker-compose.test.yml ./scripts/backup.sh`.

## Known gaps — what the operator still owns

- **Browser-side Sentry does not initialize.** Next inlines `NEXT_PUBLIC_*` at
  **build** time, and `console/Dockerfile` declares no build `ARG`s for them, so
  `NEXT_PUBLIC_SENTRY_DSN` reaches the Node runtime but not the browser bundle.
  Server-runtime and edge Sentry work; client-side error capture needs the
  Dockerfile to take these as build args first. The Rust server's Sentry is
  unaffected.
- **Nothing seeds the corpus.** A fresh test stack is empty. Load the synthetic
  Meridian org yourself (`fixtures/v1`, `docs/EVAL.md`) or restore a scrubbed
  dump.
- **No TLS and no reverse proxy.** Postgres and the API bind to loopback; the
  console publishes `3200` on all interfaces. Put it behind your firewall or a
  security group — this file does not.
- **No resource limits.** Unlike `docker-compose.deploy.yml` (sized for a 1 vCPU
  / 1 GB free-tier box), nothing here is capped. Add `deploy.resources.limits`
  if the test box is small, or expect the Rust build to dominate it.
- **The eval gates are not wired in.** `cargo run -p brainiac-server -- eval` is
  still something you run against fixtures (`CLAUDE.md` § Quality standards);
  the test stack does not run it for you.
- **Secrets rotation and cleanup.** Test credentials are real credentials.
  Rotate them, and never reuse a production org id or API key here.

## Related

- `docs/deploy/backups.md` — backup, offsite and restore drills
- `docs/deploy/providers.md` — the LLM provider matrix
- `docs/EVAL.md` — the fixture corpus and regression gates
