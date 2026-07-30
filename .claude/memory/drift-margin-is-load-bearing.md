# `DRIFT_MARGIN = 0.05` is load-bearing and sits near its edge on a real embedder

`crates/brainiac-eval/src/drift_profile.rs` defines
`pub const DRIFT_MARGIN: f64 = 0.05;` — the similarity margin by which a stale
claim must beat the current one before the drift detector calls it drift.

Calibrated against the real Qwen embedder (not the deterministic one), the
decisive fixture — the "10s timeout" stale claim — decides at **0.970 stale vs
0.907 fresh, a 0.063 gap**. That is 0.013 of headroom over the constant. Any
change to `DRIFT_MARGIN` is therefore **gate-affecting**, and drift results on
prose whose wording is far from the underlying memories should not be trusted
until the near-miss corpus is grown. Treat a tweak here like a baseline
recalibration: deliberate, measured on the real embedder, and committed with
the eval diff.
