# Arm B — the baseline, written first and written generously

> **This file is the evidence that the trial was fair. It is committed, versioned, and
> written BEFORE any journey — before we know what Brainiac will retrieve — so it cannot be
> tuned to lose. A rigged baseline makes every number in this repo worthless.**

Arm B is **not "a CLAUDE.md."** It is *what a competent senior with a weekend and current
best practice would actually build*, in July 2026. Anything less is a strawman, and the
research is explicit that the obvious ways to weaken it are the wrong ones.

## What arm B gets (the whole free stack)

1. **The four concatenated `CLAUDE.md` scopes** — managed policy, user (`~/.claude/CLAUDE.md`),
   project (`./CLAUDE.md`, committed), local (`./CLAUDE.local.md`, gitignored). They stack;
   they do not override.
2. **`.claude/rules/` with `paths:` glob frontmatter.** Path-scoped rules that load only when
   matching files are touched. **This is free, just-in-time, path-conditional rule retrieval —
   the closest free analogue to Brainiac's retrieval, and omitting it is the single easiest
   way to fake a win.** Every arm-B repo gets it.
3. **Subdirectory `CLAUDE.md`** in the one or two deep modules that warrant it. Loaded on
   demand when the agent reads files there — free progressive disclosure.
4. **Auto-memory left ON** (`~/.claude/projects/<repo>/memory/`). Arm B has a learning loop.
   Its limit is structural, not a defect we introduced: **machine-local, per-repo, never
   shared between developers.** That limit is the hypothesis, not the handicap.
5. **The symlinked shared org-rules file** — `ln -s ~/meridian-standards/backend.md .claude/rules/org.md`.
   The free cross-repo mechanism a good senior finds. **Include it, then beat it.**
6. **Hooks** for anything that must be *enforced*. `CLAUDE.md` is context, not configuration —
   Anthropic says so outright, and a senior knows it.

## Maintenance budget (non-negotiable)

Between sprint phases, arm B's owner **may edit any of the above**, exactly as a real team
does. A frozen arm B against a live arm C is a rigged fight.

They will probably not think to update it. **That is arm B's rot, and we measure it — we do
not assume it.** Record every phase: did the owner touch the file? What did they add? What
went stale that they didn't notice? "Nobody updated it" is a *result*, and it is the result
that most of the knowledge-management literature predicts.

## What arm B structurally CANNOT do — the pre-registered gaps

Verified from primary docs. These, and only these, are where Brainiac is allowed to win:

- **Share a learning from Ada's session with Ingrid.** Auto-memory is machine-local and
  per-repo. Every developer re-learns the same thing from scratch.
- **Cross a repo boundary** without per-machine symlink setup — and with zero governance.
- **Retract.** No provenance, no review, no expiry. A stale line in `CLAUDE.md` sits there
  being confidently wrong forever, and nothing goes red.
- **Retrieve just-in-time against the within-session decay curve** (compliance drops ~5.6%
  per generated function; the file is followed at the start and the end and ignored in the
  middle, where the work happens). A front-loaded blob has no answer to this. Brainiac's
  mid-session `memory_search` does — *in principle*. Test it.

---

## The files

### `payment-service/CLAUDE.md` (Ada, Petra, Mira, Nadia, Rafael)

Note what this deliberately contains: **the retry-storm gotcha.** A competent senior who
lived through `src-pay-007` would absolutely have written that line. Brainiac does not get
to win by pretending they wouldn't. If it cannot beat a hand-written "gotchas" list, it does
not have a product.

```markdown
# payment-service

Card payments + refunds. Source of truth for payment state. Rust 1.84 (axum, tokio,
sqlx) + Postgres 16. Deployed to k8s via ArgoCD. Owns: payment-service, refund-worker,
ledger-service.

## Commands
- Test:        `cargo test --workspace`      (unit; ~50s)
- Integration: `cargo test -- --ignored`     (needs `docker compose up -d`)
- Lint:        `cargo clippy -- -D warnings` (CI gate)
- Run locally: `cargo run -p payment-service` → :8080, health at /healthz
Always run `cargo clippy -- -D warnings && cargo test` before proposing a commit.

## Layout
- `crates/payment-service/`  HTTP + orchestration. Thin handlers.
- `crates/ledger/`           double-entry core. The interesting code lives here.
- `crates/refund-worker/`    async refund processing off the queue.
- `crates/psp-adapter/`      PSP integration. Normalizes decline codes.
- `migrations/`              sqlx, append-only. Never edit a shipped file.

## Conventions a linter can't catch
- Money is `ledger::Amount` (i64 **minor units**). Never f64. Never i32.
- All balance mutations go through `ledger::post()`. Never write `balances` directly,
  not even in tests — use the builders in `ledger::testing`.
- Refunds go through refund-worker. **Direct DB refund writes are forbidden.**
- Handlers return `ApiError`, never `anyhow::Error`. Map at the boundary.
- Every repo method takes `&self, ctx: &Ctx` first and honors cancellation.

## Boundaries
- Never edit `gen/` (generated from proto) or shipped `migrations/`.
- Do not touch `crates/checkout-v1/` — frozen, deleted next quarter.
- No new dependency without asking.

## Testing
- New domain logic needs a unit test. New endpoint needs an integration test on real PG.
- Do not mock the store in domain tests — use the in-memory impl.

## Gotchas that have bitten us
- **The refund-worker retry cap.** The old 2s cap with 3 attempts caused timeout storms
  against psp-gateway when the PSP's latency spikes (settlement batches ~14:00 UTC). We
  raised it to **30s with jitter**. Do not "helpfully" lower it back to match the org
  std-retry default.
- **PSP decline code 05 (do-not-honor) is issuer-side. Do not retry it** — it burns quota
  and never succeeds.
- Argo rollback of payment-service must pause refund-worker first, or refunds double-apply.
- Browser autofill can double-fire tokenization on the v2 card form. Debounce it.
- `cargo test` passing does NOT mean migrations apply cleanly. Run them against a fresh DB.

## Detail docs
- Decline taxonomy: docs/declines.md · Idempotency: docs/idempotency.md
```

`.claude/rules/migrations.md` (`paths: ["migrations/**"]`), `.claude/rules/psp.md`
(`paths: ["crates/psp-adapter/**"]`), and `crates/ledger/CLAUDE.md` (the double-entry
invariants) all exist. Arm B is well-tended.

### `infra-live/CLAUDE.md` (Tomas, Lars, Yusuf)

```markdown
# infra-live

Meridian's infra as code. Terraform + Helm + Rego. Gitops: what's in master is what runs.

## Commands
- Plan:   `make plan ENV=prod`
- Policy: `make conftest`   (OPA policy unit tests — CI gate)
- Deploy: **you don't.** ArgoCD syncs from master. There is no manual deploy path.

## Layout
- `policies/`  Rego. Every prod deploy is gated by these.
- `clusters/`  per-cluster Helm values.
- `topics/`    Kafka topic manifests. PR here to grant topic access.

## Conventions
- ArgoCD is the ONLY supported prod deploy path (since March 2026). Jenkins is gone.
- Every service must emit OpenTelemetry traces.
- Secrets come from Vault. Static k8s secrets are forbidden.
- Every repo deploys from master. Release branches are forbidden.
- Grafana dashboards are gitops-managed here now — not clicked into the UI.

## The std-retry policy
Org-wide default: **cap 2s, 3 attempts**, applied to all internal calls and every Kafka
consumer. Defined in `policies/retry.rego`. If you need different behavior, talk to
platform — do not fork it.

## Gotchas
- otel-collector-config drops spans over a 5k batch queue. Raise the queue before blaming
  the tracer.
- Kafka Streams jobs must not share consumer groups with raw consumers.
- MSK broker storage autoscaling is NOT enabled. Disk expansion is a manual infra-live PR.
- Payment topic partitions are fixed at 24. Changing them needs data-team sign-off.

## Deploy freeze exceptions
Override PR + linked incident. OPA requires 2 maintainer approvals.
```

> ⚠ **Note the trap, and note that it is FAIR.** This file says std-retry is *2s, 3 attempts*.
> As of the sprint that is **stale** — `mem-plat-0107` was superseded by `mem-pay-0043`
> (30s + jitter). Nobody updated `infra-live/CLAUDE.md`, because nobody ever does. This is
> not a handicap we invented; it is **exactly the failure mode the literature documents**,
> reproduced faithfully. It is arm B's honest weakness and Brainiac's honest opportunity —
> `H-retract` lives here. Arm B's owner *may* fix it between phases. Watch whether he does.

### `event-lake/CLAUDE.md` (Ingrid)

```markdown
# event-lake

Meridian's event warehouse. Python 3.12 + Airflow + dbt. Feeds feature-store and fraud-model.

## Commands
- Test:  `pytest`            · Lint: `ruff check .` (CI gate)
- dbt:   `dbt build --select state:modified+`
- DAGs:  `airflow dags test <dag_id>`

## Layout
- `dags/`         Airflow DAGs. The backfill replay job is a DAG.
- `models/`       dbt. **The only supported transform layer** — ad-hoc SQL is frozen.
- `schemas/`      pinned event schemas from the registry.

## Conventions
- Checkout funnel reads `checkout.events.v2`, ingested hourly. The hourly cadence IS the
  contract — dashboards lag by up to an hour and that is expected.
- Retention: raw 400 days, aggregates indefinite.
- Feature-store snapshots version daily, retained 90 days.

## Gotchas that have bitten us
- **Amounts are integer minor units, by contract.** We once had a dbt model re-divide
  already-normalized amounts and inflated every fraud feature 100x. Run the amounts sanity
  suite against a day of ledger totals after any dbt change touching money.
- The backfill DAG must not run concurrently with hourly ingest — partition deadlock.
- Schema registry enforces backward compat on payment topics. Respect the pinning.
```

### `checkout-web/CLAUDE.md` (Jonas) — **NEW, not in fixture**

```markdown
# checkout-web

Meridian's card checkout UI. TypeScript, Next.js 15, React 19. Talks to payment-service.

## Commands
- Dev: `npm run dev` · Test: `npm test` (vitest) · Lint: `npm run lint` (CI gate)
- Types: `npm run typecheck` — must be clean before commit.

## Conventions
- Never format money client-side. The API returns minor units; use `formatAmount()`.
- All payment calls go through `lib/payments.ts`. No direct fetch to the payments API.
- Card form state is owned by `useCardForm` — do not lift it.

## The payments API
- Base: `/v1/payments`. Contract lives in payment-service's `openapi.yaml`.
- Checkout v2 is the live flow. v1 endpoints are frozen.

## Gotchas
- Autofill can double-fire tokenization. `useCardForm` debounces — don't remove it.
- The decline code shown to the user is NOT the PSP's raw code. Map through `declineCopy`.
```

> ⚠ **The `after-the-file` gap lives here.** This file records the payments API contract as
> Jonas last understood it. When payments changes something mid-sprint, nothing tells
> `checkout-web`. A repo-committed file cannot cross a repo boundary. `H-cross` lives here.

### `~/meridian-standards/backend.md` — the shared symlink

The free cross-repo mechanism. Symlinked into each repo as `.claude/rules/org.md`. Contains
what Meridian's staff engineers agreed on: ArgoCD-only deploys, OTel everywhere, Vault for
secrets, minor-units for money, master-only branches. **It is real, it works, and it is arm
B's answer to "org-wide knowledge."**

Its limits — and these are the honest ones: it is set up per-machine (Mira, in week 1, does
not have it), it has **no governance, no provenance and no expiry**, and it says nothing
about *why*. When std-retry changed, nobody edited it either.
