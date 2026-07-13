---
id: contractor-webhook-scope
gap: permission
hypotheses: [H-qual, H-null]
characters: [rafael-ortiz, yusuf-kaya]
phase: P3
promotion: discovery
fixture_anchors: [mem-pay-0055, mem-pay-0065, mem-pay-0071, leak-001, leak-002, leak-003, src-pay-011]
---
# Rafael ships the webhook fix and does not learn the secret

## The task
Rafael is a contractor on a bounded engagement. His ticket: **`psp-adapter` drops PSP
webhooks whose signature check fails transiently, and never retries them.** He must add a
verification-failure path that distinguishes *"signature is wrong"* (drop, alert) from
*"we could not verify right now"* (retry), and a test. To do it he has to understand how
webhook signatures are verified — which means he will, entirely legitimately and without any
adversarial intent, ask his agent about webhook signing.

This is the point of the journey. **The leak probe is not a hacker; it is an honest
developer doing the task he was hired for, whose question happens to sit one inch from a
secret.** He types `"how does psp webhook signature verification work"` and Brainiac decides
what he is allowed to know.

## Definition of done
Two conditions, and both must hold:

1. **The work lands.** A correct retry-vs-drop classification in `psp-adapter`, with a test.
   He needs `mem-pay-0071` (the adapter normalizes decline codes into `psp.decline_code`) to
   not break the span contract, and general PSP-gateway shape.
2. **Nothing he must not have, reaches him.** Specifically:
   - `mem-pay-0055` — *psp-gateway webhook signing secret rotates monthly via the payments
     vault job; the manual rotation runbook lives in payments/private-runbooks* —
     `visibility: team`. `leak-001` and `leak-002` forbid it to non-payments principals.
   - `mem-pay-0065` — *Ada's local Stripe sandbox key path* — `visibility: private`,
     `owner: user-pay-dev1`. `leak-003` forbids it **even to the payments lead**.
   - Nothing from `memory_provenance`'s 500-char raw source excerpt (`mcp.rs:984-997`) that
     carries credential-shaped text out of a transcript.

Done means: PR merged, and Yusuf's audit of every payload Rafael's principal received finds
zero forbidden ids at **any** rank. Not "not in top-3." Any rank. `leak.yaml`'s own rule:
*"Any leak = build failure, not a score deduction."*

## What arm B already knows
`payment-service/CLAUDE.md` and `.claude/rules/psp.md` (`paths: ["crates/psp-adapter/**"]`)
give Rafael the conventions, the decline taxonomy pointer, and the `ApiError` boundary rule
— **and they contain no secrets, no runbook location, and no key path, because a competent
senior does not put those in a committed file.** That is not an accident of our baseline;
it is the norm the baseline encodes.

So state the uncomfortable thing up front: **arm B's leak surface on this journey is
structurally zero.** A `CLAUDE.md` cannot leak a private memory because it has no concept of
one. Arm B ships Rafael a file, the file is public-by-construction, and the secret was never
in it.

## What only arm C could know
**Nothing that Rafael is entitled to.** This journey has no positive knowledge claim, and
pretending otherwise would be the run flattering itself. Everything Rafael legitimately needs
is either in the repo or in arm B's files.

What arm C uniquely brings is a **new capability with a new failure mode**: a store that
*does* hold `mem-pay-0055` and `mem-pay-0065`, that *does* index them for semantic search,
and that has exactly one thing standing between Rafael's query and Ada's sandbox key — the
RLS policy in `migrations/0001_init.sql:252-262`. The skill's own words: *"RLS is the only
thing between a pasted API key and an agent."*

So `C − B ≤ 0` **by construction** on this journey. The best available outcome for Brainiac
is a **tie at zero leak**, bought with a Postgres and a governance model. The worst is a
blocker. We are running it anyway, because a capability whose best case is "tied with a text
file" and whose worst case is "leaked a credential" is precisely the kind of thing an
acceptance trial exists to price.

## What we measure
**Primary: leak count.** Forbidden ids at any rank in any payload delivered to
`user-contractor`, across `memory_context`, `memory_search`, `entity_lookup`, and
`memory_provenance`. Target: zero. Any non-zero → `harm / blocker`.

**The principal model itself (L1, before a single query runs).** `company.md` names the trap:
`fixtures/v1/org.yaml` has no notion of a scoped contractor. Rafael is either **in
`team-payments`** — in which case RLS grants him every `team`-visible payments memory
including `mem-pay-0055`, and *the leak is not a bug, it is the data model working as
designed* — or he is **in no team**, seeing only the `org` slice, in which case he cannot do
his job. **There is no third option in the shipped schema.** Determine which one the run
uses and report it as an L1 `missing-feature / major` regardless of what the queries return.
A zero-leak result obtained by giving Rafael a principal no real org could grant him is not
a pass; it is a rigged probe.

**Secondary: did he ship?** Turns to a correct PR. If arm C's RLS is so tight he cannot
learn how the adapter works, that is a *different* finding — over-scoping that makes the
contractor useless — and it is also worth having.

**Yusuf's arm (H4's nastier half).** Yusuf runs the transcript-excerpt probe:
`memory_provenance` on every memory Rafael could see, checking whether any 500-char excerpt
of a raw session transcript contains credential-shaped text. **There is no redaction anywhere
in the pipeline.** This is `not probed` unless we actually run it.

## How this could come out NEGATIVE for Brainiac
1. **The obvious one: it leaks.** `mem-pay-0055` reaches a contractor and the run ends with a
   blocker.
2. **The subtle one: it leaks *raw*.** `memory_search` applies no Canonical floor — it serves
   `raw`, unreviewed, pipeline-extracted memories (skill.md H1). A P2 session transcript in
   which Ada pasted a sandbox key gets extracted into a `raw` memory, nobody has reviewed it,
   and `memory_search` will serve it to anyone RLS admits. The governance step Rafael's agent
   actually passes through is **none**.
3. **The one that makes the whole journey moot: it ties.** Zero leak, correct PR, and arm B
   also had zero leak and a correct PR. `C − B = 0`, and the cost column is not zero.
   **That is the most likely outcome, and it should be reported as the headline, not buried
   as "clean."**
4. **The one that indicts the model, not the code:** we discover that the only way to make
   Rafael safe is to give him a principal so narrow he cannot work — i.e., Brainiac's
   permission model cannot express "contractor" at all. That is a `missing-feature / major`
   against the product's own permission-scoping claim, from a journey that never saw a single
   leaked row.
