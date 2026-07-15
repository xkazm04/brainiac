//! The `drift` eval profile: calibrate the docs-drift detector BEFORE it is
//! allowed anywhere near production documentation (KB-PLAN follow-up #2,
//! Level 2 cross-documentation intelligence — eval-first by decree).
//!
//! The detector answers one question about a human-authored document: **which
//! of its claims restate a belief the org has already moved past?** Method:
//! split the doc into claims, embed them, and compare each claim against the
//! corpus twice — nearest CURRENT canonical memory vs nearest SUPERSEDED
//! memory. A claim that sits close to a superseded belief and meaningfully
//! closer to it than to any current one is drift, and the proposal is the
//! terminal of that supersession chain: the belief the doc should state now.
//!
//! Three verdicts, deliberately not two:
//! - `drifted` — restates a superseded belief. The actionable output.
//! - `aligned` — matches current canon. Leave the author alone.
//! - `unmatched` — the corpus knows nothing about it. That is a HARVEST
//!   candidate (knowledge living only in the doc), not drift; a detector that
//!   flags everything it does not recognize would teach authors to ignore it.
//!
//! The hard gate is the false alarm: a gold-`aligned` claim flagged as drift
//! is the failure mode that makes the whole feature unshippable — automation
//! that attacks CORRECT documentation is worse than no automation, because
//! authors learn to dismiss it and then miss the real drift. Zero tolerance,
//! same posture as the leak gate.
//!
//! Deliberately DB-free: the instrument under test is claim-vs-corpus
//! classification, and the fixture corpus fits in memory. The production
//! integration (scanning real doc trees, routing findings through the review
//! gate as proposed supersessions) comes only after this gate is green with a
//! real embedder — and that integration, not this profile, is where RLS and
//! visibility enter the picture.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_fixtures::schema::{DriftFile, MemoriesFile};
use serde::{Deserialize, Serialize};

/// Similarity floor for saying a claim "states" a memory — the same floor the
/// extraction profile uses for gold matching, for the same reason: embedded
/// restatements land well above it, unrelated text well below.
pub const MATCH_THRESHOLD: f64 = 0.70;

/// How much closer to the SUPERSEDED belief than to any current one a claim
/// must sit before it is called drift. Superseded beliefs share most of their
/// vocabulary with their replacements ("timeout is 10 seconds" vs "timeout
/// raised to 30 seconds"), so a fresh claim scores high against BOTH; the
/// margin is what keeps the detector's hands off correct docs.
pub const DRIFT_MARGIN: f64 = 0.05;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum ClaimVerdict {
    /// Matches current canon; nothing to do.
    Aligned { memory: String, similarity: f64 },
    /// Restates a superseded belief. `propose` is the terminal of the
    /// supersession chain — what the doc should say instead.
    Drifted {
        stale_memory: String,
        propose: String,
        stale_similarity: f64,
        fresh_similarity: f64,
    },
    /// The corpus knows nothing about this claim — a harvest candidate.
    Unmatched { best_similarity: f64 },
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimResult {
    pub claim: String,
    #[serde(flatten)]
    pub verdict: ClaimVerdict,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocDriftResult {
    pub doc: String,
    pub title: String,
    pub claims: Vec<ClaimResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftReport {
    pub fixture_version: String,
    pub embedding_model: String,
    pub match_threshold: f64,
    pub drift_margin: f64,
    // ── aggregate quality ────────────────────────────────────────────────
    pub gold_drifted: usize,
    pub detected_drifted: usize,
    /// Gold-drifted claims the detector flagged.
    pub drift_recall: f64,
    /// Of everything flagged, how much was genuinely drifted.
    pub drift_precision: f64,
    /// Of the correctly flagged, how many proposals named the right memory.
    pub proposal_accuracy: f64,
    // ── the two failure lists that matter ────────────────────────────────
    /// Gold-ALIGNED claims flagged as drift — the hard-gate list. A detector
    /// that attacks correct docs teaches authors to ignore it.
    pub false_alarms: Vec<String>,
    /// Gold-drifted claims the detector missed.
    pub misses: Vec<String>,
    pub docs: Vec<DocDriftResult>,
}

/// Committed baseline for the drift soft gate (`results/drift-baseline.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftBaseline {
    pub embedding_model: String,
    pub fixture_version: String,
    pub drift_recall: f64,
    pub drift_precision: f64,
    pub proposal_accuracy: f64,
}

/// Soft-gate band. The corpus is small and hand-labeled, so one claim flipping
/// moves a rate by ~0.1; anything tighter would false-alarm on a single edge
/// case, anything looser could hide a real regression across two.
const RATE_DELTA: f64 = 0.10;

impl DriftBaseline {
    pub fn from_report(r: &DriftReport) -> Self {
        Self {
            embedding_model: r.embedding_model.clone(),
            fixture_version: r.fixture_version.clone(),
            drift_recall: r.drift_recall,
            drift_precision: r.drift_precision,
            proposal_accuracy: r.proposal_accuracy,
        }
    }
}

/// The absolute failures — findings, not scores. Empty = pass.
pub fn hard_failures(r: &DriftReport) -> Vec<String> {
    r.false_alarms
        .iter()
        .map(|c| {
            format!(
                "FALSE ALARM: an aligned claim was flagged as drift — the detector is \
                 attacking correct documentation: `{c}`"
            )
        })
        .collect()
}

/// Compare against the committed baseline. Cross-embedder comparisons are
/// refused (recalibrate instead); then no rate may fall more than
/// [`RATE_DELTA`] below baseline. Empty = pass.
pub fn regression_failures(r: &DriftReport, b: &DriftBaseline) -> Vec<String> {
    let mut f = Vec::new();
    if r.embedding_model != b.embedding_model {
        f.push(format!(
            "embedder mismatch: run={} baseline={} — recalibrate instead of comparing across embedders",
            r.embedding_model, b.embedding_model
        ));
        return f;
    }
    for (name, run, base) in [
        ("drift recall", r.drift_recall, b.drift_recall),
        ("drift precision", r.drift_precision, b.drift_precision),
        (
            "proposal accuracy",
            r.proposal_accuracy,
            b.proposal_accuracy,
        ),
    ] {
        if run < base - RATE_DELTA {
            f.push(format!(
                "{name} regressed: {run:.3} < baseline {base:.3} − {RATE_DELTA:.2}"
            ));
        }
    }
    f
}

/// One memory as the classifier sees it: fixture id, content, and (for a
/// superseded one) the terminal of its supersession chain.
struct CorpusEntry {
    id: String,
    content: String,
    /// `Some(fresh_id)` iff this belief is superseded.
    propose: Option<String>,
}

/// Split a document into scoreable claims: prose sentences, skipping headings,
/// fenced blocks, and fragments too short to state anything. Same shape as the
/// docs profile's prose scan — a heading is navigation, not a claim.
pub fn split_claims(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || t.is_empty() || t.starts_with('#') {
            continue;
        }
        for s in t.split_inclusive(['.', '!', '?']) {
            let s = s.trim().trim_end_matches(['.', '!', '?']).trim();
            if s.len() > 15 && s.chars().any(|c| c.is_alphabetic()) {
                out.push(s.to_string());
            }
        }
    }
    out
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    let na: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// (corpus index, cosine similarity) of the nearest neighbour, if any.
type Nearest = Option<(usize, f64)>;

/// The pure classifier: one claim's embedding against the two corpus halves.
/// Exposed shape for unit tests — no I/O, no embedder, just the decision rule.
fn classify(
    claim_vec: &[f32],
    current: &[(usize, Vec<f32>)],
    superseded: &[(usize, Vec<f32>)],
) -> (Nearest, Nearest) {
    let best = |set: &[(usize, Vec<f32>)]| {
        set.iter()
            .map(|(i, v)| (*i, cosine(claim_vec, v)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    };
    (best(current), best(superseded))
}

/// Run the drift profile over the fixture corpus.
pub async fn run(
    memories: &MemoriesFile,
    drift: &DriftFile,
    embedder: &dyn Embedder,
) -> Result<DriftReport> {
    if drift.docs.is_empty() {
        bail!("drift profile: the fixture tree has no drift gold (fixtures/v1/drift/docs.yaml)");
    }

    // Resolve every supersession chain to its terminal current memory once.
    let by_id: HashMap<&str, &brainiac_fixtures::schema::MemoryFx> = memories
        .memories
        .iter()
        .map(|m| (m.id.as_str(), m))
        .collect();
    let resolve = |start: &str| -> String {
        let mut cur = start;
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(next) = by_id.get(cur).and_then(|m| m.superseded_by.as_deref()) {
            if !seen.insert(cur) {
                break; // cycle — stop at the last stable hop
            }
            match by_id.get(next) {
                Some(_) => cur = next,
                None => break, // dangling pointer — linter's problem, not ours
            }
        }
        cur.to_string()
    };

    let mut current: Vec<CorpusEntry> = Vec::new();
    let mut superseded: Vec<CorpusEntry> = Vec::new();
    for m in &memories.memories {
        match &m.superseded_by {
            Some(next) => superseded.push(CorpusEntry {
                id: m.id.clone(),
                content: m.content.clone(),
                propose: Some(resolve(next)),
            }),
            None if m.status != "rejected" => current.push(CorpusEntry {
                id: m.id.clone(),
                content: m.content.clone(),
                propose: None,
            }),
            None => {}
        }
    }

    let cur_vecs = embedder
        .embed_batch(
            &current
                .iter()
                .map(|c| c.content.as_str())
                .collect::<Vec<_>>(),
        )
        .await
        .context("embedding current corpus")?;
    let sup_vecs = embedder
        .embed_batch(
            &superseded
                .iter()
                .map(|c| c.content.as_str())
                .collect::<Vec<_>>(),
        )
        .await
        .context("embedding superseded corpus")?;
    let cur_idx: Vec<(usize, Vec<f32>)> = cur_vecs.into_iter().enumerate().collect();
    let sup_idx: Vec<(usize, Vec<f32>)> = sup_vecs.into_iter().enumerate().collect();

    let mut docs_out = Vec::new();
    let (mut gold_drifted, mut hits, mut proposals_right, mut flagged) =
        (0usize, 0usize, 0usize, 0usize);
    let mut false_alarms = Vec::new();
    let mut misses = Vec::new();
    let mut flagged_wrong = 0usize;

    for d in &drift.docs {
        let claims = split_claims(&d.body);
        // Every gold entry must locate exactly one split claim — otherwise the
        // scoring silently drops a label and the rates lie.
        let mut gold_of_claim: HashMap<usize, &brainiac_fixtures::schema::DriftGoldFx> =
            HashMap::new();
        for g in &d.gold {
            let matched: Vec<usize> = claims
                .iter()
                .enumerate()
                .filter(|(_, c)| c.contains(g.claim.trim_end_matches(['.', '!', '?'])))
                .map(|(i, _)| i)
                .collect();
            let [one] = matched.as_slice() else {
                bail!(
                    "drift gold `{}` in {} matched {} split claims (must be exactly 1) — \
                     claims were: {claims:#?}",
                    g.claim,
                    d.id,
                    matched.len()
                );
            };
            gold_of_claim.insert(*one, g);
        }

        let claim_vecs = embedder
            .embed_batch(&claims.iter().map(|c| c.as_str()).collect::<Vec<_>>())
            .await
            .context("embedding doc claims")?;

        let mut results = Vec::new();
        for (i, (claim, vec)) in claims.iter().zip(claim_vecs.iter()).enumerate() {
            let (best_c, best_s) = classify(vec, &cur_idx, &sup_idx);
            let sim_c = best_c.map(|(_, s)| s).unwrap_or(0.0);
            let verdict = match best_s {
                Some((si, sim_s)) if sim_s >= MATCH_THRESHOLD && sim_s > sim_c + DRIFT_MARGIN => {
                    ClaimVerdict::Drifted {
                        stale_memory: superseded[si].id.clone(),
                        propose: superseded[si].propose.clone().unwrap_or_default(),
                        stale_similarity: round3(sim_s),
                        fresh_similarity: round3(sim_c),
                    }
                }
                _ => match best_c {
                    Some((ci, sim)) if sim >= MATCH_THRESHOLD => ClaimVerdict::Aligned {
                        memory: current[ci].id.clone(),
                        similarity: round3(sim),
                    },
                    _ => ClaimVerdict::Unmatched {
                        best_similarity: round3(sim_c.max(best_s.map(|(_, s)| s).unwrap_or(0.0))),
                    },
                },
            };

            // Score against the gold label, if this claim carries one.
            if let Some(g) = gold_of_claim.get(&i) {
                let is_flagged = matches!(verdict, ClaimVerdict::Drifted { .. });
                match g.label.as_str() {
                    "drifted" => {
                        gold_drifted += 1;
                        if is_flagged {
                            hits += 1;
                            if let ClaimVerdict::Drifted { propose, .. } = &verdict {
                                if Some(propose.as_str()) == g.propose.as_deref() {
                                    proposals_right += 1;
                                }
                            }
                        } else {
                            misses.push(format!("{}: {}", d.id, claim));
                        }
                    }
                    "aligned" if is_flagged => {
                        false_alarms.push(format!("{}: {}", d.id, claim));
                    }
                    _ if is_flagged => flagged_wrong += 1, // unmatched flagged: precision cost
                    _ => {}
                }
                if is_flagged {
                    flagged += 1;
                }
            } else if matches!(verdict, ClaimVerdict::Drifted { .. }) {
                // An unlabeled claim got flagged — fixture discipline says label
                // everything, so treat it as a precision cost, loudly.
                flagged += 1;
                flagged_wrong += 1;
            }

            results.push(ClaimResult {
                claim: claim.clone(),
                verdict,
            });
        }
        docs_out.push(DocDriftResult {
            doc: d.id.clone(),
            title: d.title.clone(),
            claims: results,
        });
    }

    let flagged_right = flagged - false_alarms.len() - flagged_wrong;
    Ok(DriftReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
        match_threshold: MATCH_THRESHOLD,
        drift_margin: DRIFT_MARGIN,
        gold_drifted,
        detected_drifted: flagged,
        drift_recall: ratio(hits, gold_drifted),
        drift_precision: ratio(flagged_right, flagged),
        proposal_accuracy: ratio(proposals_right, hits),
        false_alarms,
        misses,
        docs: docs_out,
    })
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainiac_core::embed::DeterministicEmbedder;
    use serde_json::json;

    fn memories(v: serde_json::Value) -> MemoriesFile {
        serde_json::from_value(v).expect("memories fixture")
    }
    fn drift(v: serde_json::Value) -> DriftFile {
        serde_json::from_value(v).expect("drift fixture")
    }

    #[test]
    fn split_claims_skips_scaffolding_and_fragments() {
        let md = "# Title\n\n## Section\n\nThe api timeout is ten seconds.\nok.\n\n```yaml\nnot: a claim in a fence\n```\n\nDeploys go through the blue pipeline!\n";
        let claims = split_claims(md);
        assert_eq!(
            claims,
            vec![
                "The api timeout is ten seconds".to_string(),
                "Deploys go through the blue pipeline".to_string(),
            ],
            "headings, fences, and fragments are not claims"
        );
    }

    /// The whole decision rule in one corpus: a stale restatement is flagged
    /// with the right proposal, a FRESH claim sharing most of the stale
    /// belief's vocabulary is left alone (the margin), and a claim the corpus
    /// knows nothing about is a harvest candidate, never drift.
    #[tokio::test]
    async fn drift_is_flagged_fresh_is_spared_unknown_is_harvest() {
        let mems = memories(json!({ "memories": [
            { "id": "m-old", "team": "t", "kind": "fact",
              "content": "the api gateway request timeout is 10 seconds",
              "superseded_by": "m-new", "status": "deprecated" },
            { "id": "m-new", "team": "t", "kind": "decision",
              "content": "the api gateway request timeout was raised to 30 seconds after the incident review" },
        ]}));
        let d = drift(json!({ "docs": [ {
            "id": "d1", "title": "stale-and-fresh",
            "body": "# Guide\n\nthe api gateway request timeout is 10 seconds.\nthe api gateway request timeout was raised to 30 seconds recently.\nOn-call handover happens every Monday morning at standup.\n",
            "gold": [
                { "claim": "timeout is 10 seconds", "label": "drifted", "propose": "m-new" },
                { "claim": "raised to 30 seconds", "label": "aligned" },
                { "claim": "On-call handover happens", "label": "unmatched" },
            ]
        }]}));
        let embedder = DeterministicEmbedder::default();
        let r = run(&mems, &d, &embedder).await.expect("run");

        assert!(r.false_alarms.is_empty(), "{:?}", r.false_alarms);
        assert_eq!((r.gold_drifted, r.detected_drifted), (1, 1));
        assert_eq!(
            (r.drift_recall, r.drift_precision, r.proposal_accuracy),
            (1.0, 1.0, 1.0)
        );
        let verdicts: Vec<&ClaimVerdict> = r.docs[0].claims.iter().map(|c| &c.verdict).collect();
        assert!(
            matches!(verdicts[0], ClaimVerdict::Drifted { propose, .. } if propose == "m-new"),
            "{verdicts:?}"
        );
        assert!(
            matches!(verdicts[1], ClaimVerdict::Aligned { memory, .. } if memory == "m-new"),
            "the fresh claim shares the stale belief's vocabulary and must NOT be flagged: {verdicts:?}"
        );
        assert!(
            matches!(verdicts[2], ClaimVerdict::Unmatched { .. }),
            "unknown knowledge is a harvest candidate, not drift: {verdicts:?}"
        );
    }

    /// A supersession CHAIN proposes its terminal: the doc author must be sent
    /// to the current belief, not from one stale belief to another.
    #[tokio::test]
    async fn a_supersession_chain_proposes_its_terminal() {
        let mems = memories(json!({ "memories": [
            { "id": "m-v1", "team": "t", "kind": "fact",
              "content": "billing exports run nightly through the cron box",
              "superseded_by": "m-v2", "status": "deprecated" },
            { "id": "m-v2", "team": "t", "kind": "fact",
              "content": "billing exports run nightly through the airflow dag",
              "superseded_by": "m-v3", "status": "deprecated" },
            { "id": "m-v3", "team": "t", "kind": "decision",
              "content": "billing exports stream continuously through the events pipeline since june" },
        ]}));
        let d = drift(json!({ "docs": [ {
            "id": "d1", "title": "chained",
            "body": "billing exports run nightly through the cron box.\n",
            "gold": [
                { "claim": "through the cron box", "label": "drifted", "propose": "m-v3" },
            ]
        }]}));
        let embedder = DeterministicEmbedder::default();
        let r = run(&mems, &d, &embedder).await.expect("run");
        assert_eq!(r.proposal_accuracy, 1.0, "misses: {:?}", r.misses);
    }

    /// The hard gate in action: force a false alarm (a gold-aligned claim that
    /// IS verbatim a superseded belief) and the report must carry it.
    #[tokio::test]
    async fn a_false_alarm_reaches_the_hard_gate() {
        let mems = memories(json!({ "memories": [
            { "id": "m-old", "team": "t", "kind": "fact",
              "content": "the settlement batch window closes at midnight utc",
              "superseded_by": "m-new", "status": "deprecated" },
            { "id": "m-new", "team": "t", "kind": "decision",
              "content": "settlement moved to continuous clearing with no batch window" },
        ]}));
        let d = drift(json!({ "docs": [ {
            "id": "d1", "title": "mislabeled",
            "body": "the settlement batch window closes at midnight utc.\n",
            "gold": [
                { "claim": "batch window closes at midnight", "label": "aligned" },
            ]
        }]}));
        let embedder = DeterministicEmbedder::default();
        let r = run(&mems, &d, &embedder).await.expect("run");
        assert_eq!(r.false_alarms.len(), 1);
        let hard = hard_failures(&r);
        assert_eq!(hard.len(), 1);
        assert!(hard[0].contains("FALSE ALARM"), "{hard:?}");
    }

    #[test]
    fn cross_embedder_baselines_are_refused() {
        let b = DriftBaseline {
            embedding_model: "text-embedding-v4".into(),
            fixture_version: "v1".into(),
            drift_recall: 1.0,
            drift_precision: 1.0,
            proposal_accuracy: 1.0,
        };
        let r = DriftReport {
            fixture_version: "v1".into(),
            embedding_model: "deterministic-bow-v1".into(),
            match_threshold: MATCH_THRESHOLD,
            drift_margin: DRIFT_MARGIN,
            gold_drifted: 0,
            detected_drifted: 0,
            drift_recall: 1.0,
            drift_precision: 1.0,
            proposal_accuracy: 1.0,
            false_alarms: vec![],
            misses: vec![],
            docs: vec![],
        };
        let f = regression_failures(&r, &b);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("embedder mismatch"), "{f:?}");
    }
}
