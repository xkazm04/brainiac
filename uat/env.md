# Environment — how to reach a known start state

## Stack up (L2)

```bash
docker compose up -d                                   # Postgres 17 + pgvector on :5433
export DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac_uat_<run-id>
cargo run -p brainiac-server -- serve                  # REST on :8600
cargo run -p brainiac-server -- worker                 # ← NOT OPTIONAL. See below.
curl localhost:8600/health                             # preflight
```

**The worker is not optional.** It *is* the pipeline: ingest → extract → embed → resolve →
contradict → promote. Without it, sessions are ingested and nothing is ever extracted, so the
store never grows, no relay ever completes, and the run measures an empty corpus while
reporting green. Verify it drained before trusting anything: `GET /v1/queue/health`.

**One database per run.** Two concurrent UAT runs against the same Postgres will cross-pollute
each other's memories — including each other's *decoys* — and silently poison both. Use a
run-scoped db name.

## Identities

One principal per Character (`company.md` roster). Two options:

- **Bootstrap** — `BRAINIAC_TOKENS` env map → `{token: {org, user, teams, role}}` (`auth.rs:34-51`).
- **Minted** — `POST /v1/tokens` → scoped `brk_…` tokens.

**Prefer minted tokens.** Env tokens carry **every scope, unrestricted** (`auth.rs:1-6`) — they
are break-glass operator credentials. Running the contractor or the leak probes on an env token
will hide exactly the H4 failure you are hunting, and the run will report a clean permission
model that does not exist.

## Seed

`driver/seed.sh` loads `fixtures/v1` (80 gold memories, 42 entities, 12 merge sets, 12
contradictions, 6 supersession chains, 15 leak cases, 9 transcripts), applies the `company.md`
extension (team-web, the 6 new principals), and plants `decoys.md`. Validate referential
integrity with the `brainiac-fixtures` loader before trusting a run.

**Plant decoys in the seed, drain, review — then run sessions.** A decoy injected mid-run is a
different experiment.

## Open questions to resolve before the first live run

1. **Which BYOM provider?** Extraction and contradiction quality are provider-dependent
   (ARCHITECTURE §9, risk 1). **A run on `MockProvider` measures plumbing, not knowledge.** If
   that's what we've got, the report says so in the headline — it does not quietly report the
   numbers as quality.
2. **Which embedder?** `--embedder qwen` (real; NDCG@10 0.876) vs the deterministic hashed
   default (NDCG@10 0.685). PLAN.md deviation 4 is blunt: deterministic numbers are *plumbing
   numbers, not quality claims*. Label every report with the embedder it ran on.
3. **Do Sam (staff) and Dana (EM) get a team?** They have none, and RLS + the extractor's
   `team`-by-default mean they will see almost nothing. There is no right answer — **the absence
   of one is the finding** (`company.md` § structural fact 1). Pick org-only visibility, record
   the choice, and let their journeys report what a cross-team principal actually experiences.
4. **`fixtures/v2` (stack-specific corpus): build it, or scope it out?** Without it H3 and every
   cross-stack journey are `not probed`. The core verdict does not depend on it.

## Reset between Characters

Memories persist by design — that is the flywheel. What must NOT persist across a *comparison*
is the repo working tree: each arm starts from the same commit, in its own checkout. Arms must
never see each other's diffs.
