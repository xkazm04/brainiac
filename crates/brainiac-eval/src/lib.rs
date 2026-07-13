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

pub mod gates;
pub mod report;
pub mod resolution_profile;
pub mod retrieval_profile;
pub mod seed;

pub use report::RetrievalReport;
pub use resolution_profile::ResolutionReport;
