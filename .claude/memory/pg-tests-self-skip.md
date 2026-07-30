# A green `cargo test --workspace` without DATABASE_URL proves nothing about the data plane

Every Postgres-backed test is gated on the `DATABASE_URL` env var and, when it
is absent, prints `SKIP: DATABASE_URL not set — …` and **returns Ok**. The test
passes. Nothing fails, nothing is red, and the entire store / retrieval /
eval / MCP data plane went untested. Examples:
`crates/brainiac-eval/tests/{contradiction,divergence,grid,pipeline}_pg.rs`,
`crates/brainiac-store/tests/*_pg.rs`, `crates/brainiac-server/tests/*_pg.rs`
(the `_pg` suffix is the convention).

So before claiming a change is verified:

```bash
docker compose up -d
export DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac
cargo test --workspace
```

**Port gotcha:** local `docker-compose.yml` publishes pgvector on **5433**
(container 5432), while `.github/workflows/ci.yml` runs the service on **5432**
and sets `DATABASE_URL=…@localhost:5432/brainiac`. Copying the CI URL into a
local shell silently yields "no server" or, worse, connects to some other local
Postgres. Use 5433 locally.
