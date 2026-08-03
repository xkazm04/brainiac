//! Extract stage: source text → BYOM call → validated raw memories +
//! entities + relations, all provenance-stamped (stage 2 of §3).

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{
    ActorKind, EdgeRelation, EntityKind, Lifecycle, MemoryKind, MemoryStatus, Visibility,
};
use brainiac_gateway::{ChatProvider, ChatRequest};
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// Versioned prompt — changes here must not drop eval scores past the gate
/// (EVAL.md §3.2), which is why it lives in code, not config.
///
/// A V2 recall rewrite was attempted (UAT 2026-07-14) — exhaustiveness language,
/// a worked few-shot example, per-kind definitions — and the `extraction` eval
/// REJECTED it: recall fell 0.458 → 0.29 (minimal nudge) / 0.17 (full rewrite),
/// with the model turning MORE conservative, not less. A large share of the
/// run-to-run swing is stochastic parse failure (zero-extraction count moved
/// 3/5/6 across single runs), so prompt wording is not the dominant lever here —
/// extraction ROBUSTNESS/retry is. V1 stands; the eval guarded the regression.
pub const EXTRACT_SYSTEM_PROMPT_V1: &str = "\
You distill organizational knowledge from a work transcript into discrete memory units.

Extract ONLY durable, reusable knowledge: facts, decisions, patterns, pitfalls, howtos.
NEVER extract small talk, speculation, or ideas that were explicitly rejected in the conversation.

Respond with ONLY a JSON object:
{\"memories\":[{
  \"kind\":\"fact|decision|pattern|pitfall|howto\",
  \"title\":\"a short label, <= 80 chars, no trailing period\",
  \"content\":\"one self-contained natural-language statement\",
  \"lifecycle\":\"shipped|in_flight|proposed\",
  \"detail_md\":\"optional: the verbatim code/config/table the statement summarizes\",
  \"visibility\":\"team|org\",
  \"confidence\":0.0,
  \"entities\":[{\"name\":\"...\",\"kind\":\"service|repo|tech|feature|concept|team\",\"aliases\":[\"...\"]}],
  \"relations\":[{\"src\":\"entity name\",\"rel\":\"uses|depends_on|owns|deprecates|relates_to\",\"dst\":\"entity name\"}]
}]}

Rules: entity names verbatim as the team says them (do NOT normalize across teams);
aliases are OTHER surface forms the transcript uses for that SAME entity (acronyms,
short names, spelled-out forms) — omit or leave [] when the entity is named only one
way, never invent synonyms; relations only between listed entities; confidence
reflects how explicitly the transcript supports the statement.
lifecycle: omit (defaults to shipped) unless the transcript is explicit that the thing
is not yet in production — \"in_flight\" = decided and underway, \"proposed\" = intended,
not started. Never guess.
detail_md: omit unless the transcript contains the literal artifact the statement is
about (a code block, config snippet, or table) — copy it verbatim into a markdown
block. Never write prose here and never invent an artifact; content stays the claim.
title: how a person would refer to this claim in a list — name the thing and what
about it (\"psp-gateway retry cap\", not \"Retry\" and not the whole sentence). It is a
label, NOT a summary: content must still stand alone without it, because an agent is
served the claim without the row it was listed in.";

/// Lenient array deserializer for extractor output. Real BYOM providers (Qwen
/// among them) intermittently emit a nested array field as a JSON-ENCODED STRING
/// — e.g. `"entities": "[{\"name\":\"x\"}]"` instead of a native array — which a
/// strict `Vec<T>` deserialize rejects, stalling the whole ingest job (found live
/// in the UAT flywheel run, 2026-07-13; MockProvider never exercised it). Accept
/// a native array, a JSON-string-encoded array, or null; anything else still
/// errors so genuine garbage is not silently swallowed.
///
/// Deliberately STRICT per ELEMENT, and that strictness is load-bearing —
/// measured, not assumed. Salvaging broken elements (keep the good ones, drop
/// the bad) sounds like free recall and is the opposite. Both forms were built
/// and run against the `extraction` eval on real qwen-max, and both lost:
/// salvaging here scored recall 0.095 / precision 0.500, and salvaging as a
/// post-repair last resort scored 0.333 / 0.819, against 0.349 / 0.897 for
/// leaving the array strict. The mechanism is that a partial parse
/// SHORT-CIRCUITS the repair loop — a rejected array fires the repair
/// re-prompt and the re-ask comes back COMPLETE, while a fragment banks the one
/// element that happened to deserialize and discards the rest. See the
/// regression test `a_partly_broken_array_drives_a_repair_instead_of_settling_
/// for_a_fragment`, which exists so the idea is not re-implemented on intuition.
fn de_lenient_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    use serde::de::Error;
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(Vec::new()),
        v @ serde_json::Value::Array(_) => serde_json::from_value(v).map_err(D::Error::custom),
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                Ok(Vec::new())
            } else {
                serde_json::from_str(t).map_err(D::Error::custom)
            }
        }
        _ => Err(D::Error::custom(
            "expected an array or a JSON-encoded array string",
        )),
    }
}

#[derive(Debug, Deserialize)]
struct ExtractionOutput {
    #[serde(default, deserialize_with = "de_lenient_vec")]
    memories: Vec<ExtractedMemory>,
}

#[derive(Debug, Deserialize)]
struct ExtractedMemory {
    kind: String,
    /// A short label for the row (migration 0023). Optional at every layer: a
    /// provider that ignores the instruction, or an older prompt, must not cost
    /// us the memory — the archive falls back to `content`.
    #[serde(default)]
    title: Option<String>,
    content: String,
    /// KB-PLAN D2. Absent/unparseable → [`Lifecycle::default`] (shipped): the
    /// facet is a bonus signal, never a reason to drop a memory.
    #[serde(default)]
    lifecycle: Option<String>,
    /// KB-PLAN D3. Absent for most memories; see [`clean_detail`].
    #[serde(default)]
    detail_md: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default, deserialize_with = "de_lenient_vec")]
    entities: Vec<ExtractedEntity>,
    #[serde(default, deserialize_with = "de_lenient_vec")]
    relations: Vec<ExtractedRelation>,
}

#[derive(Debug, Deserialize)]
struct ExtractedEntity {
    name: String,
    #[serde(default = "default_entity_kind")]
    kind: String,
    #[serde(default, deserialize_with = "de_lenient_vec")]
    aliases: Vec<String>,
}

impl ExtractedEntity {
    /// Surface forms to store as aliases: trimmed, non-empty, de-duplicated,
    /// and never the entity's own name (that is the canonical anchor).
    fn clean_aliases(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for a in &self.aliases {
            let a = a.trim();
            if a.is_empty() || a.eq_ignore_ascii_case(self.name.trim()) {
                continue;
            }
            if !out.iter().any(|e: &String| e.eq_ignore_ascii_case(a)) {
                out.push(a.to_string());
            }
        }
        out
    }
}

fn default_entity_kind() -> String {
    "concept".into()
}

/// Largest structured payload we keep on a memory. `detail_md` is evidence for
/// one claim, not a document: a model that dumps half the transcript into it is
/// misusing the field, and an unbounded body would bloat every page that cites
/// the memory. Truncated (never dropped) — a clipped code block still helps.
const MAX_DETAIL_CHARS: usize = 2_000;

/// Hard ceiling on a title. MUST stay <= the `memories_title_len` check in
/// migration 0023: the database rejects a longer one, and a rejected insert
/// loses the whole memory over a cosmetic field.
const MAX_TITLE_CHARS: usize = 120;

/// Sanitize an extractor-proposed title.
///
/// Trims, drops empties, strips a trailing period (it is a label, not a
/// sentence, and models add one anyway), and truncates on a CHARACTER boundary —
/// `&str[..n]` panics mid-codepoint, and this corpus has a Czech slice.
///
/// Truncation is a last resort rather than the norm: the prompt asks for <= 80,
/// so a title long enough to hit 120 is a model ignoring the instruction. Better
/// a clipped label than a dropped memory.
fn clean_title(raw: Option<&str>) -> Option<String> {
    let t = raw?.trim().trim_end_matches('.').trim();
    if t.is_empty() {
        return None;
    }
    let out: String = t.chars().take(MAX_TITLE_CHARS).collect();
    Some(out)
}

/// Sanitize an extractor-proposed `detail_md` (KB-PLAN D3): trim, drop empties,
/// run the SAME secret firewall as `content` (a credential is no less durable
/// for living in a code block — this is the likeliest place for one to hide),
/// and bound the length. Returns `None` when there is nothing worth keeping.
fn clean_detail(raw: Option<&str>) -> Option<String> {
    let t = raw?.trim();
    if t.is_empty() {
        return None;
    }
    let redacted = brainiac_core::redact::redact(t);
    if redacted.trim().is_empty() {
        return None;
    }
    let clipped: String = if redacted.chars().count() > MAX_DETAIL_CHARS {
        redacted.chars().take(MAX_DETAIL_CHARS).collect::<String>() + "\n…"
    } else {
        redacted
    };
    Some(clipped)
}

#[derive(Debug, Deserialize)]
struct ExtractedRelation {
    src: String,
    rel: String,
    dst: String,
}

#[derive(Debug, Default)]
pub struct ExtractStats {
    pub memories_written: usize,
    pub entities_created: Vec<Uuid>,
    pub memory_ids: Vec<Uuid>,
    pub dropped_invalid: usize,
    /// Memories skipped because an identical (source, content) row already
    /// existed — the idempotency guard against a redelivered retry or an
    /// overlap region re-extracted by an adjacent chunk.
    pub deduped: usize,
    /// Chunks the source was split into = number of primary extract calls.
    pub chunks: usize,
    /// First responses that failed to parse and triggered a repair re-prompt.
    pub parse_failures: usize,
    /// Repair re-prompts that recovered a parseable response.
    pub repairs: usize,
    /// Extra sampling passes spent on a chunk that had resolved to ZERO
    /// memories (see [`PASS_TEMPERATURES`]). Cost of the recall floor.
    pub empty_resamples: usize,
    /// Chunks that produced nothing on the first pass and real memories on a
    /// later one — knowledge the old single-pass path silently dropped.
    pub recovered_from_empty: usize,
    /// Chunks served from the extraction cache (migration 0048) — model calls
    /// NOT made. This is the honest denominator for "what did the cache save":
    /// one hit = one call avoided, counted, not asserted.
    pub cache_hits: usize,
    /// Actual model calls made for this source. Zero for a `manual` source —
    /// the verbatim path (F-3) never touches the provider, and the log must
    /// say so rather than implying a call happened.
    pub llm_calls: usize,
    /// Model ref the provider actually reported (from the first chunk's call),
    /// recorded on the pipeline_runs row. `None` only for an empty source.
    pub model_ref: Option<String>,
}

/// Ceiling on the extractor's completion length (tokens). One place so the
/// primary call and the repair re-prompt stay in lockstep, and so the chunk
/// budget below can reason about the input/output split of a context window.
const MAX_EXTRACT_TOKENS: u32 = 4096;

// ── chunking (ARCHITECTURE.md §5 rows 1-2) ──────────────────────────────
//
// The whole raw source used to go to the model in a single call capped at
// MAX_EXTRACT_TOKENS — long agent sessions silently truncated exactly the
// transcripts most likely to contain decisions (the tail). Oversized sources
// are split into overlapping char windows so no region is dropped; the
// (source, content) dedup above collapses the memories an overlap re-extracts.

/// Coarse chars-per-token for English transcripts (~4). Only used to size
/// chunks; exactness doesn't matter because the window carries headroom.
const CHARS_PER_TOKEN: usize = 4;

/// Target source tokens per chunk. With MAX_EXTRACT_TOKENS of completion plus
/// the system prompt, ~3000 source tokens keeps a call comfortably inside a
/// modest (8k) context window.
const CHUNK_TARGET_TOKENS: usize = 3000;

/// A source at or under this many chars is sent whole — no behavior change for
/// the common short session. ~12000 chars.
const MAX_CHUNK_CHARS: usize = CHUNK_TARGET_TOKENS * CHARS_PER_TOKEN;

/// Overlap between consecutive windows so a decision straddling a boundary is
/// seen intact by at least one chunk; dedup collapses the repeat.
const CHUNK_OVERLAP_CHARS: usize = 800;

/// Split `text` into overlapping char windows. UTF-8 safe (windows are cut on
/// char boundaries, never mid-codepoint). Short sources return a single chunk.
fn chunk_source(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }
    let step = MAX_CHUNK_CHARS - CHUNK_OVERLAP_CHARS;
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + MAX_CHUNK_CHARS).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start += step;
    }
    chunks
}

/// Extract the outermost JSON object from provider output that may carry
/// prose or fences around it (string/escape aware).
pub fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// TTL for a kind in days: `BRAINIAC_TTL_DAYS_<KIND>` overrides the
/// core default; `0` disables expiry for that kind (valid_to stays NULL).
fn ttl_days(kind: MemoryKind) -> Option<i64> {
    let days = std::env::var(format!(
        "BRAINIAC_TTL_DAYS_{}",
        kind.as_str().to_uppercase()
    ))
    .ok()
    .and_then(|v| v.trim().parse::<i64>().ok())
    .unwrap_or(i64::from(kind.default_ttl_days()));
    (days > 0).then_some(days)
}

/// Extract the outermost JSON array from provider output (string/escape aware).
/// Real BYOM extractors (Qwen in JSON mode) intermittently DROP the `{memories:…}`
/// wrapper and return a bare array of memories; this recovers it.
pub fn extract_json_array(s: &str) -> Option<&str> {
    let start = s.find('[')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse an extractor response into the typed output, or return the failure
/// reason (used verbatim in the repair re-prompt). Tolerant of the shapes real
/// BYOM providers emit even in JSON mode (found via the `extraction` eval on
/// Qwen, which failed ~half the transcripts before this): the canonical
/// `{"memories":[…]}` object, a bare `[…]` array with the wrapper dropped, and —
/// critically — the case where the outermost `{` is actually the FIRST memory
/// (so an object-only parse would silently yield zero). We try both shapes and
/// keep whichever recovered more memories, so a dropped wrapper can never
/// masquerade as an empty extraction.
fn parse_extraction(text: &str) -> std::result::Result<ExtractionOutput, String> {
    let obj_str = extract_json_object(text);
    // Canonical object shape — but ONLY when the object actually carries a
    // `memories` key. `ExtractionOutput.memories` is #[serde(default)], so a
    // valid-but-wrong object (a refusal/reasoning wrapper like {"refusal":"…"} or
    // {"result":{…}}) would otherwise deserialize to an empty vec and masquerade
    // as a clean 0-extraction — skipping the repair loop and silently dropping a
    // transcript full of knowledge. A genuine empty result is {"memories":[]},
    // which HAS the key, so it still parses here.
    let obj_out = obj_str.and_then(|s| {
        let v: serde_json::Value = serde_json::from_str(s).ok()?;
        v.get("memories")?; // require the key to be present
        serde_json::from_str::<ExtractionOutput>(s).ok()
    });
    let arr_out = extract_json_array(text)
        .and_then(|s| serde_json::from_str::<Vec<ExtractedMemory>>(s).ok())
        .map(|memories| ExtractionOutput { memories });
    match (obj_out, arr_out) {
        (Some(o), Some(a)) => Ok(if a.memories.len() > o.memories.len() {
            a
        } else {
            o
        }),
        (Some(o), None) => Ok(o),
        // A recovered array counts on its own only when it is NON-EMPTY: an empty
        // array pulled out of an ambiguous wrapper ({"status":"ok","data":[]}) is
        // not a confident "0 extraction" — only the canonical {"memories":[]}
        // object shape is. Fall through to a repair instead of dropping the chunk.
        (None, Some(a)) if !a.memories.is_empty() => Ok(a),
        // Nothing parsed as a valid extraction — return the sharpest reason so the
        // repair re-prompt asks for the right shape instead of accepting empty.
        _ => match obj_str {
            None => Err("no JSON object or array in response".to_string()),
            Some(s) => match serde_json::from_str::<serde_json::Value>(s) {
                Ok(v) if v.get("memories").is_none() => {
                    Err("response JSON had no `memories` field".to_string())
                }
                Ok(_) => Err("`memories` was not an array of the expected shape".to_string()),
                Err(e) => Err(e.to_string()),
            },
        },
    }
}

/// Bounded JSON-repair re-prompts after a malformed first response. Real Qwen
/// parse failures are largely STOCHASTIC (the `extraction` eval showed the
/// zero-extraction count swinging 3→6 across identical runs on the same prompt),
/// so a single repair leaves a chunk of recoverable extractions on the floor —
/// a fresh re-ask often just succeeds. Two attempts (three total calls max) is
/// the cost/recall trade; the FINAL attempt also offers the empty-result escape
/// hatch, so a genuine "nothing to extract" (or a model that keeps returning
/// prose) resolves to a clean `{"memories":[]}` — 0 memories, job succeeds —
/// instead of failing the job and clogging the queue. Only a persistent failure
/// past the budget returns Err, which the worker maps onto the queue's
/// attempt-aware path (dead-letter after MAX_ATTEMPTS, never infinite retry).
const MAX_REPAIR_ATTEMPTS: usize = 2;

// ── the empty-extraction floor (recall) ──────────────────────────────────
//
// The measured recall failure is NOT mostly "the model wrote a worse memory
// than gold" — it is "the model returned NOTHING for a transcript that plainly
// contains knowledge". The `extraction` eval saw the zero-extraction count
// swing 3/5/6 across identical runs of 9 transcripts (see the V1 prompt note):
// a third to two thirds of the corpus recalled 0.0, at random, per run.
//
// The single-pass path treats every route to zero as final:
//   * a clean `{"memories":[]}` parses fine, so no repair fires;
//   * the LAST repair attempt actively OFFERS `{"memories":[]}` as the escape
//     hatch, so a model that was merely being lazy is handed a way out;
//   * three prose responses in a row return Err and lose the whole SOURCE.
// All three are indistinguishable from a genuinely contentless chunk, and all
// three are stochastic — which is exactly what makes re-sampling the right
// lever (the repo's own conclusion: robustness/retry, not prompt wording).
//
// So: a substantive chunk that resolves to zero memories is treated as a
// FAILURE and re-sampled, at a rising temperature so the second draw is
// actually a different draw (qwen-max is a MoE model whose routing already
// varies at temperature 0, but a nudge makes the resample meaningful rather
// than hopeful). The first non-empty result wins.
//
// Precision safety — the reason this is not a recall-for-precision trade:
// a resample only ever REPLACES an empty result. It never appends to a
// successful extraction, so it cannot pad output with near-duplicate
// paraphrases of an already-extracted fact (which is how multi-pass schemes
// normally buy recall and lose precision). What it adds are ordinary
// first-class extractions on chunks whose previous contribution was zero
// memories AND zero matches — the only region of the corpus where precision
// has no downside to give.

/// Sampling temperatures, in order, for the passes over one chunk. Pass 0 stays
/// at 0.0 — unchanged behavior for the overwhelmingly common case where the
/// first call yields memories. Later passes only run when the previous one
/// produced nothing at all.
const PASS_TEMPERATURES: [f32; 3] = [0.0, 0.3, 0.6];

/// Hard ceiling on model calls per chunk, across all passes and their repairs.
/// Bounds the worst case explicitly instead of letting passes × repairs
/// multiply: today's worst case is 3 (1 + 2 repairs), this one is 6.
const MAX_LLM_CALLS_PER_CHUNK: usize = 6;

/// A chunk shorter than this is allowed to extract nothing without argument —
/// "ok, thanks" genuinely has no durable knowledge in it, and re-sampling it
/// would spend real tokens to re-confirm an obvious zero. Meridian transcripts
/// run ~1.5–2.5k chars, so this only exempts the trivially short.
const MIN_CHARS_FOR_RESAMPLE: usize = 300;

/// Call accounting for one chunk, threaded through the passes so the numbers
/// survive a pass that ends in `Err` (a pass that burned three calls and failed
/// still cost three calls, and the run log must say so).
#[derive(Debug, Default)]
struct CallBudget {
    calls: usize,
    parse_failures: usize,
    repairs: usize,
}

impl CallBudget {
    fn exhausted(&self) -> bool {
        self.calls >= MAX_LLM_CALLS_PER_CHUNK
    }
}

#[derive(Debug)]
struct LlmExtract {
    output: ExtractionOutput,
    model_ref: String,
    /// The provider's raw response that produced `output`, kept so a successful
    /// extraction can be cached (see [`cache_store`]). `None` for a path that
    /// never called a model — the verbatim ingest, and a cache hit itself.
    text: Option<String>,
}

// ── extraction call cache (migration 0048) ───────────────────────────────
//
// There was no cache or dedupe anywhere on the LLM path; the only dedupe that
// existed (`memory_exists_for_source`) runs AFTER the call, so a redelivered
// job or a re-ingested source paid full price to produce rows that were then
// discarded. The queue makes that routine: 30s backoff, a 300s visibility
// window, SKIP-LOCKED redelivery. Pass 0 is temperature 0.0, so identical input
// genuinely should yield identical output.
//
// The KEY is the whole design — see migrations/0048_extraction_cache.sql. It
// binds the prompt (by fingerprint, not by a version string someone must
// remember to bump), the extract stage's model ref (per-stage overrides exist),
// and the REDACTED chunk. A prompt edit or a model swap empties the cache by
// construction rather than by discipline.

/// Fingerprint of the extraction prompt. Derived from the prompt TEXT, so
/// editing one word invalidates every cached extraction — a version constant
/// would only invalidate them when someone remembered to change it, and the
/// failure mode of forgetting is silent stale output.
fn prompt_fingerprint() -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(EXTRACT_SYSTEM_PROMPT_V1.as_bytes());
    hex16(&digest)
}

fn hex16(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// The cache key: prompt fingerprint + model ref + redacted chunk, sha256.
/// `redacted` must already have passed [`brainiac_core::redact::redact`] — the
/// secret firewall lives UPSTREAM of the key so no raw credential is ever part
/// of a cache row, and so two chunks differing only in a secret share a key
/// (safe, because a secret can never reach a memory body anyway).
fn cache_key(model_ref: &str, prompt_fp: &str, redacted: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    // Length-prefixed so no two different triples can concatenate to the same
    // byte string ("ab"+"c" must not collide with "a"+"bc").
    for part in [prompt_fp, model_ref, redacted] {
        h.update(part.len().to_le_bytes());
        h.update(part.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Look up a cached response and count the hit. RLS-scoped through the caller's
/// connection like every other query here — a cache row belongs to one org.
async fn cache_lookup(conn: &mut PgConnection, org_id: Uuid, key: &str) -> Result<Option<String>> {
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE extraction_cache
            SET hits = hits + 1, last_used_at = now()
          WHERE org_id = $1 AND cache_key = $2
        RETURNING response",
    )
    .bind(org_id)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await
    .context("extraction cache lookup")?;
    Ok(row.map(|r| r.get::<String, _>("response")))
}

/// Remember a response. Stores the REDACTED text — the row must not retain a
/// credential the model echoed out of the transcript, and `content` is redacted
/// downstream anyway, so nothing downstream can tell the difference.
async fn cache_store(
    conn: &mut PgConnection,
    org_id: Uuid,
    key: &str,
    model_ref: &str,
    prompt_fp: &str,
    response: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO extraction_cache
            (org_id, cache_key, response, model_ref, prompt_fingerprint)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (org_id, cache_key) DO NOTHING",
    )
    .bind(org_id)
    .bind(key)
    .bind(brainiac_core::redact::redact(response))
    .bind(model_ref)
    .bind(prompt_fp)
    .execute(&mut *conn)
    .await
    .context("extraction cache store")?;
    Ok(())
}

/// The verbatim path for `manual` sources (F-3): no model, no parse, no way to
/// fail. A `memory_add` sends ONE pre-distilled statement — the exact shape
/// the transcript-tuned extractor is least robust on (the ChainSonar field
/// test measured a 36% hard-failure rate on real qwen-max, every success
/// needing a repair pass). But a distilled statement does not need distilling:
/// the statement IS the memory. So it becomes one [`ExtractedMemory`]
/// directly — kind from the author's hint (default `fact`), the hinted entity
/// names carried through — and flows into the SAME firewall, dedup, insert,
/// and entity-resolution machinery as an LLM extraction. What this path gives
/// up, deliberately: free-text entity mining beyond the hints, and multi-fact
/// splitting of paragraph pastes (the tool contract is one statement). What it
/// buys: a contribution that cannot be lost to a parse failure, zero LLM cost,
/// and instant, deterministic ingestion.
fn extract_verbatim(raw_text: &str) -> LlmExtract {
    let decoded = crate::manual::decode_manual_source(raw_text);
    let kind = decoded.kind_hint.unwrap_or(brainiac_core::MemoryKind::Fact);
    LlmExtract {
        output: ExtractionOutput {
            memories: vec![ExtractedMemory {
                kind: kind.as_str().to_string(),
                title: None,
                content: decoded.content,
                lifecycle: None,
                detail_md: None,
                visibility: None,
                // No extractor ran, so there is no extraction confidence to
                // report — None, not a flattering constant. Policy treats an
                // absent confidence conservatively, which is right: the gate
                // still reviews it like any other raw memory.
                confidence: None,
                entities: decoded
                    .entities
                    .into_iter()
                    .map(|name| ExtractedEntity {
                        name,
                        kind: default_entity_kind(),
                        aliases: Vec::new(),
                    })
                    .collect(),
                relations: Vec::new(),
            }],
        },
        model_ref: "verbatim:manual".to_string(),
        text: None,
    }
}

async fn extract_once(
    provider: &dyn ChatProvider,
    user: &str,
    temperature: f32,
    budget: &mut CallBudget,
) -> Result<LlmExtract> {
    budget.calls += 1;
    let resp = provider
        .complete(&ChatRequest {
            system: EXTRACT_SYSTEM_PROMPT_V1.to_string(),
            user: user.to_string(),
            json_mode: true,
            max_tokens: MAX_EXTRACT_TOKENS,
            temperature,
        })
        .await
        .context("extract LLM call")?;

    if let Ok(output) = parse_extraction(&resp.text) {
        return Ok(LlmExtract {
            output,
            model_ref: resp.model_ref,
            text: Some(resp.text),
        });
    }
    budget.parse_failures += 1;

    // Retry loop: re-ask up to MAX_REPAIR_ATTEMPTS times, each echoing the parse
    // error and the bad output. A fresh sample clears most stochastic failures.
    let mut last_err = parse_extraction(&resp.text).err().unwrap_or_default();
    let mut last_text = resp.text;
    for attempt in 1..=MAX_REPAIR_ATTEMPTS {
        if budget.exhausted() {
            break;
        }
        let escape_hatch = if attempt == MAX_REPAIR_ATTEMPTS {
            " If there is genuinely no durable knowledge to extract, respond with \
             exactly {\"memories\":[]} — but NEVER respond with prose or an explanation."
        } else {
            ""
        };
        let repair_user = format!(
            "Your previous response could not be parsed as JSON ({last_err}).\n\
             Here is the exact response that failed:\n\n{last_text}\n\n\
             Return ONLY the corrected JSON object matching the required schema — \
             no prose, no code fences, nothing else.{escape_hatch}"
        );
        budget.calls += 1;
        let repaired = provider
            .complete(&ChatRequest {
                system: EXTRACT_SYSTEM_PROMPT_V1.to_string(),
                user: repair_user,
                json_mode: true,
                max_tokens: MAX_EXTRACT_TOKENS,
                temperature,
            })
            .await
            .context("extract repair LLM call")?;
        match parse_extraction(&repaired.text) {
            Ok(output) => {
                budget.repairs += 1;
                return Ok(LlmExtract {
                    output,
                    model_ref: repaired.model_ref,
                    text: Some(repaired.text),
                });
            }
            Err(e) => {
                last_err = e;
                last_text = repaired.text;
            }
        }
    }
    // The error alone is not actionable — "not an array of the expected shape"
    // names a schema, not the thing the model actually said. Log a REDACTED,
    // bounded snippet of the last bad response (same firewall as `content`; the
    // response quotes the transcript and could carry a credential) so the next
    // parse gap is diagnosable without re-running a paid eval to reproduce it.
    tracing::warn!(
        reason = %last_err,
        snippet = %brainiac_core::redact::redact(&last_text.chars().take(400).collect::<String>()),
        "extractor response unparseable after repairs"
    );
    Err(anyhow::anyhow!(
        "extractor output unparseable after {MAX_REPAIR_ATTEMPTS} repairs: {last_err}"
    ))
}

/// Should a zero-memory result on this chunk be believed, or re-sampled?
/// Only substantive chunks earn a resample; see [`MIN_CHARS_FOR_RESAMPLE`].
fn worth_resampling(chunk: &str) -> bool {
    chunk.trim().chars().count() >= MIN_CHARS_FOR_RESAMPLE
}

/// One chunk → memories, with the empty result treated as a retryable failure.
///
/// Pass 0 is exactly today's call. Later passes run ONLY when everything so far
/// produced zero memories — whether by a clean `{"memories":[]}`, by the final
/// repair's escape hatch, or by a pass that exhausted its repairs and errored
/// (a hard failure used to lose the entire source; here it is just a pass that
/// did not work, and the next temperature gets a turn).
///
/// Returns the first NON-EMPTY extraction. If every pass came back empty, the
/// empty result is returned as a success — a genuinely contentless chunk must
/// resolve cleanly, never fail the job and clog the queue. Only a chunk where
/// every pass ERRORED propagates the error, preserving the worker's
/// attempt-aware dead-letter path.
async fn extract_chunk(
    provider: &dyn ChatProvider,
    chunk: &str,
    budget: &mut CallBudget,
    resamples: &mut usize,
) -> Result<LlmExtract> {
    let mut empty: Option<LlmExtract> = None;
    let mut last_err: Option<anyhow::Error> = None;

    for (pass, &temperature) in PASS_TEMPERATURES.iter().enumerate() {
        if pass > 0 {
            *resamples += 1;
        }
        match extract_once(provider, chunk, temperature, budget).await {
            Ok(call) if !call.output.memories.is_empty() => return Ok(call),
            // Empty but well-formed: keep it as the fallback, keep looking.
            Ok(call) => empty = Some(call),
            Err(e) => last_err = Some(e),
        }
        if !worth_resampling(chunk) || budget.exhausted() {
            break;
        }
    }
    match (empty, last_err) {
        (Some(call), _) => Ok(call),
        (None, Some(e)) => Err(e),
        // Unreachable: PASS_TEMPERATURES is non-empty, so one arm always sets.
        (None, None) => Err(anyhow::anyhow!("extractor made no attempt")),
    }
}

/// Idempotency guard: has a memory with this exact content already been
/// written for this source? Scoped to (org, source) via the provenance link
/// (memories carry no source_id column; provenance.source_id is the join).
/// Exact-content equality is the cheapest correct dedup here — no schema
/// change, no hash column — and it collapses both a redelivered job's retry
/// and an overlap region re-extracted by an adjacent chunk. Relies on the
/// worker read scope (org+team) to see the just-written rows under RLS.
async fn memory_exists_for_source(
    conn: &mut PgConnection,
    org_id: Uuid,
    source_id: Uuid,
    content: &str,
) -> Result<bool> {
    let existing = sqlx::query(
        "SELECT 1 FROM memories m
         JOIN provenance p ON p.id = m.provenance_id
         WHERE m.org_id = $1 AND p.source_id = $2 AND m.content = $3
         LIMIT 1",
    )
    .bind(org_id)
    .bind(source_id)
    .bind(content)
    .fetch_optional(conn)
    .await?;
    Ok(existing.is_some())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_extract(
    conn: &mut PgConnection,
    provider: &dyn ChatProvider,
    embedder: &dyn Embedder,
    embedding_version: i32,
    org_id: Uuid,
    team_id: Option<Uuid>,
    source_id: Uuid,
    // The source's `kind` column. `manual` (a memory_add — one pre-distilled
    // statement) takes the deterministic verbatim path; everything else
    // (`session_transcript`, `doc`, …) is prose that genuinely needs the
    // model to distill it.
    source_kind: &str,
    raw_text: &str,
    // Direction 2: the run this extraction belongs to. Stamped onto the single
    // provenance row so every memory (and entity) written here links back to
    // the pipeline_runs record via provenance.pipeline_run_id.
    pipeline_run_id: Option<Uuid>,
    // PROJECT-PLAN PR0: the source row's project, stamped onto every memory
    // extracted from it. None = org-shared.
    project_id: Option<Uuid>,
) -> Result<ExtractStats> {
    let verbatim = source_kind == "manual";
    // Chunk oversized sources so a long session's tail isn't truncated. One
    // provenance row per source still (stamped from the first chunk's model
    // ref), every chunk's memories linked to it; dedup collapses overlaps.
    // A manual source is one bounded statement — never chunked (chunking a
    // statement could split it across the overlap and duplicate it).
    let chunks = if verbatim {
        vec![raw_text.to_string()]
    } else {
        chunk_source(raw_text)
    };
    let mut stats = ExtractStats {
        chunks: chunks.len(),
        ..Default::default()
    };
    let mut provenance_id: Option<Uuid> = None;

    for chunk in &chunks {
        // Manual sources bypass the model entirely (F-3: nothing to distill,
        // nothing to mis-parse); everything else pays one BYOM call per chunk,
        // each with a bounded JSON-repair re-prompt.
        let call = if verbatim {
            extract_verbatim(chunk)
        } else {
            // Cache first (0048): the redelivered job, the re-ingested source,
            // and the 800-char overlap every adjacent window re-reads all land
            // here. Key on the REDACTED chunk so the secret firewall sits
            // upstream of the cache, never bypassed by it.
            let redacted = brainiac_core::redact::redact(chunk);
            let model_ref = provider.model_ref();
            let prompt_fp = prompt_fingerprint();
            let key = cache_key(&model_ref, &prompt_fp, &redacted);
            // A cached response is re-parsed by the SAME parser the live path
            // uses, so a hit and a miss can never diverge; a row that no longer
            // parses (or parses to nothing) is ignored rather than trusted.
            let cached = match cache_lookup(conn, org_id, &key).await? {
                Some(resp) => parse_extraction(&resp)
                    .ok()
                    .filter(|o| !o.memories.is_empty()),
                None => None,
            };
            match cached {
                Some(output) => {
                    stats.cache_hits += 1;
                    LlmExtract {
                        output,
                        model_ref,
                        text: None,
                    }
                }
                None => {
                    let mut budget = CallBudget::default();
                    let mut resamples = 0usize;
                    let out = extract_chunk(provider, chunk, &mut budget, &mut resamples).await;
                    // Account for what was actually spent even when the chunk
                    // failed — an errored chunk still burned its calls.
                    stats.llm_calls += budget.calls;
                    stats.parse_failures += budget.parse_failures;
                    stats.repairs += budget.repairs;
                    stats.empty_resamples += resamples;
                    let out = out?;
                    if resamples > 0 && !out.output.memories.is_empty() {
                        stats.recovered_from_empty += 1;
                    }
                    // Only a NON-EMPTY extraction is worth remembering. Caching
                    // a zero would freeze a stochastic failure into a permanent
                    // one and defeat the resample floor built to catch it.
                    if let Some(text) = out.text.as_deref() {
                        if !out.output.memories.is_empty() {
                            cache_store(conn, org_id, &key, &out.model_ref, &prompt_fp, text)
                                .await?;
                        }
                    }
                    out
                }
            }
        };

        // Lazily create the single provenance row on the first chunk, using
        // the model ref the provider actually reported, and stamp the run id so
        // memories link back to their pipeline_runs record.
        if provenance_id.is_none() {
            stats.model_ref = Some(call.model_ref.clone());
            let pid = Uuid::new_v4();
            brainiac_store::governance::insert_provenance(
                conn,
                pid,
                org_id,
                ActorKind::Pipeline,
                "extract-worker",
                Some(&call.model_ref),
                Some(source_id),
                pipeline_run_id,
            )
            .await?;
            provenance_id = Some(pid);
        }
        let provenance_id = provenance_id.expect("provenance set above");

        for m in call.output.memories {
            // Validation firewall: invalid kinds/empty content are dropped and
            // counted, never written.
            let Some(kind) = MemoryKind::parse(&m.kind) else {
                stats.dropped_invalid += 1;
                continue;
            };
            // Secret firewall (H4): a credential the model lifted verbatim out of
            // the transcript must never become a durable, RLS-shared memory body.
            // Redact before the content is stored, deduped, or embedded.
            let content = brainiac_core::redact::redact(m.content.trim());
            if content.is_empty() {
                stats.dropped_invalid += 1;
                continue;
            }
            // Idempotency: skip a memory this source already produced (a
            // redelivered retry, or an overlap region seen by an adjacent
            // chunk).
            if memory_exists_for_source(conn, org_id, source_id, &content).await? {
                stats.deduped += 1;
                continue;
            }
            let visibility = m
                .visibility
                .as_deref()
                .and_then(Visibility::parse)
                .unwrap_or(Visibility::Team);
            let confidence = m.confidence.map(|c| c.clamp(0.0, 1.0));
            // Facet firewall (KB-PLAN D2/D3): both facets are advisory. An
            // unknown lifecycle coerces to `shipped` rather than dropping the
            // memory — losing a real learning over a mislabeled facet would be
            // the exact recall failure the UAT flagged as the top threat.
            let lifecycle = m
                .lifecycle
                .as_deref()
                .and_then(Lifecycle::parse)
                .unwrap_or_default();
            let detail_md = clean_detail(m.detail_md.as_deref());

            let memory_id = Uuid::new_v4();
            // Freshness lifecycle: stamp the validity window at extraction so
            // knowledge expires from retrieval (temporal::valid_at) instead of
            // staying presumed-true forever; /v1/memories/expiring surfaces rows
            // approaching the boundary for re-verification.
            let now = chrono::Utc::now();
            brainiac_store::memories::insert(
                conn,
                &brainiac_store::memories::NewMemory {
                    id: memory_id,
                    org_id,
                    team_id,
                    owner_user_id: None,
                    visibility,
                    status: MemoryStatus::Raw,
                    kind,
                    title: clean_title(m.title.as_deref()),
                    content: content.clone(),
                    lifecycle,
                    detail_md,
                    language: "en".into(),
                    valid_from: Some(now),
                    valid_to: ttl_days(kind).map(|d| now + chrono::Duration::days(d)),
                    superseded_by: None,
                    confidence,
                    provenance_id: Some(provenance_id),
                    project_id,
                },
            )
            .await?;

            // Entities: get-or-create within the source's team scope.
            let mut name_to_id: std::collections::HashMap<String, Uuid> =
                std::collections::HashMap::new();
            for e in &m.entities {
                let id =
                    match brainiac_store::entities::find_by_name(conn, org_id, team_id, &e.name)
                        .await?
                    {
                        Some(id) => id,
                        None => {
                            let id = Uuid::new_v4();
                            // Entity-kind firewall: coerce an unknown/typo'd
                            // kind to the `concept` default rather than storing
                            // the raw typo (dropping a whole memory over one
                            // mislabeled entity would be too harsh — the kind is
                            // advisory metadata, and a canonical value keeps the
                            // graph queryable).
                            let entity_kind = EntityKind::parse(&e.kind).unwrap_or_default();
                            brainiac_store::entities::insert_entity(
                                conn,
                                id,
                                org_id,
                                team_id,
                                &e.name,
                                entity_kind.as_str(),
                                &e.clean_aliases(),
                                Some(provenance_id),
                            )
                            .await?;
                            stats.entities_created.push(id);
                            id
                        }
                    };
                name_to_id.insert(e.name.to_lowercase(), id);
                brainiac_store::memories::link_entity(conn, memory_id, id).await?;
            }
            for r in &m.relations {
                let (Some(src), Some(dst)) = (
                    name_to_id.get(&r.src.to_lowercase()),
                    name_to_id.get(&r.dst.to_lowercase()),
                ) else {
                    stats.dropped_invalid += 1;
                    continue; // relation names must reference listed entities
                };
                // Relation firewall: unlike entity kind, an unknown relation is
                // DROPPED (counted), not coerced — a bogus edge invents graph
                // structure (wrong depends_on/owns), which is worse than an
                // untyped node. Only the five canonical relations reach the DB.
                let Some(relation) = EdgeRelation::parse(&r.rel) else {
                    stats.dropped_invalid += 1;
                    continue;
                };
                brainiac_store::entities::insert_edge(
                    conn,
                    Uuid::new_v4(),
                    org_id,
                    *src,
                    *dst,
                    relation.as_str(),
                    Some(memory_id),
                )
                .await?;
            }

            // Embed stage (local model, no queue round-trip needed in v0).
            brainiac_store::memories::upsert_embedding(
                conn,
                memory_id,
                embedding_version,
                &embedder.embed(&content).await?,
            )
            .await?;

            stats.memory_ids.push(memory_id);
            stats.memories_written += 1;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── titles (migration 0023) ─────────────────────────────────────────────

    #[test]
    fn title_is_optional_and_absence_is_not_a_failure() {
        // The shape every V1-prompt provider still emits, and the shape a
        // provider that ignores the instruction emits. Neither may cost us the
        // memory — the archive falls back to `content`.
        let raw = r#"{"memories":[{"kind":"fact","content":"the ledger holds the authoritative balance"}]}"#;
        let out: ExtractionOutput = serde_json::from_str(raw).expect("title-less output parses");
        assert_eq!(out.memories.len(), 1);
        assert_eq!(clean_title(out.memories[0].title.as_deref()), None);
    }

    #[test]
    fn title_loses_its_trailing_period_and_padding() {
        // Models write a title like a sentence. It is a label.
        assert_eq!(
            clean_title(Some("  psp-gateway retry cap.  ")),
            Some("psp-gateway retry cap".to_string())
        );
        assert_eq!(clean_title(Some("   ")), None);
        assert_eq!(clean_title(Some("")), None);
    }

    #[test]
    fn title_is_clamped_to_the_database_constraint() {
        // migration 0023 rejects > 120 chars, and a rejected INSERT loses the
        // whole memory over a cosmetic field. The clamp is the boundary's job.
        let long = "a".repeat(400);
        let got = clean_title(Some(&long)).expect("a non-empty title survives the clamp");
        assert_eq!(got.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn title_truncates_on_a_character_boundary() {
        // The corpus has a Czech slice, and `&s[..n]` panics mid-codepoint.
        // Truncation must count CHARACTERS, not bytes.
        let czech = "ě".repeat(200);
        let got = clean_title(Some(&czech)).expect("a non-empty title survives the clamp");
        assert_eq!(got.chars().count(), MAX_TITLE_CHARS);
        assert!(got.chars().all(|c| c == 'ě'));
    }

    // ── KB0 facets (KB-PLAN D2/D3) ──────────────────────────────────────────

    #[test]
    fn facets_are_optional_and_default_safely() {
        // The overwhelmingly common shape: a V1-style memory with neither
        // facet. It must still parse, and must NOT be treated as unshipped.
        let raw = r#"{"memories":[{"kind":"fact","content":"MSK is the cluster"}]}"#;
        let out = parse_extraction(raw).expect("parses");
        let m = &out.memories[0];
        assert!(m.lifecycle.is_none() && m.detail_md.is_none());
        assert_eq!(
            m.lifecycle
                .as_deref()
                .and_then(Lifecycle::parse)
                .unwrap_or_default(),
            Lifecycle::Shipped
        );
    }

    #[test]
    fn unknown_lifecycle_coerces_to_shipped_and_keeps_the_memory() {
        // A mislabeled facet must never cost us the memory — dropping real
        // learnings over advisory metadata is the recall failure UAT flagged.
        let raw = r#"{"memories":[{"kind":"fact","content":"x","lifecycle":"someday"}]}"#;
        let out = parse_extraction(raw).expect("parses");
        assert_eq!(out.memories.len(), 1);
        assert_eq!(
            out.memories[0]
                .lifecycle
                .as_deref()
                .and_then(Lifecycle::parse)
                .unwrap_or_default(),
            Lifecycle::Shipped
        );
    }

    #[test]
    fn lifecycle_parses_when_the_transcript_was_explicit() {
        let raw =
            r#"{"memories":[{"kind":"decision","content":"adopt kafka","lifecycle":"in_flight"}]}"#;
        let out = parse_extraction(raw).expect("parses");
        assert_eq!(
            out.memories[0]
                .lifecycle
                .as_deref()
                .and_then(Lifecycle::parse),
            Some(Lifecycle::InFlight)
        );
    }

    #[test]
    fn detail_md_is_redacted_like_content() {
        // The likeliest hiding place for a credential is the code block, not
        // the prose. Same secret firewall, or the field is a leak vector.
        let detail = clean_detail(Some(
            "```\nexport AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\n```",
        ))
        .expect("kept");
        assert!(
            !detail.contains("AKIAIOSFODNN7EXAMPLE"),
            "secret survived into detail_md: {detail}"
        );
        // The surrounding artifact still survives — we redact, not discard.
        assert!(detail.contains("AWS_ACCESS_KEY_ID"));
    }

    #[test]
    fn detail_md_drops_empties_and_bounds_length() {
        assert!(clean_detail(None).is_none());
        assert!(clean_detail(Some("   \n ")).is_none());
        let huge = "x".repeat(MAX_DETAIL_CHARS * 2);
        let clipped = clean_detail(Some(&huge)).expect("kept");
        assert!(clipped.chars().count() <= MAX_DETAIL_CHARS + 2);
    }

    #[test]
    fn json_extraction_tolerates_fences() {
        let raw = "Sure! ```json\n{\"memories\":[]}\n```";
        assert_eq!(extract_json_object(raw), Some("{\"memories\":[]}"));
    }

    #[test]
    fn braces_inside_strings_do_not_confuse() {
        let raw = "{\"memories\":[{\"kind\":\"fact\",\"content\":\"a { b } c\"}]}";
        assert_eq!(extract_json_object(raw), Some(raw));
    }

    #[test]
    fn tolerates_json_encoded_array_fields() {
        // Real BYOM output (Qwen) double-encodes a nested array as a string.
        // Both the native-array and the stringified form must parse to the same
        // structure, and neither may stall the ingest job.
        let native = r#"{"memories":[{"kind":"pitfall","content":"x",
            "entities":[{"name":"ledger-service","kind":"service","aliases":[]}]}]}"#;
        let stringy = r#"{"memories":[{"kind":"pitfall","content":"x",
            "entities":"[{\"name\":\"ledger-service\",\"kind\":\"service\",\"aliases\":[]}]"}]}"#;
        let a = parse_extraction(native).expect("native parses");
        let b = parse_extraction(stringy).expect("json-encoded-string array parses");
        assert_eq!(a.memories.len(), 1);
        assert_eq!(b.memories.len(), 1);
        assert_eq!(b.memories[0].entities.len(), 1);
        assert_eq!(b.memories[0].entities[0].name, "ledger-service");
    }

    #[test]
    fn recovers_bare_array_and_null_aliases() {
        // Real Qwen (JSON mode) intermittently drops the {memories:…} wrapper and
        // returns a bare array — and the FIRST element's `{` would otherwise be
        // grabbed as an empty ExtractionOutput (silent zero). Both memories must
        // survive, and a null `aliases` must not fail the whole parse.
        let bare = r#"[
            {"kind":"pitfall","content":"a","entities":[{"name":"x","aliases":null}]},
            {"kind":"decision","content":"b"}
        ]"#;
        let out = parse_extraction(bare).expect("bare array recovered");
        assert_eq!(
            out.memories.len(),
            2,
            "dropped-wrapper array must not read as empty"
        );
        assert_eq!(out.memories[0].entities[0].aliases.len(), 0);

        // Prose-wrapped bare array (the model chatting around the JSON).
        let noisy = "Here you go:\n[{\"kind\":\"fact\",\"content\":\"c\"}]\nHope that helps!";
        assert_eq!(parse_extraction(noisy).expect("noisy").memories.len(), 1);
    }

    #[test]
    fn parse_extraction_reports_failure_reason() {
        assert!(parse_extraction("not json at all").is_err());
        assert!(parse_extraction(r#"{"memories":[]}"#).is_ok());
    }

    #[test]
    fn valid_but_wrong_object_is_a_failure_not_a_silent_empty() {
        // A well-formed JSON object with no `memories` key (a refusal or a
        // reasoning/status wrapper) must drive the repair loop, NOT deserialize to
        // an empty extraction and drop the transcript. A genuine empty keeps its
        // key and still parses.
        for wrapper in [
            r#"{"refusal":"I won't do that"}"#,
            r#"{"result":{"memories":[]}}"#,
            r#"{"status":"ok","data":[]}"#,
        ] {
            let err = parse_extraction(wrapper).expect_err("wrapper must fail to parse");
            assert!(
                err.contains("memories"),
                "reason should name the missing field, got: {err}"
            );
        }
        assert!(
            parse_extraction(r#"{"memories":[]}"#).is_ok(),
            "real empty still ok"
        );
    }

    #[test]
    fn short_source_is_a_single_chunk() {
        let chunks = chunk_source("a short transcript");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "a short transcript");
    }

    #[test]
    fn long_source_splits_into_overlapping_chunks() {
        // Oversized source splits into contiguous windows that cover the whole
        // text (no dropped region), stepping by MAX_CHUNK_CHARS - overlap.
        let text: String = std::iter::repeat_n('x', MAX_CHUNK_CHARS + 1000).collect();
        let chunks = chunk_source(&text);
        assert!(
            chunks.len() >= 2,
            "oversized source splits: {}",
            chunks.len()
        );
        assert_eq!(chunks[0].chars().count(), MAX_CHUNK_CHARS);
        // Last window ends exactly at the true end of the source.
        let step = MAX_CHUNK_CHARS - CHUNK_OVERLAP_CHARS;
        assert_eq!(
            (chunks.len() - 1) * step + chunks.last().expect("chunk").chars().count(),
            text.chars().count()
        );
    }

    #[test]
    fn chunking_is_utf8_boundary_safe() {
        // Multibyte chars around the cut must not panic or corrupt: windows
        // are cut on char boundaries, so every chunk is valid UTF-8.
        let text: String = "é😀🚀".chars().cycle().take(MAX_CHUNK_CHARS + 50).collect();
        let chunks = chunk_source(&text);
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].chars().count(), MAX_CHUNK_CHARS);
    }

    #[test]
    fn aliases_parse_and_clean() {
        // Parser captures the aliases array; cleaning trims, drops blanks and
        // any form equal to the name, and de-duplicates case-insensitively.
        let e: ExtractedEntity = serde_json::from_str(
            r#"{"name":"psp-gateway","kind":"service",
                "aliases":["PSP"," psp-gateway ","psp","PSP",""]}"#,
        )
        .expect("parse");
        assert_eq!(e.clean_aliases(), vec!["PSP".to_string()]);
    }

    #[test]
    fn entity_without_aliases_defaults_empty() {
        let e: ExtractedEntity =
            serde_json::from_str(r#"{"name":"kafka","kind":"tech"}"#).expect("parse");
        assert!(e.clean_aliases().is_empty());
    }

    // ── the extraction cache key (migration 0048) ───────────────────────────

    #[test]
    fn the_cache_key_binds_the_prompt_the_model_and_the_content() {
        // A cache that outlives a prompt or a model change is worse than no
        // cache: it serves last month's extraction under this month's prompt
        // and nothing in the output ever says so. All three components must
        // move the key.
        let base = cache_key("qwen:qwen-max", "fp1", "the transcript");
        assert_ne!(base, cache_key("qwen:qwen-plus", "fp1", "the transcript"));
        assert_ne!(base, cache_key("qwen:qwen-max", "fp2", "the transcript"));
        assert_ne!(
            base,
            cache_key("qwen:qwen-max", "fp1", "another transcript")
        );
        assert_eq!(base, cache_key("qwen:qwen-max", "fp1", "the transcript"));
    }

    #[test]
    fn the_prompt_fingerprint_is_derived_from_the_prompt_itself() {
        // Not a version constant someone has to remember to bump — the failure
        // mode of forgetting is silent stale output, which is the one failure a
        // cache must not have.
        let fp = prompt_fingerprint();
        assert_eq!(fp.len(), 16);
        let other: String = {
            use sha2::{Digest, Sha256};
            hex16(&Sha256::digest(
                format!("{EXTRACT_SYSTEM_PROMPT_V1} one more word").as_bytes(),
            ))
        };
        assert_ne!(fp, other, "editing the prompt must change the fingerprint");
    }

    #[test]
    fn the_cache_key_cannot_be_confused_by_concatenation() {
        // Without length prefixes, ("ab","c",x) and ("a","bc",x) would hash to
        // the same bytes — a cross-model cache hit produced by nothing more
        // than a naming coincidence.
        assert_ne!(cache_key("ab", "c", "t"), cache_key("a", "bc", "t"));
    }

    // ── what a partly broken array must NOT do (measured) ───────────

    #[test]
    fn a_partly_broken_array_drives_a_repair_instead_of_settling_for_a_fragment() {
        // The tempting "fix" for the biggest observed parse failure (~2 of 9
        // transcripts per run died on "`memories` was not an array of the
        // expected shape") is to keep the elements that DO deserialize. Both
        // forms of that were built and measured on real qwen-max, and both
        // LOST: salvaging inside the deserializer scored recall 0.095 /
        // precision 0.500, and salvaging as a post-repair last resort scored
        // 0.333 / 0.819 — against 0.349 / 0.897 for leaving the array strict.
        // The reason is mechanical: a rejected array fires the repair
        // re-prompt, and the RE-ASK comes back complete, while a fragment banks
        // one fact and discards the other three. This assertion guards that
        // finding so the idea is not re-implemented on intuition.
        let partly_broken = r#"{"memories":[
            {"kind":"decision","content":"psp-gateway caps retries at three"},
            {"kind":"fact","title":"no content field at all"},
            {"kind":"pitfall","content":"a per-request idempotency key double-charges"}
        ]}"#;
        assert!(
            parse_extraction(partly_broken).is_err(),
            "a partly broken array must drive a repair, not settle for a fragment"
        );
    }

    // ── the empty-extraction floor (recall robustness) ──────────────────────

    use brainiac_gateway::MockProvider;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A chunk long enough to earn a resample (Meridian transcripts are ~1.5–2.5k).
    fn substantive_chunk() -> String {
        "engineer: we capped psp-gateway retries at three and moved the idempotency \
         key onto the charge id, because the double-charge incident traced back to \
         a retry storm with a per-request key. "
            .repeat(4)
    }

    /// Scripted provider: hands back `replies[i]` for the i-th call, then the
    /// last reply forever. Counts calls so the cost bound is testable.
    fn scripted(replies: Vec<&'static str>) -> (MockProvider, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let p = MockProvider::new(move |_| {
            let i = seen.fetch_add(1, Ordering::SeqCst);
            replies[i.min(replies.len() - 1)].to_string()
        });
        (p, calls)
    }

    const EMPTY: &str = r#"{"memories":[]}"#;
    const ONE: &str = r#"{"memories":[{"kind":"pitfall","content":"a per-request idempotency key retries into a double charge"}]}"#;

    #[tokio::test]
    async fn a_clean_empty_on_a_substantive_chunk_is_resampled_not_believed() {
        // THE recall bug: `{"memories":[]}` parses perfectly, so the repair loop
        // never fires and a transcript full of knowledge scores 0.0 recall. The
        // eval saw this on 3–6 of 9 transcripts per run, at random.
        let (p, calls) = scripted(vec![EMPTY, ONE]);
        let mut budget = CallBudget::default();
        let mut resamples = 0;
        let got = extract_chunk(&p, &substantive_chunk(), &mut budget, &mut resamples)
            .await
            .expect("chunk extracts");
        assert_eq!(got.output.memories.len(), 1, "resample must recover it");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(resamples, 1);
        assert_eq!(budget.parse_failures, 0, "an empty is not a parse failure");
    }

    #[tokio::test]
    async fn a_pass_that_exhausts_its_repairs_hands_the_chunk_to_the_next_pass() {
        // Three prose responses used to return Err and lose the WHOLE SOURCE
        // (every chunk of it). Here it is just a pass that did not work.
        let (p, calls) = scripted(vec![
            "I'm sorry, I can't help.",
            "still prose",
            "more prose",
            ONE,
        ]);
        let mut budget = CallBudget::default();
        let mut resamples = 0;
        let got = extract_chunk(&p, &substantive_chunk(), &mut budget, &mut resamples)
            .await
            .expect("the failed pass must not fail the source");
        assert_eq!(got.output.memories.len(), 1);
        // pass 0 burned 1 + 2 repairs; pass 1's first call succeeded.
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(budget.parse_failures, 1);
    }

    #[tokio::test]
    async fn a_short_chunk_is_allowed_to_extract_nothing() {
        // "ok, thanks" genuinely has no durable knowledge. Re-sampling it would
        // spend real tokens to re-confirm an obvious zero.
        let (p, calls) = scripted(vec![EMPTY, ONE]);
        let mut budget = CallBudget::default();
        let mut resamples = 0;
        let got = extract_chunk(&p, "ok thanks, ship it", &mut budget, &mut resamples)
            .await
            .expect("empty is a success, never a job failure");
        assert!(got.output.memories.is_empty());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "no resample for a short chunk"
        );
        assert_eq!(resamples, 0);
    }

    #[tokio::test]
    async fn a_persistent_empty_resolves_clean_within_the_call_budget() {
        // A chunk that really has nothing must not fail the job and clog the
        // queue — and must not burn calls without end.
        let (p, calls) = scripted(vec![EMPTY]);
        let mut budget = CallBudget::default();
        let mut resamples = 0;
        let got = extract_chunk(&p, &substantive_chunk(), &mut budget, &mut resamples)
            .await
            .expect("a genuine zero resolves cleanly");
        assert!(got.output.memories.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), PASS_TEMPERATURES.len());
        assert!(calls.load(Ordering::SeqCst) <= MAX_LLM_CALLS_PER_CHUNK);
    }

    #[tokio::test]
    async fn the_per_chunk_call_budget_is_hard() {
        // Worst case: every pass burns its full repair allowance. The budget —
        // not passes × repairs — is what bounds the bill.
        let (p, calls) = scripted(vec!["prose, forever"]);
        let mut budget = CallBudget::default();
        let mut resamples = 0;
        let err = extract_chunk(&p, &substantive_chunk(), &mut budget, &mut resamples)
            .await
            .expect_err("nothing ever parsed");
        assert!(err.to_string().contains("unparseable"), "{err}");
        assert_eq!(calls.load(Ordering::SeqCst), MAX_LLM_CALLS_PER_CHUNK);
        assert_eq!(budget.calls, MAX_LLM_CALLS_PER_CHUNK);
    }

    #[tokio::test]
    async fn a_successful_first_pass_costs_exactly_one_call() {
        // The common case must be untouched: no extra token is spent on a chunk
        // that extracted something, and no second pass can pad it with
        // near-duplicate paraphrases (the usual multi-pass precision leak).
        let (p, calls) = scripted(vec![ONE, ONE]);
        let mut budget = CallBudget::default();
        let mut resamples = 0;
        let got = extract_chunk(&p, &substantive_chunk(), &mut budget, &mut resamples)
            .await
            .expect("extracts");
        assert_eq!(got.output.memories.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(resamples, 0);
    }

    #[tokio::test]
    async fn resample_passes_raise_the_temperature_so_the_draw_is_a_new_draw() {
        // Re-asking at temperature 0.0 mostly re-draws the same answer. The
        // escalation is what makes the retry a retry.
        let temps: Arc<std::sync::Mutex<Vec<f32>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = temps.clone();
        let p = MockProvider::new(move |req| {
            seen.lock().expect("lock").push(req.temperature);
            EMPTY.to_string()
        });
        let mut budget = CallBudget::default();
        let mut resamples = 0;
        extract_chunk(&p, &substantive_chunk(), &mut budget, &mut resamples)
            .await
            .expect("resolves empty");
        assert_eq!(*temps.lock().expect("lock"), PASS_TEMPERATURES.to_vec());
        assert_eq!(PASS_TEMPERATURES[0], 0.0, "pass 0 stays deterministic");
    }
}
