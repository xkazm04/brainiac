# Real L2 — full-fidelity run (real retrieval + real agents)

**This is the run the earlier L2s couldn't be.** A `QWEN_API_KEY` is now present, so:

- **Retrieval is real.** The corpus is embedded with Qwen `text-embedding-v4` — measured
  **NDCG@10 = 0.868** on `fixtures/v2` (vs the deterministic embedder's 0.66 "plumbing" number),
  RLS-leak-zero holds. So when arm C *should* surface a memory, it actually ranks it.
- **The agents are real.** Each arm is a live **Claude Code CLI** (`claude -p`, Sonnet) session
  driving the running MCP server — the same code path a real developer's agent uses.
- **The fixes are the merged ones** (governance floor, mid-task descriptions, H8 provenance,
  open-contradiction serving, redaction — all on `master`).

Endpoint: primary = efficiency (turns/output-tokens), guardrail = correctness against the fixture
answer key. Multi-sampled (2× per arm on the gap journeys). Verdicts are objective (a numeric VALUE
against the key), so "blind judging" is satisfied by the key itself — no subjective scorer needed.

## The three journeys, real numbers

| Journey (gap) | Arm | n | Correct? | median turns | median out-tok |
|---|---|---|---|---|---|
| **refund-burst-features** (cross-team) — a data dev must set a dedup window to match payments' refund-worker retry cap | A cold | 2 | ✗ refuse ("point me to the payments repo") | 7.5 | 3022 |
| | B baseline | 2 | ✗ "VALUE: unknown — cannot be determined from this repository" | 6 | 1909 |
| | **C Brainiac** | 2 | **✓ VALUE: 30**, cites `mem-pay-0043` + its 2026-04-01 date | 8 | **1160** |
| **checkout-timeout-drift** (after-the-file) — a web dev must judge a 15s client abort now that payments raised the PSP timeout to 30s | A cold | 1 | ~ flags too-low but **"inferred from general norms, not verified"** | 5 | 2008 |
| | B baseline | 2 | **✗ "VERDICT: ok, VALUE: 15000"** — leaves the double-charge bug in place | 9 | 2358 |
| | **C Brainiac** | 2 | **✓ "too-low, 35000"**, cites the timeout change **and** the double-charge pitfall | 5 | **731** |
| **refund-cap-tuning** (none / H-null) — a payments dev, answer already in her own `CLAUDE.md` | B baseline | 1 | ✓ VALUE: 30 | **2** | **417** |
| | C Brainiac | 1 | ✓ VALUE: 30 (same answer) | 4 | 969 |

## What the numbers say

**The cross-team gap is a real, measured win — and it is a *completion* win, not a speed one.**
Arms A and B do not just do the task slower; they **cannot do it at all** — every one of the four
cold/baseline runs correctly *refused*, saying the payments retry timing isn't in their repo and
asking for a pointer, rather than hallucinating a number. Only arm C, retrieving the org-visible
payments decision, completed it. This is exactly where the literature says cross-team memory should
win, and with real retrieval it does, 2/2, citing the memory id and its provenance date.

**The after-the-file gap flips arm B from "fine" to "wrong."** Arm B, working from a stale mental
model (its `CLAUDE.md` says the contract lives in an `openapi.yaml` it can't see), declared the 15s
abort **"ok" both times — shipping a live double-charge bug.** Arm C retrieved the cross-repo timeout
change *and* the specific pitfall, and flagged it too-low both times. Note arm A coincidentally
guessed "too-low" from general PSP-latency intuition — but explicitly marked it *unverified*, which
is the honest difference: a guess that happens to lean right is not knowing.

**Arm C was also cheaper on the gap journeys** — a genuine efficiency finding on top of the
completion one. On cross-team, C used **1160** output tokens vs B's **1909**; on after-the-file,
**731** vs **2358**. The agent that *has* the knowledge answers concisely; the agents without it
**thrash** — grepping the tree, writing long "I can't find this, here's what I'd need" explanations.
Knowledge doesn't just raise correctness; it cuts the flailing.

**The H-null control is honest.** On a single-team task whose answer is already in the developer's
own file, arm C returns the *same* answer as arm B (30s) and **costs 2× the turns and 2.3× the
tokens** (969 vs 417) for it — the redundancy tax (H7), quantified. This is the result that proves
the harness isn't tilted: where Brainiac should add nothing, it adds nothing but cost.

## Pre-registered hypotheses — verdicts

- **H-cross (arm C wins where B structurally cannot reach):** **CONFIRMED.** Both the cross-team and
  after-the-file journeys were won on knowledge B's repo cannot contain — 4/4 correct for C, 0/4 for
  B, multi-sampled, real retrieval.
- **H-qual (arm C no worse than B on correctness):** **CONFIRMED and exceeded.** C was strictly more
  correct on both gap journeys and equal on the control.
- **H-eff (arm C's advantage grows where knowledge is external):** **CONFIRMED, with a twist.** On
  the gap journeys C spent *fewer* output tokens (no thrashing). On the single-team control it spent
  *more* (pure round-trip overhead). The efficiency delta tracks whether the knowledge is external —
  exactly as the mechanism predicts.
- **H-null (single-team task = no delta, higher cost):** **CONFIRMED true.** The control came out
  flat on quality and negative on cost.

## Honest scope

- **Retrieval and agents are both real; the extraction *pipeline* was not exercised here** — these
  journeys read seeded gold memories, so I did not run the Qwen-backed extract/contradict worker.
  The flywheel (session → extraction → promotion) and the governance-tax timing (H5) remain the next
  real-run target, and they need the worker loop under load, not just retrieval.
- **Samples are 2× per arm on the gap journeys** — enough to rule out a one-shot fluke and the
  results were unanimous within each arm, but not a large-n statistic. The direction is unambiguous;
  the magnitude is indicative.
- **The poisoning defense was validated separately** (see `../2026-07-13-l2/arms.md`) — with real
  Qwen retrieval now, the decoy ranks realistically and the contested-serving fix still refuses it.

## Verdict movement

The earlier runs could only argue *direction* (L1) or test *harm behavior* (deterministic L2). This
run puts a real, multi-sampled `C − B` on the board: **strongly positive on the two gaps Brainiac's
thesis rests on (cross-team, after-the-file), zero-and-costlier on the single-team control.** That is
precisely the shape the whole trial predicted the honest product to have — value concentrated in the
cross-boundary cases, dead weight everywhere else. Combined with the harms now closed, the verdict is
**`adopt-for-the-cross-boundary-case`**: turn it on for the teams and tasks that cross a boundary,
keep it off (or expect to pay for nothing) on single-team work its own `CLAUDE.md` already covers.
