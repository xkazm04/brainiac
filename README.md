# Brainiac

**GitOps for organizational AI knowledge.** Capture knowhow from real LLM
sessions → provenance → review/promotion pipeline → versioned,
permission-aware distribution to agents.

Rust core (axum, single binary) · Postgres 16+ as the only mandatory stateful
dependency (pgvector + FTS + job queue) · BYOM gateway (Qwen/DashScope first) ·
golden-fixture eval harness ("Meridian") as the CI quality gate.

- Architecture contract: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Eval & fixture design: [`docs/EVAL.md`](docs/EVAL.md)
- Build plan / status: [`docs/PLAN.md`](docs/PLAN.md)

## Quickstart (dev)

```bash
docker compose up -d          # Postgres 17 + pgvector on :5433
export DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac
cargo test --workspace        # store/retrieval integration tests need DATABASE_URL
cargo run -p brainiac-server -- serve     # REST on :8600
cargo run -p brainiac-server -- worker    # pipeline workers
cargo run -p brainiac-server -- eval --fixtures fixtures/v1 --profile retrieval
```

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

## License

MIT
