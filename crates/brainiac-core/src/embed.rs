//! Embedding runtime seam.
//!
//! Real open models (bge-m3, gte, nomic…) plug in behind [`Embedder`] for the
//! bake-off (EVAL.md §3.1). The default v0 implementation is a deterministic
//! hashed bag-of-tokens projection: zero model downloads, identical vectors on
//! every machine, and enough token-overlap signal to exercise the entire
//! hybrid-retrieval + eval plumbing. Its metric numbers are plumbing numbers,
//! not quality claims (PLAN.md §Deviations #4).

/// Text → unit-length vector. Implementations must be deterministic for the
/// same input (retrieval tests and fixture replay depend on it).
pub trait Embedder: Send + Sync {
    fn model_name(&self) -> &str;
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Vec<f32>;

    fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// FNV-1a 64-bit — tiny, stable, no dependency.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// Deterministic hashed bag-of-tokens embedder.
///
/// Each lowercase alphanumeric token hashes to a bucket and a sign; the
/// resulting sparse vector is L2-normalized. Cosine similarity then reflects
/// token overlap (with a little collision noise) — sufficient for identifier
/// matching and near-duplicate text, weak on true paraphrase (by design; that
/// gap is what the real-model bake-off measures).
pub struct DeterministicEmbedder {
    dim: usize,
}

impl DeterministicEmbedder {
    pub const DEFAULT_DIM: usize = 256;

    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl Default for DeterministicEmbedder {
    fn default() -> Self {
        Self::new(Self::DEFAULT_DIM)
    }
}

impl Embedder for DeterministicEmbedder {
    fn model_name(&self) -> &str {
        "deterministic-bow-v1"
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0f32; self.dim];
        for token in text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() > 1)
        {
            let h = fnv1a(token.as_bytes());
            let bucket = (h % self.dim as u64) as usize;
            let sign = if (h >> 63) == 0 { 1.0 } else { -1.0 };
            v[bucket] += sign;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }
}

/// Cosine similarity between two vectors (assumed same dim).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_across_calls() {
        let e = DeterministicEmbedder::default();
        assert_eq!(
            e.embed("retry backoff for psp-gateway"),
            e.embed("retry backoff for psp-gateway")
        );
    }

    #[test]
    fn unit_length() {
        let e = DeterministicEmbedder::default();
        let v = e.embed("kafka consumer lag");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn overlapping_text_is_more_similar_than_disjoint() {
        let e = DeterministicEmbedder::default();
        let a = e.embed("refund-worker retry cap causes timeout storms against psp-gateway");
        let b = e.embed("raise the refund-worker retry cap against psp-gateway latency");
        let c = e.embed("sigma webgl graph layout rendering in the browser console");
        assert!(cosine(&a, &b) > cosine(&a, &c));
    }

    #[test]
    fn empty_text_is_zero_vector() {
        let e = DeterministicEmbedder::default();
        let v = e.embed("   ");
        assert!(v.iter().all(|x| *x == 0.0));
    }
}
