# fixtures/bank — the scale corpus

Meridian grown into a licensed bank: 12 teams, 1,200+ memories, ~260 canonical
entities. The reasoning behind the shape — why the volume is a power law, why
contradictions cluster on team boundaries, why visibility is a pyramid — is in
`docs/BANK-CORPUS.md`. Read that first; this directory is its output.

**Generated, not authored.** Every file here comes from `tools/gen-bank-corpus.mjs`
and is deterministic: the same seed produces byte-identical YAML, so a diff means
someone changed the generator, not that the generator is noisy. Do not hand-edit
these files — regenerate them:

```bash
node tools/gen-bank-corpus.mjs          # rewrites fixtures/bank/
cargo run -p brainiac-server -- fixtures lint --fixtures fixtures/bank
```

**What this tree is for, and what it is not.** It exists to put the console's
modules under organizational load — a thousand-row archive, a graph with hundreds
of nodes, a review queue with a real backlog. It is *not* a retrieval-quality
benchmark: `fixtures/v1` is the hand-authored gold corpus, where every qa case
and leak target was chosen by a human to mean something. The gold slices here are
generated, so a score against them measures the generator's imagination, not the
retriever. Keep eval baselines on v1.

## Seeding it

```bash
# DESTRUCTIVE — re-seeds the tenant. Point it at a dev database.
cargo run -p brainiac-server -- eval --fixtures fixtures/bank --profile retrieval
```

That path is used because it is the supported one: it inserts orgs, teams, users,
entities, canonical merges, memories AND their embeddings through the same store
the product uses. The knowledge base is seeded separately
(`tools/seed-bank-docs.mjs`) — see `docs/BANK-CORPUS.md` §4 for why pages are
written rather than composed.
