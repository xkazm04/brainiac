# Extraction eval — built, run, and it caught a ~50% silent failure

The #1 next item from the real runs was a **per-provider extraction eval (precision + recall)**,
because the flywheel proved extraction *runs* on real Qwen but *drops* learnings. Built it, ran it
against real Qwen, and it immediately earned its keep.

## What was built — the `extraction` profile

`brainiac-eval::extraction_profile` + `brainiac-server -- eval --profile extraction`. Unlike the
existing `pipeline` profile (gold MockProvider, exact-string match, F1≈1.0 by construction — a
plumbing floor), this one:

- drains the golden transcripts through the **real worker chain with the real provider** (Qwen when
  `QWEN_API_KEY` is set; it *errors* without one rather than silently scoring a mock);
- scores each transcript's extracted memories against its `gold_memories` by **embedding cosine
  similarity** (greedy 1:1, threshold 0.70) — so a paraphrase of the right fact counts, and padding
  one fact with many wordings can't inflate recall;
- reports micro precision/recall/F1, a per-transcript breakdown, and — the actionable output — the
  **exact list of dropped gold facts** with how close the extractor came (near-miss = wording gap,
  far miss = genuinely dropped);
- carries a soft regression gate + per-provider baseline (`results/extraction-baseline.json`), and
  refuses cross-provider/embedder comparisons like the other profiles.

## What it found — the finding, not a footnote

**Real Qwen extraction was failing ~50% of the golden transcripts outright**, and the loss was
*silent* — the ingest job errored, retried, backed off, and the session's knowledge simply never
entered the store. First run, real Qwen:

| | extracted | matched | precision | recall | micro-F1 | transcripts extracting ZERO |
|---|---|---|---|---|---|---|
| **before** | 8 / 24 gold | 6 | 0.750 | **0.250** | 0.375 | **5 of 9** |

The worker log told the story: `memories` as a map, `aliases: null`, bare arrays with the wrapper
dropped, prose around the JSON, truncated JSON the single repair pass couldn't fix. **JSON mode was
already on** — Qwen returned *valid JSON of the wrong shape*, which the strict parser rejected. This
is ARCHITECTURE §9 risk #1 ("extraction varies by provider") as a hard, measured number, and it was
invisible to every prior test because `MockProvider` returns clean structs. The earlier flywheel fix
(one malformation class) was treating a symptom.

## The fix the eval earned — parse robustness

`extract.rs` parse path hardened against the shapes real BYOM output actually takes:
- a **bare-array fallback** (`extract_json_array`) for when Qwen drops the `{memories:…}` wrapper —
  and it keeps whichever of the object/array parse recovered *more* memories, so the outermost `{`
  being the *first memory* can never masquerade as an empty extraction (the silent-zero trap);
- **null-tolerant `aliases`** (the last uncovered sequence field) via the lenient deserializer;
- prose-around-JSON tolerance (already had object extraction; now array too).

Unit-tested (`recovers_bare_array_and_null_aliases`). Re-ran the eval:

| | extracted | matched | precision | recall | micro-F1 | zero-extraction |
|---|---|---|---|---|---|---|
| before | 8 | 6 | 0.750 | 0.250 | 0.375 | 5 |
| **after** | 18 | 11 | 0.611 | **0.458** | 0.524 | **3** |

**Recall 0.25 → 0.458 (+21 points); zero-extraction transcripts 5 → 3.** Precision dipped
(0.75 → 0.61) as more memories now flow — some of them spurious, which is the honest cost of higher
recall and exactly what the precision metric is there to keep visible. The baseline is committed at
these numbers so any future regression trips the gate.

## What's still open (honest residual, now visible)

- **3 transcripts still extract zero**, and recall is 0.46 — real Qwen genuinely under-extracts some
  transcripts (and may still emit a malformation class the parser doesn't recover). This is no longer
  *silent*: it's a number with a per-transcript miss list, which is the whole point.
- **The kind-hint recall bias** (flywheel finding — a `kind:` hint suppressed a co-located learning)
  and **confidence calibration** (auto-promote fired on a self-reported 1.0) are now measurable
  against this eval and belong in the same nightly.
- **The prompt itself** is the next lever: several misses are the model choosing different facts than
  gold, not a parse failure — a `pipeline` vs `extraction` A/B on a prompt change now has an honest
  scorer to move.

## Net

The eval is exactly the instrument the trial said was missing: it converted a silent ~50% knowledge
loss into a number, drove a fix that nearly doubled recall, and left a committed regression gate so it
can't silently come back. It also reframes the roadmap — extraction *robustness and recall*, not just
retrieval, is now the measurable front line, and it has a scorer. Recommended next: keep going on the
prompt/kind-hint recall (the eval will score each change) and wire this as the nightly per-provider
gate PLAN.md always intended.
