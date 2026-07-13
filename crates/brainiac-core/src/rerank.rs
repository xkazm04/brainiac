//! Reranking runtime seam (ARCHITECTURE.md §4 stage 5).
//!
//! Stage 5 is the cross-encoder rerank: after the candidate lists are fused,
//! graph-expanded, assembled and deduped (≤40 survivors), a reranker scores
//! each surviving `(query, candidate-text)` PAIR jointly — the thing a
//! bi-encoder embedding can't do, because it never sees query and document
//! together. Those scores then feed the final blend (recency/feedback nudges)
//! and truncation in [`crate::scoring`], exactly like the fused relevance they
//! replace.
//!
//! This module is the SEAM only. The production cross-encoder (an ONNX
//! model — bge-reranker, mxbai-rerank…) plugs in behind [`Reranker`] for the
//! bake-off (EVAL.md §3.1); it is out of scope here. The default is *no
//! reranker at all*: retrieval that passes `None` is byte-identical to the
//! pre-stage-5 behavior. [`LexicalOverlapReranker`] is a deterministic,
//! model-free implementation used to prove that reordering actually flows
//! through the stage-5 slot (unit + pg tests) and to exercise the bake-off
//! plumbing without a model download.

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Joint `(query, candidate)` relevance scorer — ARCHITECTURE.md §4 stage 5.
///
/// Async + fallible for the same reason as [`crate::embed::Embedder`]: a real
/// cross-encoder is a model-runtime (ONNX) or remote call. Implementations
/// MUST be deterministic for the same input (eval replay and test stability
/// depend on it) and MUST return exactly one score per candidate, in the SAME
/// order as `candidates`. Higher scores are more relevant; the absolute scale
/// is the implementation's own — the caller only orders by it, so a reranker is
/// free to emit logits, cosine-like values, or overlap counts.
///
/// The signature takes the candidate id alongside the text purely so an
/// implementation may key its own caches/telemetry on stable ids; scores are
/// returned positionally (a `Vec<f32>` aligned to `candidates`) rather than as
/// a map so the contract "one score per candidate" is total and un-loseable.
#[async_trait]
pub trait Reranker: Send + Sync {
    fn model_name(&self) -> &str;

    /// Score every candidate for `query`. Returns one score per candidate,
    /// aligned to the input order. An empty `candidates` slice returns an empty
    /// `Vec` without any model call.
    async fn rerank(&self, query: &str, candidates: &[(Uuid, &str)]) -> Result<Vec<f32>>;
}

/// Lowercase alphanumeric tokens of length > 1 — the same tokenization the
/// deterministic embedder uses, so the two model-free seams agree on what a
/// "token" is.
fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|t| t.to_string())
        .collect()
}

/// Deterministic, model-free reranker: scores a candidate by the fraction of
/// the query's distinct tokens it contains (Jaccard-style overlap, query-side
/// normalized). Zero downloads, identical results on every machine — enough
/// joint-overlap signal to prove that stage-5 reordering flows end-to-end and
/// to stand in for the real cross-encoder while the bake-off plumbing is built.
/// Its numbers are plumbing numbers, not a quality claim (mirrors
/// [`crate::embed::DeterministicEmbedder`]).
pub struct LexicalOverlapReranker;

impl LexicalOverlapReranker {
    pub const MODEL_NAME: &'static str = "lexical-overlap-v1";

    /// Pure synchronous core — usable without a runtime (unit tests, tools).
    pub fn score(query: &str, candidate: &str) -> f32 {
        let q: std::collections::HashSet<String> = tokens(query).into_iter().collect();
        if q.is_empty() {
            return 0.0;
        }
        let c: std::collections::HashSet<String> = tokens(candidate).into_iter().collect();
        let overlap = q.iter().filter(|t| c.contains(*t)).count();
        overlap as f32 / q.len() as f32
    }
}

#[async_trait]
impl Reranker for LexicalOverlapReranker {
    fn model_name(&self) -> &str {
        Self::MODEL_NAME
    }

    async fn rerank(&self, query: &str, candidates: &[(Uuid, &str)]) -> Result<Vec<f32>> {
        Ok(candidates
            .iter()
            .map(|(_, text)| Self::score(query, text))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    /// Drive an immediately-ready future to completion with no runtime — this
    /// crate stays IO-free, so its tests carry no async executor (mirrors
    /// `embed`'s sync-core testing). The reranker future never pends.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    #[test]
    fn full_overlap_scores_higher_than_partial_than_none() {
        let q = "refund worker retry cap";
        let full = LexicalOverlapReranker::score(q, "the refund worker retry cap was raised");
        let partial = LexicalOverlapReranker::score(q, "the refund worker was restarted");
        let none = LexicalOverlapReranker::score(q, "kafka consumer lag on the data plane");
        assert!(full > partial, "{full} !> {partial}");
        assert!(partial > none, "{partial} !> {none}");
        assert_eq!(none, 0.0);
    }

    #[test]
    fn score_is_query_normalized_and_bounded() {
        let s = LexicalOverlapReranker::score("alpha beta", "alpha beta gamma delta epsilon");
        assert!((s - 1.0).abs() < 1e-6, "all query tokens present ⇒ 1.0");
        let empty_query = LexicalOverlapReranker::score("", "anything at all");
        assert_eq!(empty_query, 0.0, "empty query is a safe zero");
    }

    #[test]
    fn rerank_is_aligned_and_deterministic() {
        let r = LexicalOverlapReranker;
        let cands = [
            (uuid(1), "no shared words here"),
            (uuid(2), "retry cap tuning notes"),
            (uuid(3), "the worker retry cap"),
        ];
        let a = block_on(r.rerank("worker retry cap", &cands)).expect("rerank");
        let b = block_on(r.rerank("worker retry cap", &cands)).expect("rerank");
        assert_eq!(a, b, "deterministic across calls");
        assert_eq!(a.len(), cands.len(), "one score per candidate, in order");
        // Candidate 3 contains all three query tokens; candidate 1 none.
        assert!(a[2] > a[1], "fuller overlap ranks higher");
        assert!(a[1] > a[0]);
        assert_eq!(a[0], 0.0);
    }

    #[test]
    fn empty_candidates_returns_empty() {
        let r = LexicalOverlapReranker;
        let out = block_on(r.rerank("anything", &[])).expect("rerank");
        assert!(out.is_empty());
    }
}
