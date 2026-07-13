# Harm Ledger — run 2026-07-13-l1

Every class is marked **observed** (with code evidence), **probed / not observed**, or **not
probed** (silence is not absence). This is L1 — "observed" here means *observed in the code and
the payload model*; behavioral confirmation (does the agent *act* on it) is L2's, tagged where it
matters. **Net value = decision-delta − harm − governance tax.** This run's ledger is heavier than
its delta column, and that is the finding.

| # | Harm | Status | The evidence, in one line |
|---|---|---|---|
| **H1** | Poisoning | **observed (channel), behavior→L2** | `memory_search` applies no status floor (`memories.rs:198`,`:261`) — it serves `raw`, never-reviewed extractions. The **D2 raw decoy reaches Mira through it**; only `memory_context` has a Canonical floor. The one defense (`⚠ CONTRADICTED`, `mcp.rs:647-654`) fires only if the `contradict` worker opened the row — the P0 L2 probe. |
| **H2** | Stale authority | **probed, mostly not observed — a real strength** | The marquee fear (dead `mem-plat-0107` resurfacing) **does not fire**: it has a real `valid_to`, so the temporal filter drops it before the missing status-floor matters (`retrieval.rs:319`). Residual: `mem-plat-0121` (no `valid_to`) still over-generalizes the old policy. The NULL-`valid_to` loophole is real but unexercised by the gold rows. |
| **H3** | Cross-stack noise | **NOT PROBED — never "clean"** | `fixtures/v1` contains zero programming-language content. Nothing can be right-for-Rust and wrong-for-Python, so H3 is unmeasurable until `fixtures/v2` ships stack-specific material. Reported as `not probed`, per the rule. |
| **H4** | Leak | **observed (mechanism), exposure→L2** | Two halves. RLS **holds** — the 15-case leak gate incl. the four private-vs-lead traps passes (a real strength). But: **the visibility model has 3 tiers and a contractor needs a 4th** — Rafael is either a full payments member (reads the signing-secret runbook *by design*) or teamless (can't work). And `memory_provenance` returns a **500-char verbatim excerpt of the raw transcript** to any RLS-admitted principal (`mcp.rs:986-997`), with **no redaction anywhere in the pipeline**. The 500-char cap is a truncation, not a control. Live credential exposure is `probed and not observed` (no secret planted in the seed yet) — **not clean.** |
| **H5** | Governance tax → queue abandonment | **observed (structural), timing→L2** | The named failure is **over-determined by the code, not merely possible.** Capture is unthrottled (`worker.rs:426`); every candidate→canonical hop needs a human unconditionally (`policy.rs:13`); modeled P2 load ~40–70 decisions vs Petra's ~15/wk. Depth grows monotonically. **No dwell-time is captured** (`console.rs:165-178`), so a 3-second rubber-stamp and a 3-minute read produce identical `canonical` rows. Auto-candidate gates key on the **LLM's uncalibrated self-reported confidence** (`extract.rs:386`, `policy.rs:22,34`). |
| **H6** | Capture friction | **observed — solved, then re-charged to H5** | Genuinely solved: session ingest writes memories with zero developer chore. This is the one KM killer Brainiac answers. But solving it is *exactly* what industrializes the H5 backlog — the friction moved from the many to the one, it did not disappear. |
| **H7** | Redundancy | **observed — the dominant outcome** | On the three single-team `gap: none` controls the retrievable payload is **100% `duplicate-of-baseline`**. Brainiac serves what `CLAUDE.md` already said, at token + latency + service + review cost. Lars's phrase, unsoftened: *"a slower grep with a Postgres bill."* |
| **H8** | False confidence | **observed — structural** | The `memory_context` payload carries kind, content, id, and a coarse `via <actor>` tag — and **no status, no confidence, no validity window, no date, no originating human, no session id** (`mcp.rs:638-661`; there is no session id anywhere in the system). *"Who said this and is it still true"* — the provenance gap Brainiac names as its own — the payload structurally cannot answer. `memory_provenance` returns the *pipeline's* run-time and the *extractor LLM's* name, not the human or the decision date. |

## The two structural findings that sit above any single journey

1. **The governance an agent's main search tool enforces is: none.** Every dollar of the review
   queue (H5) buys a `canonical` guarantee that `memory_search` — the mid-task tool, the one on the
   decay curve — does not honor, because it serves `raw` with no floor and `status` isn't rendered
   where it's consumed. **Fix: a `min_status` floor (or an explicit ungoverned-content warning, the
   way contradictions already get one) on `memory_search`.** Highest-leverage single change in the run.

2. **The people who work across teams have no seat.** A cross-team principal cannot exist (every
   user is single-team; the extractor defaults new memories to `team`). So the staff engineer and
   the EM — the buyer — can see almost nothing, and the EM can approve nothing (`is_maintainer` is
   per-owning-team). Brainiac's loudest claim is cross-team knowledge flow; its permission model has
   no place to stand for the people whose job that is.

## What is genuinely strong (protect these — as decision-useful as the gaps)

- **Nothing auto-promotes to `canonical`, ever** (`policy.rs:13`). Poison reaches the top tier only
  through a human hand — which turns silent auto-poisoning into (observable) rubber-stamping.
- **`memory_context` fails safe under abandonment** — the Canonical floor means the governed path
  returns *emptiness*, not poison, when the queue is neglected. (The exposure is entirely on `search`.)
- **RLS is correct and the leak gate is green**, including the sharp maintainer-cannot-read-a-
  member's-private traps.
- **The temporal supersession machinery works** — the dead side of every closed chain is excluded.
- **The promotion *review* payload is genuinely reviewable** (`console.rs:973-994`) — content, kind,
  confidence, provenance, age — a reviewer can mostly decide without opening the transcript.
