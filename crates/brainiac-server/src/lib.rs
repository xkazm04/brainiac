//! brainiac-server library surface: auth stub + HTTP router. The `brainiac`
//! binary (main.rs) wires these into serve/worker/eval subcommands; tests
//! boot the same router directly.

pub mod alerts;
pub mod auth;
pub mod console;
pub mod docs;
pub mod http;
pub mod mcp;
pub mod openapi;
pub mod provision;
pub mod sweeps;
