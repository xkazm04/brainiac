//! brainiac-server library surface: auth stub + HTTP router. The `brainiac`
//! binary (main.rs) wires these into serve/worker/eval subcommands; tests
//! boot the same router directly.

pub mod alerts;
pub mod auth;
pub mod console;
pub mod docs;
pub mod http;
pub mod library;
pub mod mcp;
pub mod onboard;
pub mod openapi;
pub mod projects;
pub mod provision;
pub mod sweeps;
