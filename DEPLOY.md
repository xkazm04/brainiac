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

# 4. Verify
docker compose -f docker-compose.deploy.yml ps          # all three healthy
curl -sf http://localhost/ >/dev/null && echo "console up"   # pitch page, token-free

# 5. Get off the break-glass admin token (see "Harden auth" below)
./scripts/first-boot.sh
```

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

The console answers on **port 80**; the server's 8600 stays on the Docker network
(never public). Open **80** and **22** in the security group; leave **8600**
closed. Migrations run automatically at server boot.

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
| `BRAINIAC_CONSOLE_URL` | Public console URL (`http://<public-ip>`); onboarding approval links are built from it. |
| `RUST_LOG` | Log level (default `info`). |

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
