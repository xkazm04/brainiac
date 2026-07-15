//! The `extraction` eval profile: score a REAL BYOM extractor's output against
//! the Meridian gold, with SEMANTIC matching.
//!
//! The `pipeline` profile drains transcripts through the worker with a *gold
//! MockProvider* and scores by exact string equality — F1 ≈ 1.0 by construction,
//! a plumbing floor. It explicitly defers "real LLM extraction quality" and "a
//! fuzzier semantic match". This profile is that deferred piece, and it exists
//! because the UAT flywheel run (2026-07-13) showed the real Qwen extractor
//! silently DROPPING a learning — a recall failure invisible to the mock path
//! (the mock emits gold, so it can never miss) and to exact-match scoring (real
//! output paraphrases the gold, so string equality would score every real memory
//! as a miss).
//!
//! Method:
//! - Drain the raw seed transcripts through the ACTUAL worker chain with the
//!   REAL provider the caller passes (Qwen when `QWEN_API_KEY` is set).
//! - For each transcript, match its extracted memories against its
//!   `gold_memories` by EMBEDDING COSINE similarity (greedy 1:1, threshold
//!   [`MATCH_THRESHOLD`]) — a gold fact is *recalled* iff some extracted memory
//!   is semantically close to it, regardless of wording.
//! - Report micro precision/recall/F1, a per-transcript breakdown, and — the
//!   actionable output — the exact list of MISSED gold facts (recall failures)
//!   and the count of spurious extractions (precision failures).
//!
//! Provider-specific by nature: the report is tagged with the provider + embedder
//! and the soft gate refuses a cross-config comparison, exactly like the other
//! profiles. This is a nightly / on-demand per-provider run, not a per-commit
//! gate — it needs a real key and real tokens.

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_gateway::ProviderRouter;
use brainiac_pipeline::worker;
use brainiac_store::{governance, memories, orgs, Store};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

/// Cosine-similarity floor for calling an extracted memory a match to a gold
/// fact. Embedding paraphrases of the same statement land well above this;
/// unrelated statements land well below. Tunable, and surfaced in the report so
/// a run's numbers are always read against the threshold that produced them.
pub const MATCH_THRESHOLD: f64 = 0.70;

#[derive(Debug, Clone, Serialize)]
pub struct ExtractionReport {
    pub fixture_version: String,
    pub embedding_model: String,
    pub provider: String,
    pub match_threshold: f64,
    // ── aggregate quality ────────────────────────────────────────────────
    pub gold_memories: usize,
    pub extracted_memories: usize,
    pub matched: usize,
    pub precision: f64,
    pub recall: f64,
    pub micro_f1: f64,
    /// Extracted memories that matched no gold fact — spurious output (a
    /// precision cost, and a candidate for a hallucinated/off-topic extraction).
    pub spurious: usize,
    // ── the actionable detail ────────────────────────────────────────────
    pub per_transcript: Vec<TranscriptScore>,
    /// Gold facts NO extracted memory covered — the recall failures, the thing
    /// the flywheel run cared about. Ordered worst-transcript first.
    pub misses: Vec<Miss>,
    /// Precision by self-reported confidence band — the measurement that must
    /// exist BEFORE confidence is allowed to gate auto-promotion. Flat bands =
    /// confidence is noise; do not build the lever.
    pub calibration: Vec<CalibrationBucket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptScore {
    pub transcript: String,
    pub team: String,
    pub gold: usize,
    pub extracted: usize,
    pub matched: usize,
    pub recall: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Miss {
    pub transcript: String,
    pub kind: String,
    pub gold: String,
    /// The best similarity any extracted memory reached for this gold fact — how
    /// close the extractor came. A near-miss (just under threshold) is a wording
    /// gap; a far miss (~0) is a genuinely dropped learning.
    pub best_similarity: f64,
}

/// Committed baseline for the extraction SOFT gate (`results/extraction-baseline.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionBaseline {
    pub embedding_model: String,
    pub provider: String,
    pub fixture_version: String,
    pub recall: f64,
    pub precision: f64,
    pub micro_f1: f64,
}

/// Rates may not regress below the committed baseline by more than this.
///
/// Deliberately WIDE (0.15, vs 0.03 elsewhere) because real qwen-max extraction
/// is irreducibly high-variance: across 3 runs of the identical config, recall
/// spanned 0.25–0.54 (mean 0.42) — qwen-max is a large MoE model whose expert
/// routing is non-deterministic even at temperature 0, so a single run is a
/// noisy sample, not a fixed score. At 0.03 the gate would false-alarm on nearly
/// every unlucky draw; 0.15 (~one half-spread) catches a REAL regression — e.g.
/// a parse-hardening revert that collapses recall toward 0.1 — without tripping
/// on normal variance. The right long-term fix is to multi-sample the eval and
/// gate on the mean; until then this floor is honest about the noise.
const RATE_DELTA: f64 = 0.15;

impl ExtractionBaseline {
    pub fn from_report(r: &ExtractionReport) -> Self {
        Self {
            embedding_model: r.embedding_model.clone(),
            provider: r.provider.clone(),
            fixture_version: r.fixture_version.clone(),
            recall: r.recall,
            precision: r.precision,
            micro_f1: r.micro_f1,
        }
    }
}

/// Compare a run against the committed baseline. A cross-config comparison
/// (different embedder OR provider) is refused; then recall/precision/F1 may not
/// regress past [`RATE_DELTA`]. Empty = pass.
pub fn regression_failures(r: &ExtractionReport, b: &ExtractionBaseline) -> Vec<String> {
    let mut f = Vec::new();
    if r.embedding_model != b.embedding_model {
        f.push(format!(
            "embedder mismatch: run={} baseline={} — recalibrate instead of comparing across embedders",
            r.embedding_model, b.embedding_model
        ));
        return f;
    }
    if r.provider != b.provider {
        f.push(format!(
            "provider mismatch: run={} baseline={} — extraction quality is provider-specific; recalibrate instead of comparing across providers",
            r.provider, b.provider
        ));
        return f;
    }
    if r.recall < b.recall - RATE_DELTA {
        f.push(format!(
            "recall regressed: {:.3} < baseline {:.3} − {:.2}",
            r.recall, b.recall, RATE_DELTA
        ));
    }
    if r.precision < b.precision - RATE_DELTA {
        f.push(format!(
            "precision regressed: {:.3} < baseline {:.3} − {:.2}",
            r.precision, b.precision, RATE_DELTA
        ));
    }
    if r.micro_f1 < b.micro_f1 - RATE_DELTA {
        f.push(format!(
            "micro-F1 regressed: {:.3} < baseline {:.3} − {:.2}",
            r.micro_f1, b.micro_f1, RATE_DELTA
        ));
    }
    f
}

// ── multi-sample: the "right long-term fix" the RATE_DELTA comment promised ──

/// N runs of the profile, aggregated. Real qwen-max extraction is irreducibly
/// high-variance (recall spanned 0.25–0.54 across identical configs), so a
/// single run is a noisy sample and the honest single-run band is a wide 0.15.
/// The mean of N runs shrinks that noise by ~1/√N, which lets the gate tighten
/// without false alarms — the difference between "we can't tell" and "we can".
#[derive(Debug, Clone, Serialize)]
pub struct MultiSampleReport {
    pub samples: usize,
    pub embedding_model: String,
    pub provider: String,
    pub fixture_version: String,
    pub mean_recall: f64,
    pub mean_precision: f64,
    pub mean_micro_f1: f64,
    pub min_recall: f64,
    pub max_recall: f64,
    /// The regression band the mean was gated with: max(0.05, 0.15/√N).
    pub gate_delta: f64,
    /// Every run in full — the per-transcript misses are still the actionable
    /// output, and averaging must not hide them.
    pub runs: Vec<ExtractionReport>,
}

/// The mean's regression band: the single-run band shrunk by √N, floored at
/// 0.05 (below that we'd be gating on fixture-sized noise, not the model).
pub fn mean_gate_delta(samples: usize) -> f64 {
    (RATE_DELTA / (samples.max(1) as f64).sqrt()).max(0.05)
}

pub fn aggregate(runs: Vec<ExtractionReport>) -> MultiSampleReport {
    let n = runs.len().max(1) as f64;
    let mean = |f: fn(&ExtractionReport) -> f64| runs.iter().map(f).sum::<f64>() / n;
    MultiSampleReport {
        samples: runs.len(),
        embedding_model: runs
            .first()
            .map(|r| r.embedding_model.clone())
            .unwrap_or_default(),
        provider: runs.first().map(|r| r.provider.clone()).unwrap_or_default(),
        fixture_version: runs
            .first()
            .map(|r| r.fixture_version.clone())
            .unwrap_or_default(),
        mean_recall: mean(|r| r.recall),
        mean_precision: mean(|r| r.precision),
        mean_micro_f1: mean(|r| r.micro_f1),
        min_recall: runs.iter().map(|r| r.recall).fold(f64::INFINITY, f64::min),
        max_recall: runs.iter().map(|r| r.recall).fold(0.0, f64::max),
        gate_delta: mean_gate_delta(runs.len()),
        runs,
    }
}

/// Gate the MEANS against the committed baseline with the √N-tightened band.
/// Same config-mismatch refusal as the single-run gate.
pub fn regression_failures_multi(agg: &MultiSampleReport, b: &ExtractionBaseline) -> Vec<String> {
    let mut f = Vec::new();
    if agg.embedding_model != b.embedding_model || agg.provider != b.provider {
        f.push(format!(
            "config mismatch: run={}/{} baseline={}/{} — recalibrate instead of comparing across configs",
            agg.provider, agg.embedding_model, b.provider, b.embedding_model
        ));
        return f;
    }
    let d = agg.gate_delta;
    if agg.mean_recall < b.recall - d {
        f.push(format!(
            "mean recall over {} samples regressed: {:.3} < baseline {:.3} − {:.3}",
            agg.samples, agg.mean_recall, b.recall, d
        ));
    }
    if agg.mean_precision < b.precision - d {
        f.push(format!(
            "mean precision over {} samples regressed: {:.3} < baseline {:.3} − {:.3}",
            agg.samples, agg.mean_precision, b.precision, d
        ));
    }
    if agg.mean_micro_f1 < b.micro_f1 - d {
        f.push(format!(
            "mean micro-F1 over {} samples regressed: {:.3} < baseline {:.3} − {:.3}",
            agg.samples, agg.mean_micro_f1, b.micro_f1, d
        ));
    }
    f
}

impl ExtractionBaseline {
    /// Recalibrate from a multi-sample aggregate: the baseline stores the MEANS,
    /// so future single runs compare a sample to a mean (wide band) and future
    /// multi-sample runs compare mean to mean (tight band).
    pub fn from_multi(agg: &MultiSampleReport) -> Self {
        Self {
            embedding_model: agg.embedding_model.clone(),
            provider: agg.provider.clone(),
            fixture_version: agg.fixture_version.clone(),
            recall: agg.mean_recall,
            precision: agg.mean_precision,
            micro_f1: agg.mean_micro_f1,
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// Greedy 1:1 matching of extracted → gold by descending similarity above the
/// threshold. Returns (matched_pairs, per-gold best similarity, per-extracted
/// matched flags). One extracted memory can satisfy at most one gold fact and
/// vice versa, so padding the output with paraphrases of one fact cannot
/// inflate recall across many.
fn greedy_match(
    sims: &[Vec<f64>],
    n_extracted: usize,
    n_gold: usize,
) -> (usize, Vec<f64>, Vec<bool>) {
    let mut best_for_gold = vec![0.0f64; n_gold];
    for (row, best) in sims.iter().zip(best_for_gold.iter_mut()) {
        // sims is indexed [gold][extracted]; track the closest extracted reached.
        for &s in row {
            if s > *best {
                *best = s;
            }
        }
    }
    // Flatten candidate pairs, sort desc, assign greedily.
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (g, row) in sims.iter().enumerate() {
        for (e, &s) in row.iter().enumerate() {
            if s >= MATCH_THRESHOLD {
                pairs.push((s, g, e));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut gold_used = vec![false; n_gold];
    let mut ext_used = vec![false; n_extracted];
    let mut matched = 0usize;
    for (_, g, e) in pairs {
        if !gold_used[g] && !ext_used[e] {
            gold_used[g] = true;
            ext_used[e] = true;
            matched += 1;
        }
    }
    (matched, best_for_gold, ext_used)
}

/// Precision within one band of self-reported confidence — the calibration
/// measurement the auto-promotion lever is waiting on. A well-calibrated
/// extractor's high-confidence band should have visibly higher precision than
/// its low band; if the bands are flat, confidence is noise and MUST NOT gate
/// promotion. This report answers that question with data instead of a hunch —
/// the lever itself stays unbuilt until the answer says it would be safe.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationBucket {
    /// `none` | `<0.5` | `0.5–0.7` | `0.7–0.9` | `>=0.9`
    pub bucket: String,
    pub extracted: usize,
    /// How many of them matched a gold fact — precision numerator.
    pub matched: usize,
    pub precision: f64,
}

fn bucket_of(confidence: Option<f64>) -> &'static str {
    match confidence {
        None => "none",
        Some(c) if c < 0.5 => "<0.5",
        Some(c) if c < 0.7 => "0.5–0.7",
        Some(c) if c < 0.9 => "0.7–0.9",
        Some(_) => ">=0.9",
    }
}

const BUCKET_ORDER: [&str; 5] = ["none", "<0.5", "0.5–0.7", "0.7–0.9", ">=0.9"];

/// Run the extraction profile: seed sources, drain them through the REAL worker
/// chain with `providers`, then score extracted memories vs gold by embedding
/// similarity. `admin` is a raw (RLS-bypassing) pool used to read the results.
pub async fn run(
    store: &Store,
    admin: &sqlx::PgPool,
    fx: &Fixtures,
    embedder: &dyn Embedder,
    providers: &ProviderRouter,
) -> Result<ExtractionReport> {
    let org_id = stable_uuid(&fx.org.org);
    let principal = brainiac_pipeline::pipeline_principal(org_id);

    // ── seed identity + raw sources ──────────────────────────────────────
    let mut tx = store.scoped_tx(&principal).await?;
    orgs::upsert_org(&mut tx, org_id, &fx.org.org).await?;
    for t in &fx.org.teams {
        orgs::upsert_team(&mut tx, stable_uuid(&t.id), org_id, &t.name).await?;
    }
    let mut source_ids: Vec<(Uuid, String)> = Vec::new();
    for t in &fx.transcripts {
        let sid = stable_uuid(&t.id);
        let text: String = t
            .turns
            .iter()
            .map(|turn| format!("{}: {}", turn.role, turn.text))
            .collect::<Vec<_>>()
            .join("\n");
        governance::insert_source(
            &mut tx,
            sid,
            org_id,
            Some(stable_uuid(&t.team)),
            "session_transcript",
            &text,
            None,
        )
        .await?;
        source_ids.push((sid, t.id.clone()));
    }
    let embedding_version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await?;
    tx.commit().await?;

    // ── drain through the REAL worker chain ──────────────────────────────
    for (sid, _) in &source_ids {
        worker::enqueue_source(store, org_id, *sid).await?;
    }
    let cfg = worker::WorkerConfig {
        batch: (fx.transcripts.len() as i64).max(1) + 4,
        ..Default::default()
    };
    // A few ticks: extraction enqueues embed/resolve/contradict follow-ons; drain
    // until the queue is quiet so every extracted memory is present.
    for _ in 0..6 {
        let s = worker::tick(store, providers, embedder, embedding_version, &cfg).await?;
        if s.jobs == 0 {
            break;
        }
    }

    // ── read extracted memories, attributed to their source transcript ───
    let mut extracted_by_source: std::collections::HashMap<Uuid, Vec<(String, Option<f64>)>> =
        std::collections::HashMap::new();
    let rows = sqlx::query(
        "SELECT s.id AS source_id, m.content, m.confidence
         FROM memories m
         JOIN provenance p ON p.id = m.provenance_id
         JOIN sources s ON s.id = p.source_id
         WHERE m.org_id = $1",
    )
    .bind(org_id)
    .fetch_all(admin)
    .await
    .context("reading extracted memories")?;
    for r in &rows {
        extracted_by_source
            .entry(r.get::<Uuid, _>("source_id"))
            .or_default()
            .push((
                r.get::<String, _>("content"),
                r.get::<Option<f64>, _>("confidence"),
            ));
    }

    // ── score each transcript by embedding similarity ────────────────────
    let mut per_transcript = Vec::new();
    let mut misses = Vec::new();
    let (mut tot_gold, mut tot_extracted, mut tot_matched) = (0usize, 0usize, 0usize);
    // bucket → (extracted, matched), accumulated across every transcript.
    let mut cal: std::collections::HashMap<&'static str, (usize, usize)> =
        std::collections::HashMap::new();

    for t in &fx.transcripts {
        let sid = stable_uuid(&t.id);
        let golds: Vec<&brainiac_fixtures::schema::TranscriptGoldFx> =
            t.gold_memories.iter().collect();
        let extracted = extracted_by_source.get(&sid).cloned().unwrap_or_default();
        tot_gold += golds.len();
        tot_extracted += extracted.len();

        let (matched, best_for_gold, ext_matched) = if golds.is_empty() || extracted.is_empty() {
            (0, vec![0.0; golds.len()], vec![false; extracted.len()])
        } else {
            let gold_vecs = embedder
                .embed_batch(
                    &golds
                        .iter()
                        .map(|g| g.content_gist.as_str())
                        .collect::<Vec<_>>(),
                )
                .await?;
            let ext_vecs = embedder
                .embed_batch(
                    &extracted
                        .iter()
                        .map(|(s, _)| s.as_str())
                        .collect::<Vec<_>>(),
                )
                .await?;
            // sims[gold][extracted]
            let sims: Vec<Vec<f64>> = gold_vecs
                .iter()
                .map(|gv| ext_vecs.iter().map(|ev| cosine(gv, ev)).collect())
                .collect();
            greedy_match(&sims, extracted.len(), golds.len())
        };
        tot_matched += matched;
        for ((_, confidence), hit) in extracted.iter().zip(ext_matched.iter()) {
            let e = cal.entry(bucket_of(*confidence)).or_default();
            e.0 += 1;
            if *hit {
                e.1 += 1;
            }
        }

        // Record the specific misses (gold facts nothing covered).
        for (g, best) in golds.iter().zip(best_for_gold.iter()) {
            if *best < MATCH_THRESHOLD {
                misses.push(Miss {
                    transcript: t.id.clone(),
                    kind: g.kind.clone(),
                    gold: g.content_gist.clone(),
                    best_similarity: (*best * 1000.0).round() / 1000.0,
                });
            }
        }
        per_transcript.push(TranscriptScore {
            transcript: t.id.clone(),
            team: t.team.clone(),
            gold: golds.len(),
            extracted: extracted.len(),
            matched,
            recall: ratio(matched, golds.len()),
        });
    }

    // Worst transcripts + far misses first — the triage order.
    misses.sort_by(|a, b| {
        a.best_similarity
            .partial_cmp(&b.best_similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    per_transcript.sort_by(|a, b| {
        a.recall
            .partial_cmp(&b.recall)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let precision = ratio(tot_matched, tot_extracted);
    let recall = ratio(tot_matched, tot_gold);
    let micro_f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };

    Ok(ExtractionReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
        // Tag with the EXTRACT stage's model specifically — that is the provider
        // whose quality this profile measures (resolve/contradict may differ).
        provider: providers
            .for_stage(brainiac_gateway::Stage::Extract)
            .model_ref(),
        match_threshold: MATCH_THRESHOLD,
        gold_memories: tot_gold,
        extracted_memories: tot_extracted,
        matched: tot_matched,
        precision,
        recall,
        micro_f1,
        spurious: tot_extracted.saturating_sub(tot_matched),
        per_transcript,
        misses,
        calibration: BUCKET_ORDER
            .iter()
            .filter_map(|b| {
                let (extracted, matched) = *cal.get(b)?;
                Some(CalibrationBucket {
                    bucket: (*b).to_string(),
                    extracted,
                    matched,
                    precision: ratio(matched, extracted),
                })
            })
            .collect(),
    })
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_bands_cover_the_whole_range_with_no_gaps() {
        // A confidence that falls between bands would silently vanish from the
        // calibration table — and a table with holes reads as "measured" when
        // it is not.
        assert_eq!(bucket_of(None), "none");
        assert_eq!(bucket_of(Some(0.0)), "<0.5");
        assert_eq!(bucket_of(Some(0.499)), "<0.5");
        assert_eq!(bucket_of(Some(0.5)), "0.5–0.7");
        assert_eq!(bucket_of(Some(0.699)), "0.5–0.7");
        assert_eq!(bucket_of(Some(0.7)), "0.7–0.9");
        assert_eq!(bucket_of(Some(0.9)), ">=0.9");
        assert_eq!(bucket_of(Some(1.0)), ">=0.9");
        // Every band the bucketer can emit has a place in the report's order.
        for c in [None, Some(0.1), Some(0.6), Some(0.8), Some(0.95)] {
            assert!(BUCKET_ORDER.contains(&bucket_of(c)));
        }
    }

    #[test]
    fn greedy_match_reports_which_extractions_earned_their_keep() {
        // Two golds, three extractions: e0 matches g0, e2 matches g1, e1
        // matches nothing. The per-extraction flags feed the calibration
        // table, so they must name exactly the earners.
        let sims = vec![vec![0.95, 0.10, 0.20], vec![0.15, 0.30, 0.88]];
        let (matched, _, ext_matched) = greedy_match(&sims, 3, 2);
        assert_eq!(matched, 2);
        assert_eq!(ext_matched, vec![true, false, true]);
    }
}
