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
const RATE_DELTA: f64 = 0.03;

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
/// threshold. Returns (matched_pairs, per-gold best similarity). One extracted
/// memory can satisfy at most one gold fact and vice versa, so padding the
/// output with paraphrases of one fact cannot inflate recall across many.
fn greedy_match(sims: &[Vec<f64>], n_extracted: usize, n_gold: usize) -> (usize, Vec<f64>) {
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
    (matched, best_for_gold)
}

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
    let mut extracted_by_source: std::collections::HashMap<Uuid, Vec<String>> =
        std::collections::HashMap::new();
    let rows = sqlx::query(
        "SELECT s.id AS source_id, m.content
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
            .push(r.get::<String, _>("content"));
    }

    // ── score each transcript by embedding similarity ────────────────────
    let mut per_transcript = Vec::new();
    let mut misses = Vec::new();
    let (mut tot_gold, mut tot_extracted, mut tot_matched) = (0usize, 0usize, 0usize);

    for t in &fx.transcripts {
        let sid = stable_uuid(&t.id);
        let golds: Vec<&brainiac_fixtures::schema::TranscriptGoldFx> =
            t.gold_memories.iter().collect();
        let extracted = extracted_by_source.get(&sid).cloned().unwrap_or_default();
        tot_gold += golds.len();
        tot_extracted += extracted.len();

        let (matched, best_for_gold) = if golds.is_empty() || extracted.is_empty() {
            (0, vec![0.0; golds.len()])
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
                .embed_batch(&extracted.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .await?;
            // sims[gold][extracted]
            let sims: Vec<Vec<f64>> = gold_vecs
                .iter()
                .map(|gv| ext_vecs.iter().map(|ev| cosine(gv, ev)).collect())
                .collect();
            greedy_match(&sims, extracted.len(), golds.len())
        };
        tot_matched += matched;

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
    })
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}
