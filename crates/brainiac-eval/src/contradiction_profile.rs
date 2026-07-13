//! The `contradiction` eval profile (EVAL.md §2.3, §3): score the REAL pipeline
//! contradict stage against the Meridian gold contradiction cases.
//!
//! # Why a standalone profile (not folded into `pipeline`)
//!
//! [`brainiac_pipeline::contradict::run_contradict`] takes ONE memory and does
//! its own candidate discovery (nearest vectors that share an entity anchor),
//! then asks the provider for a verdict. Scoring it against gold means seeding
//! specific memory PAIRS and invoking the stage per case — a seed strategy
//! fundamentally different from the `pipeline` profile's transcript-drain. So
//! this is its own profile with its own baseline; folding it in would conflate
//! two unrelated seedings.
//!
//! # Method
//!
//! Each gold case names two gold memories and an expected relation
//! (`resolved_supersede` / `resolved_coexist` / `dismissed`, with a supersede
//! direction where it applies). For each case we:
//!   1. seed a FRESH synthetic org (namespaced ids) holding just that case's two
//!      memories + their entity anchors + embeddings — full per-case isolation,
//!      so the candidate search can only surface the partner and cases never
//!      cross-contaminate;
//!   2. run the real contradict stage on the newer memory (`memory_b`) inside a
//!      `worker_tx` — the exact authority the pipeline's stage-5 uses;
//!   3. read back whether a contradiction row opened and its suggested direction.
//!
//! The verdict provider is a deterministic ORACLE derived from the gold (the
//! contradiction analog of the resolution profile's oracle adjudicator): given
//! the two contents it returns the gold relation, and for a supersede it names
//! the gold winner. So — exactly like the other profiles' gold mocks — the score
//! numbers are PLUMBING floors, not a quality claim about any real adjudicator.
//!
//! What the floor actually measures is the stage's CANDIDATE-DISCOVERY reach: a
//! gold supersede whose two memories share no entity anchor is never compared
//! (the anchor filter drops it), so it is honestly recorded as `not_compared`
//! and counts against recall. That gap is a real property of the pipeline worth
//! surfacing, not smoothed over.
//!
//! Visibility is forced to `org` on the seeded memories: like the temporal
//! suite, this profile measures contradiction LOGIC, not RLS visibility (the
//! leak suite owns that), and org visibility keeps the candidate search
//! deterministic regardless of team membership.
//!
//! Scores (EVAL.md §2.3):
//! - detection recall  = gold supersede pairs the stage flagged / all gold supersede;
//! - detection precision = flagged supersedes / all flagged (a coexist/dismiss
//!   pair that gets flagged is a false positive — the queue-poisoning failure);
//! - false-positive rate = non-contradictory pairs flagged / all non-contradictory;
//! - supersede-direction accuracy = flagged supersedes with the right direction.
//!
//! One soft regression gate rides on this (mirroring the pipeline/resolution
//! gates): recall/precision/direction may not regress, and the false-positive
//! rate may not rise, past a delta; a cross-config comparison (different embedder
//! OR provider) is refused.

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{MemoryKind, MemoryStatus, Visibility};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_gateway::{ChatProvider, ChatRequest, MockProvider};
use brainiac_pipeline::{contradict::run_contradict, pipeline_principal};
use brainiac_store::{entities, memories, orgs, Store};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct ContradictionReport {
    pub fixture_version: String,
    pub embedding_model: String,
    /// The verdict provider (the gold oracle mock's model ref) — tagged like the
    /// embedder because detection quality is provider-specific; the gate refuses
    /// to compare across providers.
    pub provider: String,
    // ── detection quality (EVAL.md §2.3) ─────────────────────────────────
    /// Gold cases whose expected relation is a supersession.
    pub gold_supersede: usize,
    /// Gold cases that must NOT be flagged (coexist + dismiss).
    pub gold_non_contradiction: usize,
    /// Gold supersede pairs the stage flagged (true positives).
    pub detected_supersede: usize,
    /// Non-contradictory pairs the stage flagged anyway (false positives).
    pub false_positives: usize,
    /// Supersede pairs the stage never compared (no shared entity anchor) — a
    /// candidate-discovery gap, counted against recall and surfaced for triage.
    pub not_compared: usize,
    pub detection_recall: f64,
    pub detection_precision: f64,
    pub false_positive_rate: f64,
    /// Flagged supersedes whose suggested direction matched gold.
    pub supersede_direction_correct: usize,
    /// Flagged supersedes for which gold specifies a direction (the denominator).
    pub supersede_direction_scored: usize,
    pub direction_accuracy: f64,
    /// Per-case outcomes (failures/interesting first: see [`ContradictionReport::sort_cases`]).
    pub cases: Vec<ContradictionCaseResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContradictionCaseResult {
    pub id: String,
    /// resolved_supersede | resolved_coexist | dismissed
    pub expected: String,
    /// Did the stage actually compare the pair (shared anchor + surfaced candidate)?
    pub compared: bool,
    /// Did a contradiction row open?
    pub flagged: bool,
    pub expected_direction: Option<String>,
    pub detected_direction: Option<String>,
    /// true_positive | true_negative | false_positive | missed | not_compared
    pub outcome: String,
}

impl ContradictionReport {
    /// No hard gate (mirrors the pipeline profile): over-flagging is a soft
    /// quality regression, not a zero-tolerance invariant. Present for symmetry.
    pub fn gate_failures(&self) -> Vec<String> {
        Vec::new()
    }

    fn sort_cases(&mut self) {
        // Interesting first: anything not a clean true_positive/true_negative.
        self.cases.sort_by_key(|c| {
            let rank = match c.outcome.as_str() {
                "false_positive" => 0,
                "missed" => 1,
                "not_compared" => 2,
                _ => 3,
            };
            (rank, c.id.clone())
        });
    }
}

/// Committed baseline for the contradiction SOFT gate
/// (`results/contradiction-baseline.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionBaseline {
    pub embedding_model: String,
    pub provider: String,
    pub fixture_version: String,
    pub detection_recall: f64,
    pub detection_precision: f64,
    pub direction_accuracy: f64,
    pub false_positive_rate: f64,
}

/// Recall/precision/direction may not regress below baseline, and the
/// false-positive rate may not rise above it, by more than this.
const RATE_DELTA: f64 = 0.02;

impl ContradictionBaseline {
    pub fn from_report(report: &ContradictionReport) -> Self {
        Self {
            embedding_model: report.embedding_model.clone(),
            provider: report.provider.clone(),
            fixture_version: report.fixture_version.clone(),
            detection_recall: report.detection_recall,
            detection_precision: report.detection_precision,
            direction_accuracy: report.direction_accuracy,
            false_positive_rate: report.false_positive_rate,
        }
    }
}

/// Compare a run against the committed baseline (mirrors the pipeline gate):
/// refuse a cross-config comparison, then check each rate. Empty = pass.
pub fn regression_failures(
    report: &ContradictionReport,
    baseline: &ContradictionBaseline,
) -> Vec<String> {
    let mut fails = Vec::new();
    if report.embedding_model != baseline.embedding_model {
        fails.push(format!(
            "embedder mismatch: run={} baseline={} — recalibrate the baseline instead of comparing across embedders",
            report.embedding_model, baseline.embedding_model
        ));
        return fails;
    }
    if report.provider != baseline.provider {
        fails.push(format!(
            "provider mismatch: run={} baseline={} — detection quality is provider-specific; recalibrate instead of comparing across providers",
            report.provider, baseline.provider
        ));
        return fails;
    }
    if report.detection_recall < baseline.detection_recall - RATE_DELTA {
        fails.push(format!(
            "detection recall regressed: {:.3} < baseline {:.3} − {:.2}",
            report.detection_recall, baseline.detection_recall, RATE_DELTA
        ));
    }
    if report.detection_precision < baseline.detection_precision - RATE_DELTA {
        fails.push(format!(
            "detection precision regressed: {:.3} < baseline {:.3} − {:.2}",
            report.detection_precision, baseline.detection_precision, RATE_DELTA
        ));
    }
    if report.direction_accuracy < baseline.direction_accuracy - RATE_DELTA {
        fails.push(format!(
            "supersede-direction accuracy regressed: {:.3} < baseline {:.3} − {:.2}",
            report.direction_accuracy, baseline.direction_accuracy, RATE_DELTA
        ));
    }
    if report.false_positive_rate > baseline.false_positive_rate + RATE_DELTA {
        fails.push(format!(
            "false-positive rate rose: {:.3} > baseline {:.3} + {:.2} — over-flagging poisons the review queue",
            report.false_positive_rate, baseline.false_positive_rate, RATE_DELTA
        ));
    }
    fails
}

/// Map the fixture's expected relation to the contradict stage's verdict noun.
fn expected_relation(expected: &str) -> &'static str {
    match expected {
        "resolved_supersede" => "supersede",
        "resolved_coexist" => "coexist",
        _ => "dismiss",
    }
}

/// One oracle entry: the two memory contents and the gold verdict for the pair.
#[derive(Clone)]
struct OracleEntry {
    a: String,
    b: String,
    relation: &'static str,
    /// For a supersede: the winning memory's content (so the mock can name the
    /// winner relative to whichever slot the stage puts it in).
    winner_content: Option<String>,
}

/// A deterministic oracle verdict provider derived from the gold cases: given
/// the contradict prompt's two statements, it returns the gold relation and (for
/// a supersede) the gold winner. The contradiction analog of the resolution
/// profile's `oracle_adjudicator`.
fn oracle_verdict_mock(entries: Vec<OracleEntry>) -> MockProvider {
    MockProvider::new(move |req: &ChatRequest| {
        if !req.system.contains("Decide their relationship") {
            return "{}".to_string();
        }
        let (a, b) = match parse_ab(&req.user) {
            Some(pair) => pair,
            None => return dismiss(),
        };
        let Some(entry) = entries
            .iter()
            .find(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
        else {
            // Not a gold pair (cross-case noise can't reach here under per-case
            // isolation, but stay conservative): no contradiction.
            return dismiss();
        };
        match entry.relation {
            "supersede" => {
                let winner = match entry.winner_content.as_deref() {
                    Some(w) if w == a => "\"a\"",
                    Some(w) if w == b => "\"b\"",
                    _ => "null",
                };
                format!(r#"{{"relation":"supersede","winner":{winner},"reason":"oracle"}}"#)
            }
            "coexist" => r#"{"relation":"coexist","winner":null,"reason":"oracle"}"#.to_string(),
            _ => dismiss(),
        }
    })
}

fn dismiss() -> String {
    r#"{"relation":"dismiss","winner":null,"reason":"oracle"}"#.to_string()
}

/// Pull the two statements out of the contradict prompt body
/// (`A: <a>\nB: <b>` — see `run_contradict`). Contents are single-line gists.
fn parse_ab(user: &str) -> Option<(String, String)> {
    let rest = user.strip_prefix("A: ")?;
    let (a, b) = rest.split_once("\nB: ")?;
    Some((a.to_string(), b.to_string()))
}

/// Parse the suggested supersession direction out of the stored resolution note
/// (`suggested: supersede (b_over_a) — …`). `a_over_b` is checked first so the
/// shared `over_a`/`over_b` substrings don't collide.
fn parse_direction(note: &str) -> Option<String> {
    if note.contains("a_over_b") {
        Some("a_over_b".to_string())
    } else if note.contains("b_over_a") {
        Some("b_over_a".to_string())
    } else {
        None
    }
}

/// Run the contradiction profile end-to-end: per gold case, seed an isolated
/// synthetic org, run the real contradict stage, and score detection against
/// gold. The store is truncated by the caller; each case uses its own org so no
/// cross-case truncation is needed.
pub async fn run(
    store: &Store,
    fx: &Fixtures,
    embedder: &dyn Embedder,
) -> Result<ContradictionReport> {
    // Build the oracle entries + provider once.
    let mem = |id: &str| {
        fx.memories
            .memories
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("contradiction references unknown memory {id}"))
    };
    let mut entries: Vec<OracleEntry> = Vec::new();
    for c in &fx.contradictions.cases {
        let a = mem(&c.memory_a);
        let b = mem(&c.memory_b);
        let relation = expected_relation(&c.expected);
        let winner_content = if relation == "supersede" {
            // Gold direction is expressed as a/b = memory_a/memory_b; the stage
            // always slots memory_a into A and the processed memory_b into B, so
            // b_over_a means memory_b wins.
            match c.supersede_direction.as_deref() {
                Some("a_over_b") => Some(a.content.clone()),
                _ => Some(b.content.clone()),
            }
        } else {
            None
        };
        entries.push(OracleEntry {
            a: a.content.clone(),
            b: b.content.clone(),
            relation,
            winner_content,
        });
    }
    let provider = oracle_verdict_mock(entries);
    let provider_tag = provider.model_ref();

    let mut case_results: Vec<ContradictionCaseResult> = Vec::new();
    let mut detected_supersede = 0usize;
    let mut false_positives = 0usize;
    let mut not_compared = 0usize;
    let mut gold_supersede = 0usize;
    let mut gold_non_contradiction = 0usize;
    let mut direction_correct = 0usize;
    let mut direction_scored = 0usize;

    for c in &fx.contradictions.cases {
        let a = mem(&c.memory_a);
        let b = mem(&c.memory_b);
        let relation = expected_relation(&c.expected);
        let org_id = stable_uuid(&format!("contradict-org-{}", c.id));

        // Namespaced ids so cases sharing a gold memory don't collide across orgs.
        let ns = |id: &str| stable_uuid(&format!("{}::{}", c.id, id));
        let embedding_version =
            seed_case(store, fx, embedder, org_id, c.id.as_str(), a, b, &ns).await?;

        // ── run the REAL contradict stage on memory_b (worker authority) ──
        let principal = pipeline_principal(org_id);
        let mut tx = store.worker_tx(&principal).await?;
        let memory_b_id = ns(&b.id);
        let loaded = memories::get_by_ids(&mut tx, &[memory_b_id]).await?;
        let memory_b = loaded
            .into_iter()
            .next()
            .context("seeded memory_b not found")?;
        let stats = run_contradict(
            &mut tx,
            &provider,
            embedder,
            embedding_version,
            org_id,
            &memory_b,
        )
        .await?;
        let compared = stats.compared > 0;
        let flagged = stats.opened > 0;
        let detected_direction = if flagged {
            let note: Option<String> =
                sqlx::query("SELECT resolution_note FROM contradictions WHERE org_id = $1 LIMIT 1")
                    .bind(org_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .map(|r| r.get::<String, _>("resolution_note"));
            note.as_deref().and_then(parse_direction)
        } else {
            None
        };
        tx.commit().await?;

        // ── classify ─────────────────────────────────────────────────────
        let expected_direction = if relation == "supersede" {
            c.supersede_direction.clone()
        } else {
            None
        };
        let outcome = if relation == "supersede" {
            gold_supersede += 1;
            if flagged {
                detected_supersede += 1;
                if let Some(exp) = &expected_direction {
                    direction_scored += 1;
                    if detected_direction.as_deref() == Some(exp.as_str()) {
                        direction_correct += 1;
                    }
                }
                "true_positive"
            } else if !compared {
                not_compared += 1;
                "not_compared"
            } else {
                "missed"
            }
        } else {
            gold_non_contradiction += 1;
            if flagged {
                false_positives += 1;
                "false_positive"
            } else {
                "true_negative"
            }
        };

        case_results.push(ContradictionCaseResult {
            id: c.id.clone(),
            expected: c.expected.clone(),
            compared,
            flagged,
            expected_direction,
            detected_direction,
            outcome: outcome.to_string(),
        });
    }

    let detection_recall = ratio(detected_supersede, gold_supersede);
    let flagged_total = detected_supersede + false_positives;
    let detection_precision = if flagged_total == 0 {
        1.0
    } else {
        detected_supersede as f64 / flagged_total as f64
    };
    let false_positive_rate = ratio(false_positives, gold_non_contradiction);
    let direction_accuracy = if direction_scored == 0 {
        1.0
    } else {
        direction_correct as f64 / direction_scored as f64
    };

    let mut report = ContradictionReport {
        fixture_version: "v1".into(),
        embedding_model: embedder.model_name().to_string(),
        provider: provider_tag,
        gold_supersede,
        gold_non_contradiction,
        detected_supersede,
        false_positives,
        not_compared,
        detection_recall,
        detection_precision,
        false_positive_rate,
        supersede_direction_correct: direction_correct,
        supersede_direction_scored: direction_scored,
        direction_accuracy,
        cases: case_results,
    };
    report.sort_cases();
    Ok(report)
}

/// Seed one case's isolated org: identity, the two memories' entity anchors, and
/// the two memories (org visibility) with embeddings. Returns the embedding
/// version. Uses a scoped writer tx (INSERT policies are org-checked).
#[allow(clippy::too_many_arguments)]
async fn seed_case(
    store: &Store,
    fx: &Fixtures,
    embedder: &dyn Embedder,
    org_id: Uuid,
    case_id: &str,
    a: &brainiac_fixtures::schema::MemoryFx,
    b: &brainiac_fixtures::schema::MemoryFx,
    ns: &dyn Fn(&str) -> Uuid,
) -> Result<i32> {
    let principal = pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&principal).await?;

    orgs::upsert_org(&mut tx, org_id, &format!("contradict-{case_id}")).await?;
    // Teams for both memories (namespaced so distinct orgs never collide).
    for team in [&a.team, &b.team] {
        orgs::upsert_team(&mut tx, ns(team), org_id, team).await?;
    }
    // Entity anchors: the union of both memories' entities. A shared gold entity
    // gets the SAME namespaced id in both memories, preserving the anchor overlap
    // the stage's candidate filter keys on.
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for e in a.entities.iter().chain(b.entities.iter()) {
        if !seen.insert(e.as_str()) {
            continue;
        }
        let fx_ent = fx.entities.entities.iter().find(|x| x.id == *e);
        let (name, kind, team) = match fx_ent {
            Some(x) => (x.name.as_str(), x.kind.as_str(), x.team.as_str()),
            None => (e.as_str(), "service", a.team.as_str()),
        };
        entities::insert_entity(
            &mut tx,
            ns(e),
            org_id,
            Some(ns(team)),
            name,
            kind,
            &[],
            None,
        )
        .await?;
    }

    let embedding_version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await?;
    for m in [a, b] {
        let id = ns(&m.id);
        memories::insert(
            &mut tx,
            &memories::NewMemory {
                id,
                org_id,
                team_id: Some(ns(&m.team)),
                owner_user_id: None,
                // Force org visibility: this profile measures contradiction
                // logic, not RLS (the leak suite owns that), and org visibility
                // keeps candidate discovery deterministic.
                visibility: Visibility::Org,
                status: MemoryStatus::parse(&m.status).unwrap_or(MemoryStatus::Canonical),
                kind: MemoryKind::parse(&m.kind).unwrap_or(MemoryKind::Fact),
                content: m.content.clone(),
                language: m.language.clone(),
                valid_from: m.valid_from,
                valid_to: m.valid_to,
                superseded_by: None,
                confidence: None,
                provenance_id: None,
            },
        )
        .await?;
        for e in &m.entities {
            memories::link_entity(&mut tx, id, ns(e)).await?;
        }
        memories::upsert_embedding(
            &mut tx,
            id,
            embedding_version,
            &embedder.embed(&m.content).await?,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(embedding_version)
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}
