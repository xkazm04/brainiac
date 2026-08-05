# Quickstart — from nothing to "my agent has memory"

This is the shortest honest path from a fresh clone (or a fresh deploy) to an
agent that can **retrieve** what your org already learned. It is the one flow
the product exists for, and it involves **three parties in a fixed order**:

| # | Who | Where | What |
| - | --- | ----- | ---- |
| 1 | **operator** | console → **projects** | create a project, register the repo's git remote |
| 2 | **developer** | their checkout | install + run the `brainiac-onboard` skill |
| 3 | **operator** | console → **projects** | approve the pairing request |
| 4 | anyone | anywhere | write a memory, retrieve it back |

The order matters and is not negotiable: **approval derives the project from the
repo allow-list**, so a pairing request whose remote matches no registered repo
cannot be approved — only denied. Step 1 first, always.

If you already have a running deployment, skip to
[Connect your first repo](#connect-your-first-repo).

---

## Run it locally (dev)

```bash
docker compose up -d          # Postgres 17 + pgvector on :5433
export DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac

# The REST API on :8600, with the pipeline worker in-process.
# --mock uses the deterministic provider so you need no LLM key at all;
# drop it (and set QWEN_API_KEY) when you want real extraction.
cargo run -p brainiac-server -- serve --with-worker --mock
```

Give yourself a bearer token by setting `BRAINIAC_TOKENS` before that command —
the JSON key is the secret, the value is the principal it acts as:

```bash
export BRAINIAC_TOKENS='{"dev-token":{"org":"00000000-0000-0000-0000-0000000000aa","user":"00000000-0000-0000-0000-0000000000bb","teams":[]}}'
```

Then prove the whole pipeline actually works — not just that a port answers:

```bash
BRAINIAC_API_URL=http://127.0.0.1:8600 BRAINIAC_API_TOKEN=dev-token \
  ./scripts/verify-retrieval.sh
```

It writes a marked memory, waits for the pipeline, and searches it back. See
[Prove it works](#prove-it-works) for what each step covers and what it leaves
behind.

### The console

```bash
cd console
npm install
npm run dev            # http://localhost:3100
```

Point it at the server with a `console/.env.local`:

```bash
BRAINIAC_API_URL=http://127.0.0.1:8600
BRAINIAC_API_TOKEN=dev-token
# Optional: the URL a DEVELOPER on another machine should use. The console's
# own BRAINIAC_API_URL is often a private/docker address; this is what gets
# rendered into the copyable onboarding command.
# BRAINIAC_PUBLIC_API_URL=http://192.168.1.10:8600
```

> ### ⚠ The console's passcode gate is OFF in development — by omission
>
> The console is protected by a shared-passcode gate (`CONSOLE_PASSCODE` →
> an HMAC-signed session cookie). With **no** `CONSOLE_PASSCODE` set, the gate
> behaves differently depending on `NODE_ENV`
> ([`console/src/lib/auth.ts`](../console/src/lib/auth.ts) `isUnlockedByDefault`):
>
> | `CONSOLE_PASSCODE` | `NODE_ENV` | Result |
> | --- | --- | --- |
> | unset | not `production` (`npm run dev`) | **open — no login at all** |
> | unset | `production` | **fails closed** — only `/`, `/login`, `/signup`, `/demo`, `/pitch`, `/kb`, `/library` are reachable; every console surface 302s to `/login`, which explains what to set |
> | set | either | passcode gate active |
>
> So `cd console && npm run dev` "just works" **because** you have not set a
> passcode, not because dev mode is special-cased to be convenient. Two
> consequences worth internalising:
>
> 1. **A dev server on a shared network serves the live org to anyone who can
>    reach the port**, if `BRAINIAC_API_TOKEN` points at a real deployment. Set
>    `CONSOLE_PASSCODE` in `console/.env.local` whenever the token is not a
>    throwaway.
> 2. **The same omission behaves oppositely in production.** If you forget the
>    variable on a deploy, you do not get an open console — you get an empty
>    one, and the symptom is "the console shows demo data / `/api/*` returns
>    401", not an error about a missing passcode.
>
> Also set `CONSOLE_SESSION_SECRET` in production: without it the session
> cookie is keyed on the passcode itself, so a guessed passcode can be turned
> into a valid cookie offline, skipping `/login`'s rate limiter.

## Or deploy it

One small VM runs the whole stack (Postgres + server + console + a Caddy TLS
terminator). Follow **[`DEPLOY.md`](../DEPLOY.md)** — 60-second happy path,
every environment variable, and the free-tier recipe.

---

## Connect your first repo

On a fresh database the console's front door (`/console`) shows this flow
directly instead of empty dashboards. It is the same three steps.

### 1. Operator — register the repo (console → **projects**)

Create a project, then add the repo's `git remote get-url origin` value to its
allow-list. **Do this before the developer runs anything**: an unmatched
pairing request can only be denied.

### 2. Developer — install and run the skill

In the repo's checkout:

```bash
mkdir -p .claude/skills/brainiac-onboard && \
  curl -fsSL "$BRAINIAC_API_URL/v1/onboard/skill" -o .claude/skills/brainiac-onboard/SKILL.md
```

Then run `/brainiac-onboard` in Claude Code. There is **no secret in that
command** — it is a plain GET of a skill file. The skill:

- starts a device-authorization pairing against `POST /v1/onboard/start`,
- shows a short user code and the console URL to approve it at,
- polls until decided, and on approval writes a **project-scoped** API key
  straight into `.env` (never printed, never echoed, never committed),
- verifies the round trip with a real authenticated search,
- enriches `CLAUDE.md` / `AGENTS.md` so future sessions use org memory,
- optionally registers the MCP server and syncs the org's skill library.

It is safe to re-run.

### 3. Operator — approve the pairing (console → **projects**)

The right-hand rail of the projects module is the doorbell; it polls, so the
request appears while you are watching. Compare the user code the developer
read out, then approve. Approval mints a read/write key scoped to the matched
project — **never `admin`**, so a leaked laptop key cannot mint more keys. The
code expires in 15 minutes.

### 4. First retrieval

Now the agent has memory. Writing and reading it, over REST:

```bash
API="$(grep '^BRAINIAC_API_URL=' .env | tail -1 | cut -d= -f2-)"
TOKEN="$(grep '^BRAINIAC_API_TOKEN=' .env | tail -1 | cut -d= -f2-)"

# write (async — it enqueues extraction and returns a receipt)
curl -fsS -X POST "$API/v1/memories" \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"content":"Checkout retries are idempotent on the PSP reference, not the order id.","kind":"fact"}'

# read it back a few seconds later
curl -fsS -X POST "$API/v1/memories/search" \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"query":"checkout retry idempotency","k":5}'
```

Over MCP, agents get the same surface as tools — the onboarding skill registers
it for you (step 2).

Once one memory is canonical, `/console` stops showing the on-ramp and becomes
the Observatory.

---

## Prove it works

`curl http://localhost/` proves the console's **pitch page renders**. That page
never touches the API, so it stays green through a dead database, a wrong
`BRAINIAC_API_TOKEN`, an embedder with no credit, and a worker that never
started — every one of which breaks the product completely.

```bash
./scripts/verify-retrieval.sh                                  # docker deploy stack
BRAINIAC_API_URL=... BRAINIAC_API_TOKEN=... ./scripts/verify-retrieval.sh   # direct
```

Three steps, covering auth, scopes, RLS, the Postgres write path, the job
queue, the worker, the embedder, and hybrid retrieval:

1. `POST /v1/memories` — write a uniquely-marked statement
2. `GET /v1/sources/{id}` — poll until the pipeline reports `processed`
3. `POST /v1/memories/search` — retrieve it back **by its marker**

**It leaves one memory behind**, in the org the token belongs to, whose own
text says what it is and carries the marker. That is deliberate: a check that
writes nothing cannot prove the write path. Search the archive module for the
marker and deprecate it if you would rather it were not there.

> ⚠ **Use a principal that belongs to a team.** A memory defaults to
> `visibility = 'team'` and inherits the writer's **first** team. A teamless
> principal — including the `"teams": []` shape both `BRAINIAC_TOKENS` examples
> ship with — writes memories with `team_id = NULL`, and the RLS read policy
> matches team-visible rows by `team_id IN (…)`. `NULL` matches nothing, for
> anyone, permanently: the write returns 202, the pipeline reports `processed`,
> and the memory is unreadable by every principal in the org, including the one
> that wrote it. Give the token a real team uuid **and** make that user a row in
> `team_members` for it — RLS reads the table, not the token.
> `verify-retrieval.sh` detects this case specifically and explains it rather
> than blaming the embedder.

For liveness/plumbing checks that write nothing (health, auth-is-on, OpenAPI,
console gate), use [`scripts/smoke-test-env.sh`](../scripts/smoke-test-env.sh)
instead — the two are complementary.

---

## Where to go next

| | |
| --- | --- |
| What the system is and guarantees | [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) |
| Deploying for real | [`DEPLOY.md`](../DEPLOY.md) · [`docs/deploy/`](deploy/) |
| Pages compiled from memories | [`docs/KNOWLEDGE-BASE.md`](KNOWLEDGE-BASE.md) |
| Eval harness / quality gate | [`docs/EVAL.md`](EVAL.md) |
| Build plan & status | [`docs/PLAN.md`](PLAN.md) |

Something wrong, confusing, or missing?
**[Open an issue](https://github.com/xkazm04/brainiac/issues/new)** — including
"the quickstart didn't work". That is the most useful bug report this project
can get right now.
