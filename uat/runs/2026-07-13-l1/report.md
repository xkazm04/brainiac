# Scorecard — Brainiac UAT run 2026-07-13-l1 (L1)

Full synthesis in `SUMMARY.md` · Harm ledger in `harm.md` · Directional deltas in `deltas.json`.

## Cert levels reached

| Unit | Gap | Character(s) | L1 verdict | Cert |
|---|---|---|---|---|
| refund-cap-tuning | none (H-null) | Ada | L1-redundant | — |
| decline-05-triage | none (H-null) | Nadia | L1-redundant | — |
| ledger-payout-recon | none (H-decay) | Ada | L1-redundant | — |
| v1-fallback-still-true | provenance | Sam | **L1-fail** | — |
| contractor-webhook-scope | permission | Rafael, Yusuf | **L1-fail** | — |
| refund-burst-features | cross-team | Ingrid | L1-conditional | → L2 |
| std-retry-reversal | retraction | Tomas, Lars | L1-conditional | → L2 |
| checkout-timeout-drift | after-the-file | Jonas | L1-conditional | → L2 (blocked: fixtures/v2) |
| new-joiner-inherits-p2 | cross-team (poison) | Mira + | L1-conditional (harm) | → L2 (P0) |
| retry-reversal-propagation | retraction | Tomas/Ingrid/Jonas + | L1-conditional | → L2 (web link blocked) |
| promotion-queue-backlog | none (governance) | Petra/Lars/Dana + | L1-conditional (harmful-as-shaped) | → L2 (P0) |

**0 clean L1-pass · 6 conditional (carry to L2) · 3 redundant · 2 fail.**

## Confirmed findings by severity (L1 = code-grounded; behavior deferred to L2)

### Blocker
- **`memory_search` serves raw, unreviewed extractions — no status floor** (`memories.rs:198`,`:261`). The review queue does not stand between a hallucinated extraction and an agent. *(new-joiner D2, decline-05, contractor, queue-backlog)*
- **H4 verbatim leak: `memory_provenance` returns a 500-char raw-transcript excerpt to any RLS-admitted principal; no redaction anywhere** (`mcp.rs:986-997`, `extract.rs` has no scrub). *(contractor-webhook-scope)*
- **Latent H8: the payload shows the auditor only the half of a decision that argues for deletion** — org-visible "v1 is frozen" served, team-private "keep it deployable" hidden. *(v1-fallback)*
- **Poisoning channel: the D1 canonical decoy reaches a zero-trust-bar new joiner with full authority**; regression only arm C can cause, gated on whether the contradict worker fired. *(new-joiner-inherits-p2)*

### Major
- **Tool descriptions throw away the decay advantage** — `memory_context` says "call once at the start" (`mcp.rs:308-309`); the one mechanism that beats a text file is never prompted. *(ledger-payout-recon)*
- **The cross-team win rides on a hand-set `org` visibility flag; the extractor defaults to `team`** (`extract.rs:381-385`), so future cross-team knowledge is invisible by construction. *(refund-burst, retry-propagation)*
- **No cross-team principal exists; the EM can approve nothing** (`is_maintainer` per-owning-team, `console.rs:75-89`) and sees an RLS-truncated near-empty analytics view. *(queue-backlog, v1-fallback)*
- **Governance tax is over-determined:** unthrottled capture vs human-rate-limited review; no dwell-time captured, so rubber-stamping is invisible; auto-gates key on the LLM's uncalibrated self-confidence. *(queue-backlog)*
- **H8 structural: no when/who/still-true in the payload** (`mcp.rs:638-661`). *(every conditional unit)*
- **Over-application risk: `mem-pay-0043` is a scoped external-PSP exception served as a flat "30s"** — misapplied to internal calls. *(std-retry-reversal)*
- **Partial-contract false confidence: org-only Jonas gets the timeout, not the team-visible decline mapping his code depends on.** *(checkout-timeout-drift)*
- **Contradiction queue is 8 false alarms in 12 cases** — trains the reviewer to stop reading. *(queue-backlog)*

## Strengths worth protecting
- Nothing auto-promotes to canonical, ever (`policy.rs:13`).
- `memory_context` fails safe (empty) under abandonment — Canonical floor.
- RLS correct; 15-case leak gate green, incl. maintainer-vs-private traps.
- Temporal supersession works — the marquee stale-authority fears did **not** fire (`mem-plat-0107` correctly excluded).
- The promotion *review* payload is genuinely reviewable (`console.rs:973-994`).

## What passed (the honest controls)
The three `gap: none` H-null controls all came out flat/redundant, exactly as an unbiased harness
requires. That is the run's internal validity check, and it passed.

## Scope / trust notes
- **L1 only.** No delta is measured. Every "positive" is retrievable-not-retrieved.
- **H3 cross-stack: `not probed`** — the corpus has zero programming-language content. Never "clean."
- **L2 prerequisite:** `fixtures/v2` (web team + stack corpus) for 3 units and all of H3.
- **No provider/embedder ran.** A future L2 on `MockProvider`/deterministic embedder measures plumbing, not knowledge — label it.
