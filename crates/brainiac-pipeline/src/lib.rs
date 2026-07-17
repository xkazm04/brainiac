//! brainiac-pipeline — the ingest -> extract -> embed -> resolve ->
//! contradict -> promote chain (ARCHITECTURE.md section 3).
//!
//! v0 topology: one `ingest` queue; a claimed job runs the full stage chain
//! for its source inside the worker. The stages remain separate functions
//! with the architecture's contracts, so splitting them across queues later
//! is a worker-loop change, not a redesign.

pub mod compose;
pub mod contradict;
pub mod divergence;
pub mod extract;
pub mod faithfulness;
pub mod library_sweep;
pub mod manual;
pub mod okf_ingest;
pub mod policy;
pub mod reembed;
pub mod resolve;
pub mod standards_page;
pub mod worker;

use brainiac_core::Principal;

/// The pipeline acts as an org-scoped synthetic principal: broad enough to
/// write raw knowledge for any team of the org (INSERT policies are
/// org-checked), never used for end-user retrieval.
pub fn pipeline_principal(org_id: uuid::Uuid) -> Principal {
    Principal {
        org_id,
        user_id: uuid::Uuid::from_bytes(*b"brainiac-worker!"),
        team_ids: vec![],
    }
}
