//! brainiac-core — domain types and pure algorithms.
//!
//! No IO in this crate: everything here is deterministic and unit-testable
//! without Postgres or an LLM. The store, pipeline, and eval crates build on
//! these primitives so quality-critical logic (temporal validity, rank
//! fusion, scoring metrics) has exactly one implementation.

pub mod embed;
pub mod fusion;
pub mod metrics;
pub mod rerank;
pub mod scoring;
pub mod temporal;
pub mod types;

pub use types::*;
