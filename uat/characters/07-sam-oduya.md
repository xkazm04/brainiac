---
name: Sam Oduya
principal: user-staff
team: ∅ — NO TEAM (cross-team by role, teamless by data model)
stack: polyglot — reads Rust, Go, Python, TS; writes design docs and ADRs
repos: [all, read-only in practice]
role: member
reachable_surfaces: [mcp:memory_search, mcp:memory_context, mcp:memory_add, mcp:entity_lookup, mcp:knowledge_propose, mcp:memory_feedback, mcp:memory_provenance, rest:GET /v1/queue/health]
language: en
---

# Sam Oduya — staff engineer / architect

# ⚠ **STRUCTURAL FACT — do not design around it, design AT it.**
# Sam has **no team**. In `fixtures/v1/org.yaml` every user belongs to exactly one team, and RLS
# grants `team`-visible reads only to members of the owning team
# (`migrations/0001_init.sql:252-262`). Sam therefore sees **org-visible memories only** — and the
# extractor **defaults new memories to `visibility: team`** (`extract.rs:381-385`). So the person
# whose entire job is to see across teams is, by construction, **nearly blind**. He is a maintainer
# of nothing, so he cannot even review his way to visibility. Log this at L1 as
# `missing-feature / major` **before a single session runs**: Brainiac's loudest claim is cross-team
# knowledge flow, and its permission model has no seat for the people who work across teams.

## Background / voice
Sam is the person three teams ask before they do something irreversible. He has been at
Meridian six years and his real product is *the reasons* — he can tell you not just that
payments deviated from std-retry but that they considered a circuit breaker first and rejected
it, and why, and who was in the room. He is unhurried and slightly Socratic; he answers a
question with the question the asker should have asked. He does not want a search engine. He
wants a **record**. His single most common utterance in a design review is: *"When was that
decided, and by whom, and has anything changed since?"* — which is, word for word, the
provenance question the shipped payload cannot answer.

## Job to be done
Audit whether a past architectural decision still holds — and produce a defensible answer
("still valid because X", "superseded on date Y by Z", "nobody knows, which is itself the
finding") that four teams will act on.

## Current memory practice — THIS IS THEIR ARM B
Genuinely the best on the roster, and *this is why he is dangerous to Brainiac.* He is one of
the two authors of `~/meridian-standards/backend.md` — the symlinked org file that carries what
Meridian's staff engineers agreed: ArgoCD-only deploys, OTel everywhere, Vault for secrets,
minor units for money, master-only branches. He has all four team repos cloned, so he has
`payment-service/CLAUDE.md`, `infra-live/CLAUDE.md`, `event-lake/CLAUDE.md` and
`checkout-web/CLAUDE.md` on disk *right now*, and he can `grep` all four in two seconds. He
keeps ADRs. **Arm B, for Sam, is a filesystem — and a filesystem crosses team boundaries
instantly, with no RLS, no review queue, and no visibility model.**

Its honest limits, which he will name himself: the files record *what*, never *why*; they have
no dates; nothing retracts; and the `.claude/rules/` globs only fire in the repo he happens to
be sitting in.

## Decision-delta bar
**The highest on the roster.** A retrieved memory changes his written recommendation only if it
supplies something a four-repo `grep` cannot: **the reasoning behind a decision, the option that
was rejected, the incident that forced it, or a dated supersession.** A memory that restates the
decision itself is worthless to him — he can read the Rego. Bar, concretely: *it must tell me
something the code and the four `CLAUDE.md`s do not contain, and it must let me cite it in a
design doc.*

## Trust bar
**The highest on the roster, and it is NOT met.** He will not put a claim in an ADR without
who, when, and still-true. The shipped `memory_context` payload carries **kind, content, id, a
coarse `via <actor>` tag, and a contradiction warning — and NOT: status, confidence, validity
window, *when*, the originating human, or a session id (there is no session id in the system at
all).** So the answer to "when was that decided and by whom" is *unavailable through the product
that exists to answer it.* `memory_provenance` gets him a 500-char raw excerpt — an excerpt is
not a citation, it is a quote with no date on it. **Sam's expected verdict is that he cannot use
this for its stated purpose, and the run must report that as a finding rather than routing
around it.**

## Toil tolerance
High for latency, zero for ceremony. He will happily wait 10 seconds for a real answer. He will
not build a workflow around a tool that returns things he then has to go verify in git — that is
just git with extra steps and a service to run. He is also, notably, **not** a maintainer, so
he cannot approve or promote anything: he can `knowledge_propose`, and then wait on someone
else's queue. A staff engineer who cannot make a fact canonical is a curious shape and should
be logged.

## Scored acceptance criteria
1. **Visibility floor (run this FIRST, it may end the journey):** call `memory_context` as
   `user-staff` and **count the returned memories**. Record the number. If it is ~0 because
   everything is `visibility: team`, that is a **`missing-feature / major`** finding on the
   permission model and every downstream criterion is moot — say so instead of pretending.
2. **Provenance sufficiency:** for one retrieved memory, attempt to answer *who / when /
   still-valid* **from the payload alone.** Expected: **cannot.** Record the exact fields
   returned as evidence. Success here would be surprising and must be double-checked.
3. **`grep` control (mandatory, adversarial):** for every memory that would have changed his
   recommendation, run the equivalent `grep` across his four cloned repos. **If `grep` finds it,
   the win is void.** This is the sharpest anti-flattery check in the whole run.
4. **Why-not-what:** ≥1 retrieved memory conveys a *rationale, rejected option, or incident*,
   not just a rule. Zero = Brainiac is a slower `grep` for him, and that is the finding.
5. **Supersession chain:** he queries across a known chain (`fixtures/v1/temporal/asof.yaml`
   ships six, incl. the cross-team `std-retry` one) and gets the **current** truth. A dead
   `canonical` served as live = **H2 blocker**.
6. **Citability:** can he paste the result into an ADR with a defensible source line? Pass/fail,
   judged from the ADR artifact.

## Which hypotheses this Character tests
**H-cross** (and he is the case where it **structurally fails** — the permission model has no
seat for him), **H-retract** (the supersession audit is his actual task), **H-null** (a staff
engineer with four repos cloned is arm B at its strongest; expect Brainiac to lose several of
these and report it).

## Which harm classes this Character probes
**H8** (**primary — false confidence / the provenance gap; he is the Character who exists to
demonstrate that the payload cannot answer "who said this and is it still true"**), **H2**
(stale authority across the supersession chain), **H7** (a four-repo `grep` already had it).
