# Deploying Brainiac

Brainiac's whole stack — Postgres (pgvector), the Rust server (`serve --with-worker`
= REST + MCP + in-process pipeline worker), and the Next.js console — runs from a
single `docker compose` file on **any Docker host**. Nothing in the stack is tied to
a specific cloud; the only stateful dependency is Postgres 16+ with pgvector, which
runs as a container.

- **LLM provider is decoupled from hosting.** Pick any engine (OpenAI, Anthropic,
  Gemini, Bedrock, Vertex, Azure, Groq/…, or the default Qwen) via
  [`providers.md`](./providers.md). Hosting and model provider are independent
  choices.
- **One compose file, sized for a 1 vCPU / 1 GB VM.** See
  [`../../docker-compose.deploy.yml`](../../docker-compose.deploy.yml) and
  [`../../.env.deploy.example`](../../.env.deploy.example).

## The recipe (any host)

```bash
cp .env.deploy.example .env.deploy      # fill in secrets + choose BRAINIAC_PROVIDER
docker compose -f docker-compose.deploy.yml --env-file .env.deploy pull
docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d
./scripts/first-boot.sh                 # get off the break-glass admin token
```

That is the entire deploy. The console is published on port 80; the server is
internal to the compose network. Add ~2 GB of swap on a 1 GB box — the runtime
resident set is ~90 MiB.

### Images: pull, don't compile

The deploy file references **released images on GHCR** —
`ghcr.io/xkazm04/brainiac` (the Rust workspace) and
`ghcr.io/xkazm04/brainiac-console` — published by
[`.github/workflows/release.yml`](../../.github/workflows/release.yml) on every
`v*` tag. Nothing is compiled on the host; a 1 vCPU box would otherwise spend
~10 minutes on Rust release LTO per deploy.

`BRAINIAC_IMAGE_TAG` in `.env.deploy` selects the version — `latest` (default)
or a pinned `vX.Y.Z` for a reproducible host. Upgrading is
`pull` + `up -d`.

**Building from source is the fallback** — an unreleased commit, a fork, a
patched Dockerfile, or an architecture the release workflow doesn't publish.
The `build:` stanzas live in a separate override so the default path has exactly
one behaviour (pull) and building is always explicit:

```bash
docker compose -f docker-compose.deploy.yml -f docker-compose.build.yml \
  --env-file .env.deploy up -d --build
```

### First boot: off the break-glass token

The stack boots on `BRAINIAC_TOKENS`, an env map whose JSON key is a bearer
secret. Those entries carry **every** scope, bypass the scope system entirely
(`crates/brainiac-server/src/auth.rs` — `scopes = None` makes `allows()` always
true), and are un-revocable and un-audited. `scripts/first-boot.sh` performs the
hand-off once, correctly:

1. mints a scoped, revocable `brk_` key via `POST /v1/tokens` (default scopes:
   `read,write,kb:read,kb:publish,lib:read,lib:propose,lib:publish` — `admin` is
   deliberately excluded so a leaked console key cannot mint more keys; re-run
   with `SCOPES=admin` if you want the console's own key-management modules);
2. verifies the new key authenticates against the server;
3. rewrites `BRAINIAC_API_TOKEN` in `.env.deploy` (backing the old file up into
   the git-ignored `./secrets/`) and restarts the console on it;
4. prints the remaining manual step — copy `BRAINIAC_TOKENS` into a password
   manager, delete the line, `up -d server`.

It never prints a secret value (only a 12-char display prefix), passes bearers
to `curl` through a stdin config file so they never reach the process list,
fails loudly if the server container isn't running, and is safe to re-run — a
second run just re-verifies the existing `brk_` key instead of minting a twin.

For agent / MCP / CI keys that do **not** belong in the console's env:

```bash
TOKEN_NAME=mcp-agent SCOPES=read,write ./scripts/first-boot.sh --to-file
```

which writes the secret to a `0600`, git-ignored `./secrets/<name>.key` and
leaves `.env.deploy` untouched. Manual curl equivalent + scope background:
[`alibaba.md` → Auth & the break-glass token](./alibaba.md#auth--the-break-glass-token).

`CONSOLE_SESSION_SECRET` is **required** (the compose file fails closed on it,
same as `BRAINIAC_API_TOKEN` and `CONSOLE_PASSCODE`): without it
`console/src/lib/auth.ts` keys the session cookie on the *passcode itself*, so a
guessed passcode can be turned into a valid `bx_console` cookie offline, never
meeting `/login`'s rate limiter. Generate with `openssl rand -base64 32`.

## Host options

Any of these work with the exact same command — they differ only in how you get a
Docker host and a public IP:

| Target | Free / cheap tier | Notes |
|---|---|---|
| **Alibaba ECS** | 12-month free trial (Singapore) | The fully-worked $0 recipe: [`alibaba.md`](./alibaba.md). Pairs with the free Qwen Model Studio quota. |
| **AWS Lightsail / EC2** | Lightsail from ~$5/mo; EC2 `t4g.small` | Ubuntu image → install Docker → run the compose. Bedrock provider needs no extra network (same account IAM). |
| **GCP Compute Engine** | `e2-micro` free tier (US regions) | 1 GB — use swap. Vertex provider auth via a service-account key mounted as `VERTEX_SA_JSON`. |
| **Azure VM** | B1s (small paid) | Pairs naturally with the `azure` OpenAI provider. |
| **Hetzner / DigitalOcean / Vultr** | ~€4–6/mo CX/small droplets | Cheapest always-on; no provider lock-in. |
| **Fly.io** | Pay-as-you-go | Run the server + a Fly Postgres (pgvector) app; publish the console separately. |

### Generic VM steps (Ubuntu/Debian)

```bash
# 1. install Docker + compose plugin
curl -fsSL https://get.docker.com | sh
# 2. (1 GB hosts) add swap
sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile \
  && sudo mkswap /swapfile && sudo swapon /swapfile
# 3. clone, configure, launch
git clone https://github.com/xkazm04/brainiac && cd brainiac
cp .env.deploy.example .env.deploy && $EDITOR .env.deploy
docker compose -f docker-compose.deploy.yml --env-file .env.deploy pull
docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d
# 4. harden auth (mint a scoped brk_ key, retire the break-glass map)
./scripts/first-boot.sh
```

### TLS

The stack ships its own terminator — a `caddy` service (`deploy/Caddyfile`) that
owns 80/443 and is the **only** publicly bound container. The console listens on
the Docker network only, so there is no port that bypasses TLS.

| `BRAINIAC_DOMAIN` in `.env.deploy` | What you get |
| --- | --- |
| unset / empty | Plain HTTP on port 80. The only option for a bare-IP host — no public CA issues certificates for an IP address. |
| `memory.acme.com` | Caddy obtains **and renews** a Let's Encrypt certificate automatically, serves HTTPS on 443, and redirects 80 → 443. No certbot, no cron, no cert paths, no compose edit. |

So going from HTTP to HTTPS is a DNS record plus one env var. Prerequisites for
the domain mode: the A/AAAA record must **already** resolve to the host before
you start the stack, and both **80 and 443** must be open (ACME validates over
80 — opening only 443 fails issuance). Set `BRAINIAC_CONSOLE_URL` to the
`https://…` form too, so onboarding approval links match.

Open **80** and **443** in the firewall / security group; leave the server's
**8600** closed (it never leaves the Docker network). Certificates live in the
`caddy-data` volume — keep it across redeploys, or every restart re-requests a
certificate and burns Let's Encrypt rate limit.

## What's cloud-specific vs portable

- **Portable (everything):** the compose stack, the published images, the
  Dockerfiles, the migrations, the console, and every LLM provider — all driven by
  env vars.
- **Cloud-specific (only the wrapper):** how you provision the VM, the public IP /
  DNS, and (optionally) which LLM provider you pick to sit closest to your cloud —
  e.g. Bedrock on AWS, Vertex/Gemini on GCP, Azure OpenAI on Azure, Qwen on Alibaba.
  None of these change the deploy command.

## Backups

The only durable state is the Postgres volume. `scripts/backup.sh` /
`scripts/restore.sh` (referenced in [`alibaba.md`](./alibaba.md)) are host-agnostic —
they `pg_dump` from the container and are safe to cron on any target.
