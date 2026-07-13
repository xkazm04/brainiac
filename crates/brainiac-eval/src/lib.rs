//! brainiac-eval — the golden-fixture eval harness (EVAL.md §3).
//!
//! Profiles:
//! - `retrieval`: seeds GOLD memories directly (isolating retrieval quality
//!   from extraction noise), runs the QA + temporal + leak suites, scores
//!   NDCG/MRR/Recall per stratum, and evaluates the hard gates (RLS leaks = 0).
//! - `resolution`: seeds the gold RAW entities (without their canonical links),
//!   runs the real resolve stage over them with an oracle adjudicator, and
//!   scores the predicted clustering (B³/pairwise/false-merge) against gold.
//! - `pipeline` (P5): raw transcripts in -> extraction/resolution scored
//!   against gold.
//! - `contradiction`: seeds the gold contradiction pairs into isolated orgs,
//!   runs the real contradict stage with a gold-oracle verdict mock, and scores
//!   detection recall/precision, false-positive rate, and direction accuracy.
//!
//! Alongside the profiles, `grid` is the §3.1 bake-off driver: it runs the
//! retrieval profile across the available backend cross-product and emits one
//! exploratory decision-table artifact (no gates).

pub mod contradiction_profile;
pub mod gates;
pub mod grid;
pub mod pipeline_profile;
pub mod report;
pub mod resolution_profile;
pub mod retrieval_profile;
pub mod seed;

pub use contradiction_profile::ContradictionReport;
pub use pipeline_profile::PipelineReport;
pub use report::RetrievalReport;
pub use resolution_profile::ResolutionReport;
