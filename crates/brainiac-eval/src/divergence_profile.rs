//! The `divergence` eval profile: score the REAL practice-divergence sweep
//! ([`brainiac_pipeline::divergence::scan_org`]) against the Meridian gold
//! divergence clusters.
//!
//! # Why a standalone profile
//!
//! The divergence sweep is the org-intelligence analog of `contradict`: where
//! contradiction catches two facts that cannot both be true, divergence catches
//! several teams solving the SAME practice in DIFFERENT, each-locally-reasonable
//! ways — a standardization signal invisible to any one team. It was the only
//! LLM call site with no eval coverage. This profile is that coverage, and it
//! mirrors the `contradiction` profile exactly: seed an isolated synthetic org,
//! drive the REAL stage with a deterministic gold-oracle mock, and score the
//! stored verdicts against gold.
//!
//! # Method
//!
//! The gold ([`brainiac_fixtures::DivergenceFile`]) is a handful of clusters,
//! each a canonical entity plus the per-team memories recorded about it, with an
//! `expected_divergence` verdict. For the whole set we:
//!   1. seed ONE isolated eval org holding every cluster — canonical entities,
//!      per-team raw entities linked to them, and per-team memories anchored to
//!      those entities (the exact graph shape the sweep's cluster query walks:
//!      canonical_entities → entity_links → entities → memory_entities →
//!      memories → teams);
//!   2. run the real `scan_org` over that org with a gold-oracle
//!      [`brainiac_gateway::MockProvider`] — given the cluster's canonical name
//!      in the adjudication prompt, it returns the gold verdict;
//!   3. read back the `practice_divergences` rows the sweep stored (team axis)
//!      and score detection against gold.
//!
//! Like the other profiles' gold mocks, the score numbers are PLUMBING floors,
//! not a quality claim about any real adjudicator. What the floor actually
//! verifies is that the sweep GATHERS each cross-team cluster, fires the team
//! axis, parses the verdict, and — the load-bearing half — stores ONLY the
//! divergent ones: a consistent cluster (agreement, or two different aspects)
//! that gets a row is the queue-poisoning false positive the sweep is built to
//! avoid, and it is scored as such.
//!
//! Provider-specific by nature (detection quality is the adjudicator's), so the
//! report is tagged with the provider and the soft gate refuses a cross-provider
//! comparison — exactly like the contradiction/extraction gates.

use anyhow::{Context, Result};
use brainiac_core::{MemoryKind, MemoryStatus, Visibility};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::schema::DivergenceClusterFx;
use brainiac_fixtures::Fixtures;
use brainiac_gateway::{ChatRequest, MockProvider, ProviderRouter, Stage};
use brainiac_pipeline::{divergence, pipeline_principal};
use brainiac_store::{entities, memories, orgs, Store};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// The isolated org this profile seeds and scans. Distinct from the Meridian
/// org so the divergence gold never entangles with the retrieval corpus (and so
/// a stray Meridian cluster can't inflate the score).
const EVAL_ORG: &str = "divergence-eval-org";

#[derive(Debug, Clone, Serialize)]
pub struct DivergenceReport {
    pub fixture_version: String,
    /// The adjudicating provider (the gold-oracle mock's model ref) — the config
    /// axis for this profile; the gate refuses to compare across providers.
    pub provider: String,
    // ── corpus shape ─────────────────────────────────────────────────────
    /// Gold clusters whose expected verdict is a divergence.
    pub gold_divergent: usize,
    /// Gold clusters that must NOT be flagged (consistent / different-aspect).
    pub gold_consistent: usize,
    /// (cluster, axis) adjudications the sweep actually ran. Equals the gold
    /// cluster count when every cluster was gathered on the team axis; a shortfall
    /// means the sweep never saw a cluster (a graph-gathering gap worth triage).
    pub clusters_scanned: usize,
    // ── detection quality ────────────────────────────────────────────────
    /// Divergent gold clusters the sweep flagged (true positives).
    pub detected_divergent: usize,
    /// Consistent gold clusters the sweep flagged anyway (false positives — the
    /// queue-poisoning failure).
    pub false_positives: usize,
    /// Divergent gold clusters the sweep missed (false negatives).
    pub missed: usize,
    pub detection_recall: f64,
    pub detection_precision: f64,
    pub false_positive_rate: f64,
    /// Detected divergences whose stored practice name matched gold (a detail
    /// signal — the sweep found the divergence AND named the right practice).
    pub practice_correct: usize,
    /// Per-cluster outcomes (failures/interesting first).
    pub cases: Vec<DivergenceCaseResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DivergenceCaseResult {
    pub id: String,
    pub canonical: String,
    pub expected_divergence: bool,
    /// Did a `practice_divergences` row open for this cluster (team axis)?
    pub detected: bool,
    /// The practice the sweep stored, if it opened a row.
    pub stored_practice: Option<String>,
    /// true_positive | true_negative | false_positive | missed
    pub outcome: String,
}

impl DivergenceReport {
    /// No hard gate (mirrors the contradiction profile): over-flagging is a soft
    /// quality regression tracked by the false-positive rate, not a
    /// zero-tolerance build invariant. Present for symmetry with the other
    /// profiles' dispatch.
    pub fn gate_failures(&self) -> Vec<String> {
        Vec::new()
    }

    fn sort_cases(&mut self) {
        // Interesting first: anything that is not a clean TP/TN.
        self.cases.sort_by_key(|c| {
            let rank = match c.outcome.as_str() {
                "false_positive" => 0,
                "missed" => 1,
                _ => 2,
            };
            (rank, c.id.clone())
        });
    }
}

/// Committed baseline for the divergence SOFT gate
/// (`results/divergence-baseline.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergenceBaseline {
    pub provider: String,
    pub fixture_version: String,
    pub detection_recall: f64,
    pub detection_precision: f64,
    pub false_positive_rate: f64,
}

/// Recall/precision may not regress below baseline, and the false-positive rate
/// may not rise above it, by more than this.
const RATE_DELTA: f64 = 0.02;

impl DivergenceBaseline {
    pub fn from_report(report: &DivergenceReport) -> Self {
        Self {
            provider: report.provider.clone(),
            fixture_version: report.fixture_version.clone(),
            detection_recall: report.detection_recall,
            detection_precision: report.detection_precision,
            false_positive_rate: report.false_positive_rate,
        }
    }
}

/// Compare a run against the committed baseline (mirrors the contradiction
/// gate): refuse a cross-provider comparison, then check each rate. Empty = pass.
pub fn regression_failures(
    report: &DivergenceReport,
    baseline: &DivergenceBaseline,
) -> Vec<String> {
    let mut fails = Vec::new();
    if report.provider != baseline.provider {
        fails.push(format!(
            "provider mismatch: run={} baseline={} — detection quality is provider-specific; \
             recalibrate instead of comparing across providers",
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
    if report.false_positive_rate > baseline.false_positive_rate + RATE_DELTA {
        fails.push(format!(
            "false-positive rate rose: {:.3} > baseline {:.3} + {:.2} — over-flagging poisons \
             the review queue",
            report.false_positive_rate, baseline.false_positive_rate, RATE_DELTA
        ));
    }
    fails
}

/// The system prompt of the TEAM-axis divergence adjudication — the mock keys on
/// this signature so it never misfires on the project-axis prompt (which this
/// gold never triggers, having no project-stamped positions) or any other call.
const TEAM_AXIS_SIGNATURE: &str = "Several teams in ONE organization";

/// Pull the canonical entity name out of the divergence adjudication prompt.
/// `scan_org` builds the user body as
/// `System / concept: {name}\n\nWhat each team...`, so the name is the first
/// line after the fixed prefix.
fn parse_canonical(user: &str) -> Option<&str> {
    let rest = user.strip_prefix("System / concept: ")?;
    Some(rest.split('\n').next().unwrap_or(rest).trim())
}

/// A deterministic gold-oracle verdict provider derived from the gold clusters:
/// given the divergence prompt, it reads the canonical name and returns the gold
/// verdict (divergent → the gold practice; consistent → `divergence:false`). The
/// divergence analog of the contradiction profile's `oracle_verdict_mock`.
fn oracle_verdict_mock(clusters: &[DivergenceClusterFx]) -> MockProvider {
    // canonical name → (divergent, practice)
    let gold: HashMap<String, (bool, String)> = clusters
        .iter()
        .map(|c| {
            (
                c.canonical.clone(),
                (c.expected_divergence, c.practice.clone()),
            )
        })
        .collect();
    MockProvider::new(move |req: &ChatRequest| {
        if !req.system.contains(TEAM_AXIS_SIGNATURE) {
            // Not a team-axis divergence call — never flag.
            return consistent();
        }
        match parse_canonical(&req.user).and_then(|c| gold.get(c)) {
            Some((true, practice)) => divergent(practice),
            // Consistent gold, or a canonical the gold doesn't know (can't happen
            // under the isolated eval org, but stay conservative): no divergence.
            _ => consistent(),
        }
    })
}

fn divergent(practice: &str) -> String {
    // approaches is stored verbatim as jsonb; an empty array is valid and the
    // profile scores the verdict + practice, not the per-team approaches.
    format!(
        r#"{{"divergence":true,"practice":{},"summary":"the teams solve one practice incompatibly","approaches":[],"recommended_standard":"adopt a single org standard","impact":"medium"}}"#,
        serde_json::Value::String(practice.to_string())
    )
}

fn consistent() -> String {
    r#"{"divergence":false,"practice":"","summary":"","approaches":[],"recommended_standard":"","impact":"low"}"#
        .to_string()
}

/// Run the divergence profile: seed the gold clusters into an isolated org, run
/// the REAL sweep with the gold-oracle mock, and score detection against gold.
/// `admin` is the raw (RLS-bypassing) pool the sweep reads and writes on; the
/// tenant is truncated by the caller.
pub async fn run(store: &Store, admin: &sqlx::PgPool, fx: &Fixtures) -> Result<DivergenceReport> {
    anyhow::ensure!(
        !fx.divergence.clusters.is_empty(),
        "divergence profile: the fixture tree has no divergence gold (fixtures/v1/divergence/clusters.yaml)"
    );

    let org_id = stable_uuid(EVAL_ORG);
    seed(store, fx, org_id).await?;

    // ── run the REAL sweep with the gold oracle ──────────────────────────
    // Route through the ProviderRouter's Divergence stage — the same seam the
    // production sweep uses — so the report's provider tag reflects the stage a
    // real run would route (BRAINIAC_MODEL_DIVERGENCE), not a bare handle.
    let providers = ProviderRouter::single(std::sync::Arc::new(oracle_verdict_mock(
        &fx.divergence.clusters,
    )));
    let provider = providers.for_stage(Stage::Divergence);
    let provider_tag = provider.model_ref();
    let stats = divergence::scan_org(admin, provider, org_id)
        .await
        .context("running the divergence sweep")?;

    // ── read back what the sweep stored (team axis) ──────────────────────
    let stored: HashMap<String, String> = sqlx::query(
        "SELECT ce.name AS canonical, pd.practice
         FROM practice_divergences pd
         JOIN canonical_entities ce ON ce.id = pd.canonical_id
         WHERE pd.org_id = $1 AND pd.axis = 'team'",
    )
    .bind(org_id)
    .fetch_all(admin)
    .await
    .context("reading stored divergences")?
    .into_iter()
    .map(|r| {
        (
            r.get::<String, _>("canonical"),
            r.get::<String, _>("practice"),
        )
    })
    .collect();

    // ── score ────────────────────────────────────────────────────────────
    let mut cases = Vec::new();
    let (mut gold_divergent, mut gold_consistent) = (0usize, 0usize);
    let (mut tp, mut fp, mut fn_, mut practice_correct) = (0usize, 0usize, 0usize, 0usize);

    for c in &fx.divergence.clusters {
        let stored_practice = stored.get(&c.canonical).cloned();
        let detected = stored_practice.is_some();
        let outcome = if c.expected_divergence {
            gold_divergent += 1;
            if detected {
                tp += 1;
                if stored_practice.as_deref() == Some(c.practice.as_str()) {
                    practice_correct += 1;
                }
                "true_positive"
            } else {
                fn_ += 1;
                "missed"
            }
        } else {
            gold_consistent += 1;
            if detected {
                fp += 1;
                "false_positive"
            } else {
                "true_negative"
            }
        };
        cases.push(DivergenceCaseResult {
            id: c.id.clone(),
            canonical: c.canonical.clone(),
            expected_divergence: c.expected_divergence,
            detected,
            stored_practice,
            outcome: outcome.to_string(),
        });
    }

    let detection_recall = ratio(tp, gold_divergent);
    let flagged_total = tp + fp;
    let detection_precision = if flagged_total == 0 {
        1.0
    } else {
        tp as f64 / flagged_total as f64
    };
    let false_positive_rate = ratio(fp, gold_consistent);

    let mut report = DivergenceReport {
        fixture_version: "v1".into(),
        provider: provider_tag,
        gold_divergent,
        gold_consistent,
        clusters_scanned: stats.clusters,
        detected_divergent: tp,
        false_positives: fp,
        missed: fn_,
        detection_recall,
        detection_precision,
        false_positive_rate,
        practice_correct,
        cases,
    };
    report.sort_cases();
    Ok(report)
}

/// Seed the isolated eval org: identity, the per-cluster canonical entities,
/// per-team raw entities linked to them, and per-team memories anchored to those
/// entities. The graph is exactly what `scan_org`'s cluster query walks. Uses a
/// scoped writer tx (INSERT policies are org-checked).
async fn seed(store: &Store, fx: &Fixtures, org_id: Uuid) -> Result<()> {
    let principal = pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&principal).await?;

    orgs::upsert_org(&mut tx, org_id, EVAL_ORG).await?;

    // Every distinct team across all clusters, namespaced by name.
    let team_id = |name: &str| stable_uuid(&format!("divg-team-{name}"));
    let mut teams_seen: HashSet<&str> = HashSet::new();
    for c in &fx.divergence.clusters {
        for p in &c.positions {
            if teams_seen.insert(p.team.as_str()) {
                orgs::upsert_team(&mut tx, team_id(&p.team), org_id, &p.team).await?;
            }
        }
    }

    for c in &fx.divergence.clusters {
        let canonical_id = stable_uuid(&format!("divg-canon-{}", c.id));
        entities::insert_canonical(
            &mut tx,
            canonical_id,
            org_id,
            &c.canonical,
            &c.canonical_kind,
        )
        .await?;

        for (j, p) in c.positions.iter().enumerate() {
            // A per-team raw surface form for the canonical, linked to it — so the
            // sweep resolves both teams' mentions to the one cluster.
            let entity_id = stable_uuid(&format!("divg-ent-{}-{j}", c.id));
            entities::insert_entity(
                &mut tx,
                entity_id,
                org_id,
                Some(team_id(&p.team)),
                &format!("{} ({})", c.canonical, p.team),
                &c.canonical_kind,
                &[],
                None,
            )
            .await?;
            entities::link(&mut tx, entity_id, canonical_id, 1.0, "human", None).await?;

            // The team's recorded practice, anchored to that entity. Org
            // visibility keeps the sweep deterministic (though it reads on the
            // RLS-bypassing pool regardless); the kind is in the sweep's gather set.
            let memory_id = stable_uuid(&format!("divg-mem-{}-{j}", c.id));
            memories::insert(
                &mut tx,
                &memories::NewMemory {
                    id: memory_id,
                    org_id,
                    team_id: Some(team_id(&p.team)),
                    owner_user_id: None,
                    visibility: Visibility::Org,
                    status: MemoryStatus::Canonical,
                    kind: MemoryKind::parse(&p.kind).unwrap_or(MemoryKind::Fact),
                    title: None,
                    content: p.content.clone(),
                    lifecycle: brainiac_core::Lifecycle::default(),
                    detail_md: None,
                    language: "en".into(),
                    valid_from: None,
                    valid_to: None,
                    superseded_by: None,
                    confidence: None,
                    provenance_id: None,
                    project_id: None,
                },
            )
            .await?;
            memories::link_entity(&mut tx, memory_id, entity_id).await?;
        }
    }

    tx.commit().await?;
    Ok(())
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
    use brainiac_gateway::ChatProvider;

    /// The oracle keys its verdict off the canonical name in the prompt and only
    /// fires on the team-axis system prompt — the two properties the DB-backed
    /// score depends on.
    #[tokio::test]
    async fn oracle_returns_gold_verdict_per_canonical() {
        let clusters: Vec<DivergenceClusterFx> = serde_json::from_value(serde_json::json!([
            { "id": "d1", "canonical": "retry-policy", "expected_divergence": true,
              "practice": "service retry policy",
              "positions": [
                  { "team": "payments", "kind": "decision", "content": "fixed 2s backoff" },
                  { "team": "platform", "kind": "decision", "content": "exponential + jitter" }
              ] },
            { "id": "d2", "canonical": "argocd", "expected_divergence": false, "practice": "",
              "positions": [
                  { "team": "payments", "kind": "howto", "content": "argocd app-of-apps" },
                  { "team": "platform", "kind": "howto", "content": "argocd auto-sync" }
              ] },
        ]))
        .expect("clusters");
        let mock = oracle_verdict_mock(&clusters);

        let ask = |canonical: &str| {
            ChatRequest {
            system: divergence::DIVERGENCE_SYSTEM_PROMPT_V1.to_string(),
            user: format!("System / concept: {canonical}\n\nWhat each team has recorded about it:\n- [payments] (decision) x"),
            json_mode: true,
            max_tokens: 100,
            temperature: 0.0,
        }
        };

        // Divergent gold → divergence:true with the gold practice.
        let r = mock.complete(&ask("retry-policy")).await.expect("complete");
        let v: serde_json::Value = serde_json::from_str(&r.text).expect("json");
        assert_eq!(v["divergence"], serde_json::json!(true));
        assert_eq!(v["practice"], serde_json::json!("service retry policy"));

        // Consistent gold → divergence:false (the trap the sweep must not flag).
        let r = mock.complete(&ask("argocd")).await.expect("complete");
        let v: serde_json::Value = serde_json::from_str(&r.text).expect("json");
        assert_eq!(v["divergence"], serde_json::json!(false));

        // A non-divergence system prompt is never flagged, whatever the body.
        let r = mock
            .complete(&ChatRequest {
                system: "some other classifier".into(),
                user: "System / concept: retry-policy\n...".into(),
                json_mode: true,
                max_tokens: 100,
                temperature: 0.0,
            })
            .await
            .expect("complete");
        let v: serde_json::Value = serde_json::from_str(&r.text).expect("json");
        assert_eq!(v["divergence"], serde_json::json!(false));
    }

    #[test]
    fn parse_canonical_reads_the_first_line() {
        let user = "System / concept: retry-policy-std\n\nWhat each team has recorded about it:\n- [payments] (decision) foo";
        assert_eq!(parse_canonical(user), Some("retry-policy-std"));
        assert_eq!(parse_canonical("no prefix here"), None);
    }

    #[test]
    fn cross_provider_baselines_are_refused() {
        let baseline = DivergenceBaseline {
            provider: "qwen:qwen-max".into(),
            fixture_version: "v1".into(),
            detection_recall: 1.0,
            detection_precision: 1.0,
            false_positive_rate: 0.0,
        };
        let report = DivergenceReport {
            fixture_version: "v1".into(),
            provider: "mock:deterministic".into(),
            gold_divergent: 2,
            gold_consistent: 2,
            clusters_scanned: 4,
            detected_divergent: 2,
            false_positives: 0,
            missed: 0,
            detection_recall: 1.0,
            detection_precision: 1.0,
            false_positive_rate: 0.0,
            practice_correct: 2,
            cases: vec![],
        };
        let f = regression_failures(&report, &baseline);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("provider mismatch"), "{f:?}");
    }
}
