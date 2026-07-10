//! brainiac-server library surface: auth stub + HTTP router. The `brainiac`
//! binary (main.rs) wires these into serve/worker/eval subcommands; tests
//! boot the same router directly.

pub mod auth;
pub mod http;
pub mod mcp;
