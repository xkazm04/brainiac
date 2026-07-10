//! Evaluation metrics (EVAL.md §2.2, §2.5): NDCG@k, MRR, Recall@k for
//! retrieval; pairwise precision/recall/F1 and B³ F1 for entity-resolution
//! clustering. One implementation shared by the eval harness and any future
//! runtime analytics — no reimplementation drift.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

// ---------------------------------------------------------------------------
// Retrieval metrics
// ---------------------------------------------------------------------------

/// Discounted Cumulative Gain at k over graded relevance (grades 0..=3).
/// `ranked` is the system output (best first); `grades` maps item → grade.
fn dcg_at_k<T: Eq + Hash>(ranked: &[T], grades: &HashMap<T, u8>, k: usize) -> f64 {
    ranked
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, item)| {
            let g = *grades.get(item).unwrap_or(&0) as f64;
            let gain = 2f64.powf(g) - 1.0;
            gain / ((i as f64 + 2.0).log2())
        })
        .sum()
}

/// NDCG@k. Returns 1.0 for queries with no relevant items only when the
/// system also returns nothing relevant-graded — by convention we return
/// `None` for no-relevant queries so callers can exclude them (the negative
/// stratum is scored by refusal quality instead, not NDCG).
pub fn ndcg_at_k<T: Clone + Eq + Hash>(
    ranked: &[T],
    grades: &HashMap<T, u8>,
    k: usize,
) -> Option<f64> {
    if grades.values().all(|g| *g == 0) || grades.is_empty() {
        return None;
    }
    let mut ideal: Vec<(&T, &u8)> = grades.iter().filter(|(_, g)| **g > 0).collect();
    ideal.sort_by(|a, b| b.1.cmp(a.1));
    let ideal_ranked: Vec<T> = ideal.into_iter().map(|(t, _)| t.clone()).collect();
    let idcg = dcg_at_k(&ideal_ranked, grades, k);
    if idcg == 0.0 {
        return None;
    }
    Some(dcg_at_k(ranked, grades, k) / idcg)
}

/// Mean Reciprocal Rank contribution for one query: 1/rank of the first item
/// with grade > 0, else 0.
pub fn reciprocal_rank<T: Eq + Hash>(ranked: &[T], grades: &HashMap<T, u8>) -> f64 {
    for (i, item) in ranked.iter().enumerate() {
        if grades.get(item).copied().unwrap_or(0) > 0 {
            return 1.0 / (i as f64 + 1.0);
        }
    }
    0.0
}

/// Recall@k: fraction of relevant (grade > 0) items present in the top k.
pub fn recall_at_k<T: Eq + Hash>(ranked: &[T], grades: &HashMap<T, u8>, k: usize) -> Option<f64> {
    let relevant: HashSet<&T> = grades
        .iter()
        .filter(|(_, g)| **g > 0)
        .map(|(t, _)| t)
        .collect();
    if relevant.is_empty() {
        return None;
    }
    let hit = ranked
        .iter()
        .take(k)
        .filter(|i| relevant.contains(i))
        .count();
    Some(hit as f64 / relevant.len() as f64)
}

// ---------------------------------------------------------------------------
// Clustering metrics (entity resolution)
// ---------------------------------------------------------------------------

/// A clustering: item → cluster id.
pub type Clustering<T> = HashMap<T, usize>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrecisionRecallF1 {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

fn f1(p: f64, r: f64) -> f64 {
    if p + r == 0.0 {
        0.0
    } else {
        2.0 * p * r / (p + r)
    }
}

/// Pairwise P/R/F1: over all unordered item pairs, a pair is positive when
/// both clusterings put the two items together.
pub fn pairwise_prf<T: Clone + Eq + Hash + Ord>(
    predicted: &Clustering<T>,
    gold: &Clustering<T>,
) -> PrecisionRecallF1 {
    let items: Vec<&T> = gold.keys().collect();
    let mut tp = 0u64;
    let mut fp = 0u64;
    let mut fn_ = 0u64;
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            let (a, b) = (items[i], items[j]);
            let gold_same = gold.get(a) == gold.get(b);
            let pred_same = match (predicted.get(a), predicted.get(b)) {
                (Some(x), Some(y)) => x == y,
                _ => false, // unclustered prediction = "not same"
            };
            match (pred_same, gold_same) {
                (true, true) => tp += 1,
                (true, false) => fp += 1,
                (false, true) => fn_ += 1,
                (false, false) => {}
            }
        }
    }
    let p = if tp + fp == 0 {
        1.0
    } else {
        tp as f64 / (tp + fp) as f64
    };
    let r = if tp + fn_ == 0 {
        1.0
    } else {
        tp as f64 / (tp + fn_) as f64
    };
    PrecisionRecallF1 {
        precision: p,
        recall: r,
        f1: f1(p, r),
    }
}

/// B³ (B-cubed) P/R/F1: per-item precision/recall of its predicted cluster
/// against its gold cluster, averaged over items. Standard for entity
/// resolution because it weights per mention, not per pair.
pub fn b_cubed<T: Clone + Eq + Hash>(
    predicted: &Clustering<T>,
    gold: &Clustering<T>,
) -> PrecisionRecallF1 {
    let items: Vec<&T> = gold.keys().collect();
    if items.is_empty() {
        return PrecisionRecallF1 {
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
        };
    }

    // Materialize cluster membership sets.
    let mut pred_clusters: HashMap<usize, HashSet<&T>> = HashMap::new();
    for item in &items {
        if let Some(c) = predicted.get(item) {
            pred_clusters.entry(*c).or_default().insert(item);
        }
    }
    let mut gold_clusters: HashMap<usize, HashSet<&T>> = HashMap::new();
    for item in &items {
        if let Some(c) = gold.get(item) {
            gold_clusters.entry(*c).or_default().insert(item);
        }
    }

    let mut p_sum = 0.0;
    let mut r_sum = 0.0;
    for item in &items {
        let gold_set = gold.get(item).and_then(|c| gold_clusters.get(c));
        let pred_set = predicted.get(item).and_then(|c| pred_clusters.get(c));
        match (pred_set, gold_set) {
            (Some(ps), Some(gs)) => {
                let overlap = ps.intersection(gs).count() as f64;
                p_sum += overlap / ps.len() as f64;
                r_sum += overlap / gs.len() as f64;
            }
            (None, Some(gs)) => {
                // Unclustered prediction = singleton: precision 1, recall 1/|gold|.
                p_sum += 1.0;
                r_sum += 1.0 / gs.len() as f64;
            }
            _ => {}
        }
    }
    let p = p_sum / items.len() as f64;
    let r = r_sum / items.len() as f64;
    PrecisionRecallF1 {
        precision: p,
        recall: r,
        f1: f1(p, r),
    }
}

/// Count of predicted-same pairs that gold forbids (the zero-tolerance CI
/// gate: false merges silently corrupt the graph).
pub fn false_merge_count<T: Clone + Eq + Hash + Ord>(
    predicted: &Clustering<T>,
    forbidden_pairs: &[(T, T)],
) -> usize {
    forbidden_pairs
        .iter()
        .filter(|(a, b)| match (predicted.get(a), predicted.get(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grades(pairs: &[(&'static str, u8)]) -> HashMap<&'static str, u8> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn ndcg_perfect_ranking_is_one() {
        let g = grades(&[("a", 3), ("b", 2), ("c", 1)]);
        let ndcg = ndcg_at_k(&["a", "b", "c"], &g, 10).expect("has relevant");
        assert!((ndcg - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ndcg_penalizes_inverted_ranking() {
        let g = grades(&[("a", 3), ("b", 1)]);
        let good = ndcg_at_k(&["a", "b"], &g, 10).expect("some");
        let bad = ndcg_at_k(&["b", "a"], &g, 10).expect("some");
        assert!(good > bad);
        assert!(bad > 0.0);
    }

    #[test]
    fn ndcg_none_for_no_relevant_queries() {
        let g: HashMap<&str, u8> = HashMap::new();
        assert!(ndcg_at_k(&["a"], &g, 10).is_none());
    }

    #[test]
    fn mrr_and_recall() {
        let g = grades(&[("x", 2), ("y", 1)]);
        assert!((reciprocal_rank(&["a", "x", "y"], &g) - 0.5).abs() < 1e-9);
        assert!((recall_at_k(&["a", "x"], &g, 2).expect("some") - 0.5).abs() < 1e-9);
        assert!((recall_at_k(&["x", "y"], &g, 2).expect("some") - 1.0).abs() < 1e-9);
        assert_eq!(reciprocal_rank(&["a", "b"], &g), 0.0);
    }

    fn clustering(groups: &[&[&'static str]]) -> Clustering<&'static str> {
        let mut c = HashMap::new();
        for (i, group) in groups.iter().enumerate() {
            for item in *group {
                c.insert(*item, i);
            }
        }
        c
    }

    #[test]
    fn perfect_clustering_scores_one() {
        let gold = clustering(&[&["a", "b"], &["c"]]);
        let pred = clustering(&[&["a", "b"], &["c"]]);
        let pw = pairwise_prf(&pred, &gold);
        let b3 = b_cubed(&pred, &gold);
        assert!((pw.f1 - 1.0).abs() < 1e-9);
        assert!((b3.f1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn over_merge_hurts_precision_under_merge_hurts_recall() {
        let gold = clustering(&[&["a", "b"], &["c", "d"]]);
        // Over-merge: everything in one cluster → recall 1, precision < 1.
        let over = clustering(&[&["a", "b", "c", "d"]]);
        let pw = pairwise_prf(&over, &gold);
        assert!((pw.recall - 1.0).abs() < 1e-9);
        assert!(pw.precision < 1.0);
        // Under-merge: all singletons → precision 1 (vacuous), recall < 1.
        let under = clustering(&[&["a"], &["b"], &["c"], &["d"]]);
        let pw2 = pairwise_prf(&under, &gold);
        assert!((pw2.precision - 1.0).abs() < 1e-9);
        assert!(pw2.recall < 1.0);
    }

    #[test]
    fn b_cubed_matches_known_example() {
        // Gold: {a,b,c}, {d}. Pred: {a,b}, {c,d}.
        // Per item: a: p=1, r=2/3 · b: p=1, r=2/3 · c: p=1/2, r=1/3 · d: p=1/2, r=1
        let gold = clustering(&[&["a", "b", "c"], &["d"]]);
        let pred = clustering(&[&["a", "b"], &["c", "d"]]);
        let b3 = b_cubed(&pred, &gold);
        assert!((b3.precision - 0.75).abs() < 1e-9);
        assert!((b3.recall - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn false_merges_counted() {
        let pred = clustering(&[&["repo", "artifact"], &["x"]]);
        let forbidden = vec![("repo", "artifact")];
        assert_eq!(false_merge_count(&pred, &forbidden), 1);
        let ok = clustering(&[&["repo"], &["artifact"]]);
        assert_eq!(false_merge_count(&ok, &forbidden), 0);
    }
}
