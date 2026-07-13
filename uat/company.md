# Meridian — the simulated company

The trial runs inside one org. It is **`fixtures/v1/` extended, never replaced** — every
id below that already exists in `fixtures/v1/org.yaml` keeps its exact string, so the eval
harness still loads. New ids are additive.

Meridian is a fintech: card checkout, payments, ledger, fraud scoring. ~40 engineers.
They adopted coding agents 8 months ago. Every team already has a `CLAUDE.md`. Nobody
has ever deleted a line from one.

## Teams, stacks, repos

The fixture ships three teams and no stacks. The trial assigns stacks (below) and adds a
fourth team, because a cross-tech-stack question cannot be asked of a one-stack company.

| team id | name | stack | repos | in fixture? |
|---|---|---|---|---|
| `team-payments` | payments | **Rust** — axum, tokio, sqlx | `payment-service`, `refund-worker`, `ledger-service` | yes |
| `team-platform` | platform | **Go + Terraform + Rego** — k8s, ArgoCD, OPA, Vault | `infra-live`, `deploy-tools` | yes |
| `team-data` | data | **Python + SQL** — Airflow, dbt, feast | `event-lake`, `dbt-models`, `fraud-model` | yes |
| `team-web` | web | **TypeScript** — Next.js, React | `checkout-web` | **NEW — see fixture gap** |

The stack assignment is **consistent with** the fixture's content (payments talks about
retries and PSPs; platform runs ArgoCD/OPA/MSK; data runs Airflow/dbt/feast) — it makes
explicit what the corpus already implies. It does not contradict a single existing memory.

## People (12 Characters + 2 non-player maintainers)

| # | Character | principal id | team | role | new? |
|---|---|---|---|---|---|
| 1 | Ada Kovac — senior backend | `user-pay-dev1` | payments | member | fixture |
| 2 | Petra Novak — payments maintainer | `user-pay-lead` | payments | **maintainer** | fixture |
| 3 | Tomas Reid — platform / SRE | `user-plat-dev1` | platform | member | fixture |
| 4 | Ingrid Sol — data engineer | `user-data-analyst1` | data | member | fixture |
| 5 | Jonas Weber — frontend | `user-web-dev1` | **web** | member | NEW |
| 6 | Mira Haddad — new joiner (week 1) | `user-pay-new` | payments | member | NEW |
| 7 | Sam Oduya — staff engineer | `user-staff` | **∅ / cross-team** | member | NEW |
| 8 | Rafael Ortiz — contractor | `user-contractor` | payments (scoped) | member | NEW |
| 9 | Dana Brecht — engineering manager | `user-em` | **∅ / cross-team** | member | NEW |
| 10 | Yusuf Kaya — security engineer | `user-plat-sec` | platform | member | NEW |
| 11 | Lars Bengtsson — skeptic senior | `user-plat-lead` | platform | **maintainer** | fixture |
| 12 | Nadia Roth — on-call engineer | `user-pay-oncall` | payments | member | NEW |
| — | data maintainer (NPC) | `user-data-lead` | data | maintainer | fixture |

Lars is deliberately **both the platform maintainer and the skeptic**: the person who pays
the governance tax is the person most likely to doubt it's worth paying. That is the real
adoption dynamic, not a contrivance.

## Two structural facts the model forces on us — both are findings, not setup notes

**1. A cross-team principal cannot exist.** In `fixtures/v1/org.yaml` every user belongs to
exactly one team, and RLS grants `team`-visible reads only to members of the owning team
(`migrations/0001_init.sql:252-262`). So **Sam (staff engineer) and Dana (EM) — the two
people in a real org whose whole job is to see across teams — have no principal that can.**
Either they get one team (and are blind to the rest) or they get org-only visibility (and
see almost nothing, since most memories default to `team`). *The extractor defaults new
memories to `visibility: team`* (`extract.rs:381-385`), so the corpus skews team-private by
construction. Log this at L1 as `missing-feature / major` before a single session runs:
**Brainiac's loudest claim is cross-team knowledge flow, and its permission model has no
seat for the people who work across teams.**

**2. `maintainer` is not a superuser, and that's correct.** Four fixture leak cases
(`leak-003`, `-011`, `-012`, `-014`) assert a maintainer must **not** read a member's
`private` memory. Any Character design that assumes "the lead can see everything" is wrong
and will manufacture a false leak finding. Respect it.

## Sprint calendar

Relays need time to pass. The sprint is **four phases; sessions inside a phase run in
parallel, phases are separated by a hard barrier** (queue drained via `GET /v1/queue/health`,
review queue worked by the maintainer Characters). A relay run without the barrier is
measuring a race, not a flywheel.

| phase | when | what happens | barrier |
|---|---|---|---|
| **P1 — Day 1** | seeded | Meridian's existing knowledge = `fixtures/v1` (80 gold memories, 9 transcripts). This is the org's history; arm B's `CLAUDE.md` files are its hand-written subset. | seed + drain |
| **P2 — The incident** | +1d | Tomas (platform) reverses a policy mid-sprint. Ada (payments) hits a PSP problem. Sessions are ingested. | **drain + review**: Petra and Lars must actually work the promotion queue. **Time them.** |
| **P3 — The consumers** | +3d | Ingrid (data), Jonas (web), Mira (new joiner) do tasks that *need* what P2 produced — and that their own repo cannot tell them. This is where `C − B` is measured. | drain + review |
| **P4 — The reckoning** | +5d | Sam audits a stale decision. Yusuf hunts the transcript excerpt. Dana reads analytics and decides whether to keep paying. Nadia is paged at 02:00. | — |

Between phases, **arm B's owners may edit their `CLAUDE.md`** (see `baseline.md` §
maintenance budget). They will not think to. That failure-to-update is arm B's honest,
measured rot — not an assumption we bake in.

## The fixture gap — read before claiming a cross-stack result

`fixtures/v1/` contains **zero programming-language content**. Not one line of Rust, Go,
Python or TypeScript; no repo files, no compiler errors, no idiom-specific pitfalls. The
`language:` field on memories is *natural* language (`en`/`cs`), not stack.

Consequences, stated plainly:

- **H3 (cross-stack noise) is `not probed`, not `clean`.** Nothing in the corpus can be
  right for Rust and wrong for Python, so nothing can be measured.
- The **web team and its `checkout-web` repo do not exist in the fixture** — they are a
  trial-only extension, and every journey that depends on them needs new material.
- **What IS measurable today, with no new fixtures:** cross-team knowledge flow (12 merge
  sets, 9 cross-team QA queries), retraction/temporal truth (6 supersession chains,
  including the marquee cross-team `std-retry` one where a *payments* memory supersedes a
  *platform* org policy — `mem-plat-0107` → `mem-pay-0043`), permission scoping (15 leak
  cases), contradiction precision (12 cases, 8 of them deliberate negatives), and
  cross-lingual retrieval (Czech).

Those five axes are the ones Brainiac claims to win on anyway. **Run those first.** The
stack-specific corpus (`fixtures/v2`: real repo trees, stack-specific pitfalls, transcripts
containing actual code) is a prerequisite for the cross-stack claim and is **not** a
prerequisite for the core verdict.
