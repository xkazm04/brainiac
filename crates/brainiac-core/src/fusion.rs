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

/// Weighted RRF: `score(d) = Σ_lists weights[i] / (k + rank(d))`. Identical to
/// [`reciprocal_rank_fusion`] when every weight is 1.0, so it is a strict
/// generalization — plain RRF stays available for callers that want the
/// unweighted contract. A list whose index is beyond `weights` defaults to
/// weight 1.0. Same deterministic tie-break (first appearance across lists).
///
/// The retrieval engine uses this to bias fusion toward the lexical list for
/// identifier-heavy queries (see [`query_is_identifier_heavy`]).
pub fn weighted_reciprocal_rank_fusion<T: Clone + Eq + Hash>(
    lists: &[RankedList<T>],
    weights: &[f64],
    k: f64,
    top: usize,
) -> Vec<(T, f64)> {
    let mut scores: HashMap<T, f64> = HashMap::new();
    let mut first_seen: HashMap<T, usize> = HashMap::new();
    let mut order = 0usize;

    for (li, list) in lists.iter().enumerate() {
        let weight = weights.get(li).copied().unwrap_or(1.0);
        for (rank0, item) in list.iter().enumerate() {
            let contribution = weight / (k + (rank0 as f64 + 1.0));
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

// ── query understanding: exact-identifier detection (ARCHITECTURE.md §4) ──
//
// Repo names, service names, dotted config paths and error codes are exact
// tokens: a reader who types `psp-gateway` or `E4012` wants that literal, and
// dense embeddings blur it into a neighborhood. Detecting these lets the
// retrieval engine lean on the lexical (FTS) list instead of the vector list.

/// True when a token looks like a code identifier rather than a natural word:
/// an internal separator joining alphanumerics (`psp-gateway`, `refund_worker`,
/// `checkout.events.v2`), a CamelCase hump (`QwenEmbedder`, `checkoutV2`), or an
/// error-code shape — a letter-then-digit run of length ≥ 4 (`E4012`, `HTTP500`).
/// Every branch requires at least one letter, so bare numbers and decimals
/// (`2026`, `3.5`) are not identifiers.
pub fn is_identifier_token(token: &str) -> bool {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    if t.len() < 2 || !t.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    let chars: Vec<char> = t.chars().collect();

    // (a) internal separator (-, _, .) joining two alphanumeric runs.
    for i in 1..chars.len().saturating_sub(1) {
        if matches!(chars[i], '-' | '_' | '.')
            && chars[i - 1].is_alphanumeric()
            && chars[i + 1].is_alphanumeric()
        {
            return true;
        }
    }

    // (b) CamelCase hump: a lowercase immediately followed by an uppercase.
    for w in chars.windows(2) {
        if w[0].is_lowercase() && w[1].is_uppercase() {
            return true;
        }
    }

    // (c) error-code shape: all alphanumeric, has a letter→digit boundary,
    // length ≥ 4 (so short version tags like `v2` don't qualify).
    if t.len() >= 4 && chars.iter().all(|c| c.is_alphanumeric()) {
        for w in chars.windows(2) {
            if w[0].is_ascii_alphabetic() && w[1].is_ascii_digit() {
                return true;
            }
        }
    }

    false
}

/// True when a query carries at least one identifier token — the signal that
/// exact lexical matching should lead the fusion. Whitespace-tokenized;
/// per-token punctuation is trimmed by [`is_identifier_token`].
pub fn query_is_identifier_heavy(query: &str) -> bool {
    query.split_whitespace().any(is_identifier_token)
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

    #[test]
    fn weighted_unit_weights_equal_plain_rrf() {
        let vector = vec!["a", "c", "b"];
        let bm25 = vec!["x", "c", "y"];
        let plain = reciprocal_rank_fusion(&[vector.clone(), bm25.clone()], 60.0, 10);
        let weighted = weighted_reciprocal_rank_fusion(&[vector, bm25], &[1.0, 1.0], 60.0, 10);
        assert_eq!(
            plain, weighted,
            "weight 1.0 must reproduce plain RRF exactly"
        );
    }

    #[test]
    fn weighting_the_second_list_lifts_its_topper() {
        // "v" leads the vector list, "f" leads the fts list; both are rank 1 in
        // their own list and absent from the other. Unweighted they tie and
        // first-seen ("v") wins. Weighting the fts list flips the order.
        let vector = vec!["v"];
        let fts = vec!["f"];
        let plain = reciprocal_rank_fusion(&[vector.clone(), fts.clone()], 60.0, 10);
        assert_eq!(plain[0].0, "v", "tie breaks to first-seen without weights");
        let weighted = weighted_reciprocal_rank_fusion(&[vector, fts], &[1.0, 3.0], 60.0, 10);
        assert_eq!(
            weighted[0].0, "f",
            "heavier fts weight ranks its item first"
        );
    }

    #[test]
    fn missing_weight_defaults_to_one() {
        let a = vec![1, 2];
        let b = vec![3, 4];
        let with = weighted_reciprocal_rank_fusion(&[a.clone(), b.clone()], &[1.0], 60.0, 10);
        let plain = reciprocal_rank_fusion(&[a, b], 60.0, 10);
        assert_eq!(with, plain, "absent weight falls back to 1.0");
    }

    #[test]
    fn identifier_tokens_detected() {
        for id in [
            "psp-gateway",
            "refund_worker",
            "checkout.events.v2",
            "QwenEmbedder",
            "checkoutV2",
            "E4012",
            "HTTP500",
        ] {
            assert!(is_identifier_token(id), "{id} should read as an identifier");
        }
    }

    #[test]
    fn plain_words_are_not_identifiers() {
        for word in ["gateway", "retry", "the", "v2", "2026", "3.5", "checkout"] {
            assert!(!is_identifier_token(word), "{word} is not an identifier");
        }
    }

    #[test]
    fn punctuation_is_trimmed_before_detection() {
        assert!(is_identifier_token("(psp-gateway)"));
        assert!(is_identifier_token("E4012,"));
        assert!(!is_identifier_token("gateway."));
    }

    #[test]
    fn query_identifier_heaviness() {
        assert!(query_is_identifier_heavy("why does refund-worker time out"));
        assert!(query_is_identifier_heavy("error E4012 on deploy"));
        assert!(!query_is_identifier_heavy(
            "how do we handle checkout latency"
        ));
    }
}
