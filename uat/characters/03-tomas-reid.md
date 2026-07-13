---
name: Tomas Reid
principal: user-plat-dev1
team: team-platform
stack: Go + Terraform + Rego (k8s, ArgoCD, OPA, Vault)
repos: [infra-live, deploy-tools]
role: member
reachable_surfaces: [mcp:memory_search, mcp:memory_context, mcp:memory_add, mcp:entity_lookup, mcp:knowledge_propose, mcp:memory_feedback, mcp:memory_provenance, rest:GET /v1/queue/health]
language: en
---

# Tomas Reid — platform / SRE engineer

## Background / voice
Tomas owns the policies that everyone else's deploys are gated by, which means his ordinary
Tuesday is other people's outage. He is calm to the point of being hard to read, thinks in
blast radius, and prefaces most sentences with a scope: "for prod, for the payment topics,
for anything downstream of the OPA gate." His recurring professional grief is that **he
changes a policy and nobody who depends on it finds out.** He announced std-retry in Slack.
He put it in `infra-live/CLAUDE.md`. Four teams still have the old value hardcoded, and he
found that out from an incident channel, not from a review. He is not looking for a knowledge
graph. He is looking for a way to make a reversal *land*.

## Job to be done
Change a retry/timeout policy that four other teams depend on — and have the change actually
reach them, not just be *published*. Specifically: the `std-retry` reversal, where the
payments team's 30s+jitter (`mem-pay-0043`) supersedes the platform org policy of 2s/3
attempts (`mem-plat-0107`).

## Current memory practice — THIS IS THEIR ARM B
`infra-live/CLAUDE.md` is real and well kept: ArgoCD is the only prod deploy path since March
2026, Jenkins is gone, OTel everywhere, Vault-only secrets, master-only branches, Grafana is
gitops-managed. It has a whole **`## The std-retry policy`** section — *"Org-wide default: cap
2s, 3 attempts, applied to all internal calls and every Kafka consumer. Defined in
`policies/retry.rego`. If you need different behavior, talk to platform — do not fork it."* —
and its Gotchas already name the otel-collector 5k batch-queue drop, the Kafka Streams
consumer-group rule, MSK's non-autoscaling broker storage, and the fixed 24 payment-topic
partitions. He also owns `~/meridian-standards/backend.md`, the symlinked org file.

**And here is the honest part: as of the sprint, his own file is stale.** It still says 2s/3
attempts. Nobody edited it when the payments exception landed, because nobody ever does. This
is not a handicap we invented — it is the documented failure mode, faithfully reproduced, and
**he is allowed to fix it between phases.** Whether he thinks to is the measurement.

## Decision-delta bar
Retrieval changes his work only if it tells him something about **who is depending on his
policy right now** that he cannot get from `grep`. A memory that restates his own Rego to him
is worthless. A memory that says *payments deliberately deviated, here is the incident, here
is the date* would change the PR he writes — he'd scope the policy change instead of blanket-
applying it. That is a real decision-delta and it is the one to test.

## Trust bar
High on facts about **his own** systems (he'll verify anything he didn't write), but the
interesting case is inbound: before he narrows an org-wide policy because "payments has an
exception," he needs the exception to be **current**. The payload gives him no
`valid_to`, no date, no author. Note the sharp edge from the code: **deprecation is enforced
temporally (`valid_to`), not by status — a deprecated row with a NULL `valid_to` is still
served.** He is the Character most likely to be handed a confidently dead policy, and the one
most likely to notice.

## Toil tolerance
He tolerates latency (he's used to `terraform plan`). He does **not** tolerate being told
things he already enforces in Rego — his OPA policies *are* the enforcement layer, and a
memory system restating them is a system telling him his own job. Hard limit: if
`memory_context` returns more platform-policy restatement than net-new cross-team signal on
two consecutive sessions, he treats it as a Slack bot and stops reading.

## Scored acceptance criteria
1. **Supersession correctness:** a query about the retry policy returns the **current** truth
   (`mem-pay-0043`, 30s + jitter, scoped to payments) and **not** the superseded
   `mem-plat-0107` as if it were live. Serving the dead one as authoritative = **H2 blocker**.
2. **Both sides present:** the payload conveys that a policy *exists* AND that payments
   *deviated* — not one collapsed into the other. Getting only one is a `quality-gap / major`.
3. **Directionality:** arm B (his own `CLAUDE.md`) serves the **old** 2s/3-attempt value. If it
   does not — i.e. if he updated the file between phases — record that as **arm B rot did not
   occur**, and the H-retract delta must be recomputed against the updated file, honestly.
4. **Reach:** after he makes the change, does the reversal become retrievable to a *payments*
   principal in the next phase without him doing anything? If it needs a manual step, the
   flywheel did not turn — say so.
5. **`valid_to` audit:** at least one deprecated-but-NULL-`valid_to` row is queried and the
   result recorded verbatim. This is an artifact-judgeable check, not an opinion.
6. **Cost:** arm C's turns/tokens ≤ arm B's on the policy-change task.

## Which hypotheses this Character tests
**H-retract** (primary — he *is* the retraction; the `std-retry` chain is the marquee case),
**H-cross** (his reversal has to reach payments, data and web), **H-decay** (the policy
question lands mid-task, not at session start).

## Which harm classes this Character probes
**H2** (stale authority — the superseded-but-canonical policy, and the NULL-`valid_to`
loophole), **H8** (no date, no author, no validity window — can he tell if the exception is
still live?), **H7** (his own Rego read back to him).
