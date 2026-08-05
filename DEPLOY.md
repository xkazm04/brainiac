# Deploy Brainiac — quick card

The whole stack (Postgres + server + console) runs on one small Docker host from
`docker-compose.deploy.yml`. This is the 60-second happy path; the full ECS
free-tier recipe, RAM budget, backups, and auth hardening live in
[`docs/deploy/alibaba.md`](docs/deploy/alibaba.md).

## Happy path — fresh box to serving

```bash
# 1. Docker + swap (the 1 GB free-tier box needs the swap; skip on a bigger host)
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER && newgrp docker
sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile

# 2. Clone + configure
git clone https://github.com/xkazm04/brainiac && cd brainiac
cp .env.deploy.example .env.deploy
nano .env.deploy            # fill the vars below (each has a gen hint inline)

# 3. Pull the released images + start (~1 min; nothing is compiled on the box)
docker compose -f docker-compose.deploy.yml --env-file .env.deploy pull
docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d

# 4. Verify — that the PRODUCT works, not that a page renders
docker compose -f docker-compose.deploy.yml ps          # all three healthy
curl -sf http://localhost/ >/dev/null && echo "console up"   # pitch page, token-free
./scripts/verify-retrieval.sh    # write a memory -> pipeline -> search it back

# 5. Get off the break-glass admin token (see "Harden auth" below)
./scripts/first-boot.sh
```

**Do not stop at `curl http://localhost/`.** That page never calls the API, so
it stays green through a dead database, a wrong `BRAINIAC_API_TOKEN`, an
embedder with no credit, and a worker that never started — each of which breaks
the product completely. `scripts/verify-retrieval.sh` exercises the actual
claim end to end: `POST /v1/memories` → poll `GET /v1/sources/{id}` until the
pipeline reports `processed` → `POST /v1/memories/search` and assert the marked
memory comes back. That covers auth, scopes, RLS, the Postgres write path, the
job queue, the worker, the embedder and hybrid retrieval. It talks to the
server through a throwaway container on the compose network (8600 is not
public) and prints no secret. It **leaves one memory behind** — self-labelled
and carrying its marker, because a check that writes nothing cannot prove the
write path; find it by the marker in the console's archive module and deprecate
it if you would rather it were not there.

For liveness-only checks that write nothing (health, auth-is-on, OpenAPI, the
console gate), `./scripts/smoke-test-env.sh` is the complementary script.

> ⚠ **Verify with a principal that belongs to a team.** A memory defaults to
> `visibility = 'team'` and takes its team from the writer's **first** team. The
> `BRAINIAC_TOKENS` example ships `"teams": []`, and a teamless writer produces
> memories with `team_id = NULL` — which the RLS read policy
> (`migrations/0001_init.sql`, `memories_read`) matches with
> `team_id IN (…)`. `NULL` matches nothing, for anyone, permanently. The write
> returns 202, the pipeline reports `processed`, and the memory is unreadable
> by every principal in the org including the one that wrote it. Put a real
> team uuid in the token's `teams` array **and** ensure that user is a row in
> `team_members` for it — RLS reads the table, not the token.
> `verify-retrieval.sh` detects exactly this case and says so.

`BRAINIAC_IMAGE_TAG` (default `latest`) picks the version; pin an exact tag such
as `v0.3.0` for a reproducible host. Images are published to
`ghcr.io/xkazm04/brainiac` and `…/brainiac-console` by
[`.github/workflows/release.yml`](.github/workflows/release.yml) on every `v*` tag.

**Fallback — build from source** (an unreleased commit, a fork, or an
architecture the release workflow doesn't publish). `docker-compose.build.yml`
is an override that adds the `build:` stanzas back; expect ~10 min of Rust
release LTO on a 1-core box:

```bash
docker compose -f docker-compose.deploy.yml -f docker-compose.build.yml \
  --env-file .env.deploy up -d --build
```

A **Caddy** container terminates TLS and is the only publicly bound service; the
console and the server's 8600 both stay on the Docker network. Set
`BRAINIAC_DOMAIN` and Caddy provisions/renews a Let's Encrypt certificate itself
and redirects 80 → 443; leave it empty and you get plain HTTP on 80, which is all
a bare-IP host can do. Open **80**, **443** and **22** in the security group;
leave **8600** closed. Migrations run automatically at server boot.

## Environment variables (`.env.deploy`)

| Var | What it is |
| --- | --- |
| `BRAINIAC_IMAGE_TAG` | GHCR image tag to run — `latest` (default) or a pinned `vX.Y.Z`. |
| `POSTGRES_PASSWORD` | Postgres password (container-local). **Required.** |
| `QWEN_API_KEY` | DashScope / Model Studio key (Singapore). **Required.** |
| `QWEN_BASE_URL` | DashScope endpoint (default is the Singapore compatible-mode URL). |
| `QWEN_MODEL` | Extraction model — `qwen-plus` (default), `qwen-max`, or `qwen-flash`. |
| `BRAINIAC_EMBEDDER` | `qwen` (default, best) or `deterministic` (no API, ~0.19 NDCG lower). |
| `BRAINIAC_TOKENS` | Break-glass admin map (bearer secret → principal). **Unrestricted — bootstrap only; delete it after `first-boot.sh`.** |
| `BRAINIAC_API_TOKEN` | Token the console uses. **Required.** Boot with the break-glass secret, then switch to a minted `brk_` key. |
| `CONSOLE_PASSCODE` | Shared login passcode for the console. **Required** — without it the console fails closed to `/` + `/demo`. |
| `CONSOLE_SESSION_SECRET` | Signs the session cookie independently of the passcode. **Required** — absent, the cookie is keyed on the passcode itself, so a guessed passcode forges a session without meeting the login rate limiter. |
| `BRAINIAC_CONSOLE_URL` | Public console URL (`http://<public-ip>`, or `https://<domain>` once TLS is on); onboarding approval links are built from it. |
| `BRAINIAC_DOMAIN` | Domain for automatic HTTPS. Empty ⇒ plain HTTP on 80. Set ⇒ Caddy gets and renews a Let's Encrypt cert and redirects 80 → 443. The A record must already resolve here, and 80 **and** 443 must be open (ACME validates over 80). |
| `SENTRY_DSN` | Error tracking. Read at **runtime** by the Rust server; baked in at **build time** for the console (see below). Unset ⇒ no error tracking. |
| `RUST_LOG` | Log level (default `info`). |

**Console error tracking is baked into the image.** Next.js inlines
`NEXT_PUBLIC_*` when `npm run build` runs, so `console/Dockerfile` takes
`SENTRY_DSN` / `BRAINIAC_ENV` as build args (fed by `docker-compose.build.yml`).
A **pulled** image therefore carries whatever DSN it was released with — to send
console errors to your own Sentry project you must build from source. The Rust
server is unaffected either way: it reads `SENTRY_DSN` at runtime.

**Harden auth before real use — `./scripts/first-boot.sh`.** The stack boots on
`BRAINIAC_TOKENS`, an env map whose entries carry every scope, bypass the scope
system entirely, and are un-revocable and un-audited. The script mints a scoped
`brk_` key via `POST /v1/tokens`, verifies it, rewrites `BRAINIAC_API_TOKEN`,
restarts the console, and then tells you to delete `BRAINIAC_TOKENS` and keep it
offline. It prints no secret values and is safe to re-run (a second run just
re-verifies). Use `TOKEN_NAME=… SCOPES=… ./scripts/first-boot.sh --to-file` for
agent/MCP keys. Background + the manual curl equivalent:
[`docs/deploy/alibaba.md` → Auth & the break-glass token](docs/deploy/alibaba.md#auth--the-break-glass-token).

**Back up daily:** `./scripts/backup.sh` (→ `./backups/`); restore with
`./scripts/restore.sh <dump>`. See
[`docs/deploy/alibaba.md` → Backup & restore](docs/deploy/alibaba.md#backup--restore).

## What can go wrong

1. **`up` aborts with `set POSTGRES_PASSWORD` (or `…API_TOKEN` / `…PASSCODE` /
   `…SESSION_SECRET`).**
   A required var is unset — that is the fail-fast guard, not a bug. Fill it in
   `.env.deploy` and re-run. Pass `--env-file .env.deploy` on **every** compose
   command (it is not the default file name).
2. **Console loads but shows demo/fallback data, or `/api/*` returns 401.** The
   console isn't authenticated to the live org: `CONSOLE_PASSCODE` unset (it fails
   closed), or `BRAINIAC_API_TOKEN` isn't a key in the `BRAINIAC_TOKENS` map / not
   a valid minted `brk_` key. Check `docker compose -f docker-compose.deploy.yml logs console`.
3. **Rust image build is slow or OOMs on a 1-core / 1 GB box.** It is ~10 min of
   release LTO — expected. If it OOMs, confirm the 2 GB swap from step 1 is
   active (`swapon --show`), or build elsewhere and `docker save | ssh … docker load`.
4. **`Error: parsing BRAINIAC_TOKENS`, in a restart loop.** `BRAINIAC_TOKENS` is
   **JSON**, not `secret:principal` — the whole map is one JSON object whose key
   is the bearer secret (see `.env.deploy.example` for the exact literal). The
   server exits before binding, so nothing answers on 80 and the only symptom is
   the loop in `docker compose logs server`.

## What this runbook has actually been proven to do

Recorded from a **local rehearsal** of `docker-compose.deploy.yml` (isolated
compose project, host ports remapped) so this file states evidence rather than
intent. Re-run the same steps on a real host to close the gaps below.

**Observed working:**

- The compose stack builds/starts and reaches a serving state: Postgres healthy,
  migrations applied automatically at server boot, server bound on 8600.
- The **network posture is what this file claims**: Caddy is the only
  host-published service; the console answers *through* Caddy (`GET /` → 200) and
  is **not** reachable directly on the host — so there is no port that bypasses TLS.
- `deploy/Caddyfile` passes `caddy validate` in all three states: domain set
  (auto-HTTPS + automatic 80 → 443 redirect confirmed in the adapted config),
  domain empty, and domain unset.
- `scripts/smoke-test-env.sh`: **5 of 6** checks pass — `/health` 200,
  `/v1/queue/health` 401 without a token and 200 with it (so Postgres, migrations
  and the job queue are all reachable), `/openapi.json` 200, console `/` 200.
- **Tag-pin rollback works end to end.** Changing `BRAINIAC_IMAGE_TAG` and
  re-running `up -d` re-pins *every* service and recreates the containers; rolling
  the pin back restored the previous image and the stack kept serving. The
  Postgres volume survived both swaps — a rollback does not cost data.

**Not proven here — needs a real host:**

- **TLS issuance.** Only the *configuration* is verified. No certificate can be
  obtained without a public domain and a reachable port 80, so ACME issuance,
  renewal, and the 80 → 443 redirect in practice are **UNVERIFIED**.
- **Sentry arrival.** The server carries `SENTRY_DSN` and a non-`local`
  `BRAINIAC_ENV`, and `main.rs` gates init on exactly that pair — but **no event
  has been confirmed arriving in a Sentry dashboard**, for either the server or
  the console. Treat error tracking as unproven until a real DSN on a real host
  shows an event.
- **`scripts/first-boot.sh` end to end.** Minting a `brk_` key via
  `POST /v1/tokens` was observed to work; the script's *verify* step was not, so
  the full mint → verify → rewrite → restart hand-off is **UNVERIFIED**.
- **The console image.** See the release-path caveat below — it could not be
  built during the rehearsal, so nothing about the shipped console image
  (including its baked-in Sentry DSN) is verified.

> **The pull-based happy path above has never run.** There are **no git tags and
> no images in GHCR**, so `docker compose … pull` fails with `not found` today.
> Until a `v*` tag is pushed and `.github/workflows/release.yml` publishes, the
> build-from-source fallback is the *only* working path. Additionally,
> `console/package-lock.json` is out of sync with `console/package.json`, so
> `npm ci` — the console image's very first build step, and the one the release
> workflow runs — **fails**. Both must be fixed before a first release.
