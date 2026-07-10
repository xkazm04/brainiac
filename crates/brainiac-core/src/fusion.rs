//! Reciprocal Rank Fusion (retrieval stage 3, ARCHITECTURE.md §4).
//!
//! RRF is deliberately the only fusion strategy in v0: it is rank-based (no
//! score normalization across heterogeneous retrievers), well-studied, and has
//! a single tunable (`k`, conventionally 60).

use std::collections::HashMap;
use std::hash::Hash;

/// One ranked candidate list from a single retriever (best first).
pub type RankedList<T> = Vec<T>;

/// Fuse ranked lists via RRF: `score(d) = Σ_lists 1 / (k + rank(d))`, rank
/// 1-based. Returns items sorted by fused score descending; ties break by
/// first appearance across lists (stable, deterministic).
pub fn reciprocal_rank_fusion<T: Clone + Eq + Hash>(
    lists: &[RankedList<T>],
    k: f64,
    top: usize,
) -> Vec<(T, f64)> {
    let mut scores: HashMap<T, f64> = HashMap::new();
    let mut first_seen: HashMap<T, usize> = HashMap::new();
    let mut order = 0usize;

    for list in lists {
        for (rank0, item) in list.iter().enumerate() {
            let contribution = 1.0 / (k + (rank0 as f64 + 1.0));
            *scores.entry(item.clone()).or_insert(0.0) += contribution;
            first_seen.entry(item.clone()).or_insert_with(|| {
                order += 1;
                order
            });
        }
    }

    let mut fused: Vec<(T, f64)> = scores.into_iter().collect();
    fused.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| first_seen[&a.0].cmp(&first_seen[&b.0]))
    });
    fused.truncate(top);
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_in_both_lists_outranks_single_list_toppers() {
        // "c" is rank 2 in both lists; "a" and "x" are rank-1 in one list each.
        let vector = vec!["a", "c", "b"];
        let bm25 = vec!["x", "c", "y"];
        let fused = reciprocal_rank_fusion(&[vector, bm25], 60.0, 10);
        assert_eq!(fused[0].0, "c", "consensus beats single-list rank 1");
    }

    #[test]
    fn single_list_degenerates_to_input_order() {
        let only = vec!["a", "b", "c"];
        let fused = reciprocal_rank_fusion(std::slice::from_ref(&only), 60.0, 10);
        assert_eq!(fused.iter().map(|(i, _)| *i).collect::<Vec<_>>(), only);
    }

    #[test]
    fn truncates_to_top() {
        let l = vec![1, 2, 3, 4, 5];
        assert_eq!(reciprocal_rank_fusion(&[l], 60.0, 2).len(), 2);
    }

    #[test]
    fn deterministic_tie_break() {
        // "a" and "b" get identical scores (same rank in disjoint lists);
        // first-seen order must decide, stably.
        let l1 = vec!["a"];
        let l2 = vec!["b"];
        let fused = reciprocal_rank_fusion(&[l1, l2], 60.0, 10);
        assert_eq!(fused[0].0, "a");
        assert_eq!(fused[1].0, "b");
    }

    #[test]
    fn empty_input_is_empty_output() {
        let fused: Vec<(&str, f64)> = reciprocal_rank_fusion(&[], 60.0, 10);
        assert!(fused.is_empty());
    }
}
