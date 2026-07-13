# Real flywheel — the last unrun-real piece, closed

The earlier real-L2 (`report.md`) proved retrieval and agent behavior with real Qwen, but read
**seeded** gold memories — it never exercised the write side: session → extract → promote →
another developer reads it. This run does, with the real Qwen worker
(`extract=qwen:qwen-max resolve=qwen:qwen-max contradict=qwen:qwen-max`).

## What it did, end to end

1. **Ada (payments) ingests a session** via the real MCP `memory_add`: a July incident note with two
   distinct learnings — a ledger circuit-breaker howto, and a "never process a chargeback and a
   refund for the same transaction concurrently" pitfall.
2. **The real Qwen worker extracted** a structured memory: `[pitfall] "Processing a chargeback and a
   refund for the same transaction concurrently creates a double ledger entry…"`, confidence 1.0.
3. **Policy auto-promoted** it `raw → candidate` (`pitfall_high_conf_auto_candidate` — the audit row
   confirms the rule fired), no human needed for the first hop.
4. **A DIFFERENT developer (Mira) retrieved it** via `memory_search` — served as `candidate`, and the
   governance fix correctly tagged it `governance: candidate` (provisional, not yet human-certified).
5. **Ada proposed** it `candidate → canonical`; **maintainer Petra approved** it over REST in
   **396 ms**; status → `canonical`.
6. **Mira's canonical briefing** (`memory_context`, Canonical-floored) now surfaces it — the fully
   governed loop closed: one developer's session knowledge reached another, human-certified.

**The flywheel turns, with the real model, on the merged code.** This is the first run to prove it.

## The real bug this run found — and fixed

The flywheel did not work on the first try. The real Qwen extractor **intermittently double-encodes a
nested array field as a JSON string** — `"entities": "[{\"name\":…}]"` instead of a native array —
and the strict `Vec<T>` deserializer rejected it:

> `extractor output unparseable after one repair: invalid type: string "[{…}]", expected a sequence`

The ingest job failed, retried, backed off, and **the session's knowledge never entered the store**.
This is exactly ARCHITECTURE §9 risk #1 ("extraction quality varies across BYOM providers") made
real — and it was **invisible to every prior test**, because `MockProvider` returns clean structs. A
production org on Qwen would silently lose a fraction of every session's knowledge.

**Fixed:** `extract.rs` now uses a lenient deserializer (`de_lenient_vec`) on the extractor's array
fields — it accepts a native array, a JSON-string-encoded array, or null, and still rejects genuine
garbage. Unit-tested (`tolerates_json_encoded_array_fields`) with both encodings. After the fix the
same session extracted and promoted cleanly.

## Governance-tax + extraction-quality observations (H5 / §9)

- **Review burden this session: 1 maintainer action, 396 ms.** The candidate hop was automatic; only
  candidate→canonical needed a human. Low burden — but this was a *clean, single-topic* note; a real
  incident sprint's queue depth is the thing to measure at scale, and this run doesn't establish that
  curve (one session ≠ a sprint).
- **Extraction precision looked good, recall did not.** Qwen extracted the sharpest pitfall at
  confidence 1.0 and correctly — but it **dropped the second learning entirely** (the ledger
  circuit-breaker howto). The `memory_add` was tagged `kind: pitfall`, which likely biased the
  extractor toward the single pitfall and away from the co-located howto. **Finding:** a kind hint on
  a multi-learning source can suppress recall of the other kinds. Worth a fixture-backed extraction
  eval (precision AND recall per provider) before trusting the flywheel's completeness — the golden
  transcripts already carry `must_extract` gold for exactly this.
- **The auto-candidate gate keys on the model's self-reported confidence** (1.0 here) — a reminder
  that a confidently-wrong extraction would walk the candidate promotion unchallenged (backlog item
  P2.2, confidence calibration).

## Net

The real-run arc is now complete: retrieval (real Qwen ranking), agent behavior (real Claude
sessions, cross-team + after-the-file wins + harm probes), and now the **write-side flywheel**
(real extraction → governance → cross-developer read). It cost one genuine bug fix to get there — the
kind of provider-specific breakage only a real run finds — and surfaced a concrete extraction-recall
question that belongs in the next eval. Verdict is unchanged and now better-evidenced:
**adopt-for-the-cross-boundary-case**, with extraction recall the next thing to quantify.
