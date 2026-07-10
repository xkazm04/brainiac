//! brainiac-fixtures — the Meridian golden-fixture suite (EVAL.md).
//!
//! Loads `fixtures/v1/` YAML into typed structs and validates the whole tree
//! for referential integrity — every dangling id, invisible gold item, or
//! vacuous leak test is a loader error, so fixture bugs die in CI before they
//! corrupt a benchmark run.

pub mod ids;
pub mod loader;
pub mod schema;
pub mod validate;

pub use loader::{load, Fixtures};
pub use schema::*;
