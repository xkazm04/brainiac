# Brainiac

**GitOps for organizational AI knowledge.** Capture knowhow from real LLM
sessions → provenance → review/promotion pipeline → versioned,
permission-aware distribution to agents — and, on top of that substrate, a
knowledge base whose pages are *compiled from* the canonical memories rather
than written beside them.

Rust core (axum, single binary) · Postgres 16+ as the only mandatory stateful
dependency (pgvector + FTS + job queue) · BYOM gateway (Qwen/DashScope first) ·
golden-fixture eval harness ("Meridian") as the CI quality gate.

> ### → Start here: [**docs/QUICKSTART.md**](docs/QUICKSTART.md)
>
> From nothing to *"my agent has memory"* — running it locally or deployed,
> connecting your first repo (operator → key → skill → approval), and a verify
> step that proves **retrieval actually works** rather than that a page renders.

**Shipped:** extraction → review gate → canonical memories; permission-aware
hybrid retrieval enforced by Postgres RLS *inside* the vector scan; per-fact
provenance; contradiction adjudication; temporal validity and "as of"; an MCP
surface for agents; the Knowledge Health composite (`/health`); and the memory
`lifecycle` facet (`shipped | in_flight | proposed`) the document layer composes
against.

- Architecture contract: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Eval & fixture design: [`docs/EVAL.md`](docs/EVAL.md)
- Build plan / status: [`docs/PLAN.md`](docs/PLAN.md)
- Knowledge base (v0.5, in progress): [`docs/KNOWLEDGE-BASE.md`](docs/KNOWLEDGE-BASE.md) · plan: [`docs/KB-PLAN.md`](docs/KB-PLAN.md)

## Knowledge base (v0.5, in progress)

A wiki rots because the page is where the knowledge lives. Brainiac inverts that:
**a page is a projection over canonical memories, not a second source of truth.**
Sections are bound to memory queries, cite their sources inline (`[m:uuid]`), and
are regenerated the moment a memory they depend on is superseded — so a
contradiction resolved in the review queue propagates to every page that cited the
losing claim. Truth flows one way: a human edit to a composed section re-enters
through extraction and faces the same review gate as any agent proposal. There is
no bidirectional sync, and agents never write pages — they write memories, and
pages follow.

Honest status: the substrate (memory `lifecycle` + `detail_md`, Knowledge Health)
is **shipped**; the document layer itself (tables, compose worker, dirty-marking,
citations, auto-publish policy) is **in progress**; one-way Confluence publishing,
KB token scopes and the health circuit breaker are **roadmap**.

Read: **[docs/KNOWLEDGE-BASE.md](docs/KNOWLEDGE-BASE.md)** (what a composed page
is, the projection principle, how a team turns it on, what it will never do) and
**[docs/KB-PLAN.md](docs/KB-PLAN.md)** (the phase ladder and status log). The
public page is `/kb` in the console (`cd console && npm run dev`).

## Quickstart (dev)

The full walkthrough — including the console, the three-party repo onboarding
and MCP — is **[docs/QUICKSTART.md](docs/QUICKSTART.md)**. The bare backend:

```bash
docker compose up -d          # Postgres 17 + pgvector on :5433
export DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac
export BRAINIAC_TOKENS='{"dev-token":{"org":"00000000-0000-0000-0000-0000000000aa","user":"00000000-0000-0000-0000-0000000000bb","teams":[]}}'

cargo test --workspace        # store/retrieval integration tests need DATABASE_URL
cargo run -p brainiac-server -- serve --with-worker --mock   # REST on :8600, no LLM key needed
cargo run -p brainiac-server -- eval --fixtures fixtures/v1 --profile retrieval
```

Then **prove the install works** — not that a port answers. `scripts/verify-retrieval.sh`
writes a marked memory, waits for the pipeline, and searches it back, which
exercises auth, RLS, the queue, the worker, the embedder and hybrid retrieval:

```bash
BRAINIAC_API_URL=http://127.0.0.1:8600 BRAINIAC_API_TOKEN=dev-token \
  ./scripts/verify-retrieval.sh
```

The operator console (Next.js, port 3100):

```bash
cd console && npm install && npm run dev
# console/.env.local:
#   BRAINIAC_API_URL=http://127.0.0.1:8600
#   BRAINIAC_API_TOKEN=dev-token
```

> **The console's passcode gate is OFF in dev by omission, not by design.**
> With `CONSOLE_PASSCODE` unset the gate is disabled when `NODE_ENV !== "production"`
> and **fails closed** when it is — so `npm run dev` works precisely because you
> have not set one, and the same omission on a deploy yields an empty console
> rather than an error. Full table and the consequences:
> [QUICKSTART § the passcode gate](docs/QUICKSTART.md#run-it-locally-dev).

Without Docker/`DATABASE_URL`, the pure crates (`core`, `fixtures`, `gateway`
mock, metrics) still build and test — Postgres-backed tests skip themselves.

## Deploy

The whole product — Postgres+pgvector, the Rust server (with the pipeline
worker in-process), and the Next.js console — runs on **one small VM**:

```bash
cp .env.deploy.example .env.deploy   # Qwen key, tokens, DB password
docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d --build
```

Sized for a 1 vCPU / 1 GB box. **[docs/deploy/alibaba.md](docs/deploy/alibaba.md)**
walks the free path end to end (Alibaba ECS free trial + Qwen Model Studio
free quota, Singapore) and lists the gotchas that cost hours.

## Layout

See `docs/PLAN.md` § Crate map. Fixtures are a synthetic org ("Meridian") —
fully invented, safe for a public repo, and double as demo seed data.

## Found a bug? Have feedback?

**[Open an issue](https://github.com/xkazm04/brainiac/issues/new)** — bugs,
confusing docs, a quickstart that didn't work, or a feature you expected to
find. Early feedback on the onboarding path is the most useful kind right now.

## License

MIT
