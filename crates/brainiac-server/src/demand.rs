//! Recording the demand side (migration 0043).
//!
//! Every retrieval the product serves passes through one of three chokepoints —
//! `http::search`, `mcp::memory_search`, `mcp::memory_context` — and until now
//! none of them recorded that it happened. This module is the one place that
//! writes the ledger, so the three callers stay one line each.
//!
//! **The recording must never fail a retrieval.** The answer is the product;
//! the bookkeeping is not. So this follows the posture already established by
//! `record_library_usage_quietly` (mcp.rs) and `documents::record_read`: its own
//! transaction, warn on failure, return nothing. A caller cannot accidentally
//! propagate an error from it because it has no error to propagate.
//!
//! It also takes its own transaction for a second, less obvious reason:
//! `http::search` never commits the transaction it opened, so anything written
//! into that tx would be discarded on drop.

use brainiac_core::Principal;
use brainiac_store::retrieval_events::{record, NewRetrievalEvent};
use brainiac_store::Store;
use uuid::Uuid;

/// What was served, in rank order, as `(memory_id, score)`.
pub type ServedHits<'a> = &'a [(Uuid, Option<f64>)];

/// Append one retrieval to the demand ledger. Never fails the caller.
///
/// `team_id` is the principal's first team — the same convention
/// `record_library_usage_quietly` uses for `library_usage_events`, and the same
/// privacy shape: team, never person.
#[allow(clippy::too_many_arguments)]
pub async fn record_retrieval_quietly(
    store: &Store,
    principal: &Principal,
    token_id: Option<Uuid>,
    surface: &'static str,
    tool: &'static str,
    query: &str,
    served: ServedHits<'_>,
    withheld_contested: usize,
    latency_ms: u128,
) {
    let ev = NewRetrievalEvent {
        id: Uuid::new_v4(),
        org_id: principal.org_id,
        team_id: principal.team_ids.first().copied(),
        project_id: principal.project_id,
        token_id,
        surface,
        tool,
        query_text: query.to_string(),
        returned_n: served.len() as i32,
        // The best score of the set, not of the first element: graph extras can
        // outrank the vector head after fusion, and "did we answer this well?"
        // is a question about the SET the agent received.
        top_score: served
            .iter()
            .filter_map(|(_, s)| *s)
            .fold(None::<f64>, |acc, s| Some(acc.map_or(s, |a: f64| a.max(s)))),
        withheld_contested: withheld_contested as i32,
        latency_ms: i32::try_from(latency_ms).ok(),
        served: served
            .iter()
            .enumerate()
            .map(|(i, (id, score))| (*id, i as i32, *score))
            .collect(),
    };

    let outcome = async {
        let mut tx = store.scoped_tx(principal).await?;
        record(&mut tx, &ev).await?;
        tx.commit().await?;
        anyhow::Ok(())
    }
    .await;

    if let Err(e) = outcome {
        // Warn, never propagate: a ledger write that fails must not turn a good
        // answer into a 500. It is visible in logs and absent from the ledger,
        // which is the honest failure mode — the alternative (a silent retry
        // that eventually succeeds out of order) would corrupt the window
        // aggregates that read this table.
        tracing::warn!(error = %e, surface, tool, "demand ledger write failed (answer unaffected)");
    }
}
