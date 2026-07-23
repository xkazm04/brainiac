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

# 3. Build + start (Rust image ~10 min on a 1-core box)
docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d --build

# 4. Verify
docker compose -f docker-compose.deploy.yml ps          # all three healthy
curl -sf http://localhost/ >/dev/null && echo "console up"   # pitch page, token-free
```

The console answers on **port 80**; the server's 8600 stays on the Docker network
(never public). Open **80** and **22** in the security group; leave **8600**
closed. Migrations run automatically at server boot.

## Environment variables (`.env.deploy`)

| Var | What it is |
| --- | --- |
| `POSTGRES_PASSWORD` | Postgres password (container-local). **Required.** |
| `QWEN_API_KEY` | DashScope / Model Studio key (Singapore). **Required.** |
| `QWEN_BASE_URL` | DashScope endpoint (default is the Singapore compatible-mode URL). |
| `QWEN_MODEL` | Extraction model — `qwen-plus` (default), `qwen-max`, or `qwen-flash`. |
| `BRAINIAC_EMBEDDER` | `qwen` (default, best) or `deterministic` (no API, ~0.19 NDCG lower). |
| `BRAINIAC_TOKENS` | Break-glass admin map (bearer secret → principal). **Unrestricted — bootstrap only.** |
| `BRAINIAC_API_TOKEN` | Token the console uses. **Required.** Boot with the break-glass secret, then switch to a minted `brk_` key. |
| `CONSOLE_PASSCODE` | Shared login passcode for the console. **Required** — without it the console fails closed to `/` + `/demo`. |
| `CONSOLE_SESSION_SECRET` | Signs the session cookie independently of the passcode. Recommended in prod. |
| `BRAINIAC_CONSOLE_URL` | Public console URL (`http://<public-ip>`); onboarding approval links are built from it. |
| `RUST_LOG` | Log level (default `info`). |

**Harden auth before real use:** boot on the break-glass `BRAINIAC_TOKENS` admin,
mint a scoped `brk_` key via `POST /v1/tokens`, point `BRAINIAC_API_TOKEN` at it,
and keep the break-glass secret offline. Exact curl + scope guidance:
[`docs/deploy/alibaba.md` → Auth & the break-glass token](docs/deploy/alibaba.md#auth--the-break-glass-token).

**Back up daily:** `./scripts/backup.sh` (→ `./backups/`); restore with
`./scripts/restore.sh <dump>`. See
[`docs/deploy/alibaba.md` → Backup & restore](docs/deploy/alibaba.md#backup--restore).

## What can go wrong

1. **`up` aborts with `set POSTGRES_PASSWORD` (or `…API_TOKEN` / `…PASSCODE`).**
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
