//! brainiac-store — the Postgres data plane.
//!
//! Connection discipline (the RLS load-bearing detail):
//! - `migrate(url)` runs as the connecting (admin) user — DDL + role grants.
//! - `Store::connect(url)` builds the runtime pool with an `after_connect`
//!   hook that drops every session to the non-owner `brainiac_app` role, so
//!   RLS policies apply to ALL runtime queries (superusers bypass RLS; the
//!   app role never can).
//! - `scoped_tx(principal)` opens a transaction with `app.org_id` /
//!   `app.user_id` set LOCAL from the verified principal. Every read/write
//!   goes through such a transaction; there is no unscoped query path.

pub mod entities;
pub mod memories;
pub mod orgs;
pub mod queue;
pub mod retrieval;

use anyhow::{Context, Result};
use brainiac_core::Principal;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, Executor, PgPool, Postgres, Transaction};
use std::str::FromStr;

pub type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

/// Run migrations as the admin user (the one in the URL). Separate from the
/// runtime pool so DDL never runs under the constrained role.
pub async fn migrate(database_url: &str) -> Result<()> {
    let opts =
        PgConnectOptions::from_str(database_url)?.log_statements(tracing::log::LevelFilter::Debug);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .context("connecting for migration")?;
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .context("running migrations")?;
    pool.close().await;
    Ok(())
}

impl Store {
    /// Runtime pool: every session is demoted to `brainiac_app` on connect.
    pub async fn connect(database_url: &str) -> Result<Self> {
        let opts = PgConnectOptions::from_str(database_url)?
            .log_statements(tracing::log::LevelFilter::Debug);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    conn.execute("SET ROLE brainiac_app").await?;
                    Ok(())
                })
            })
            .connect_with(opts)
            .await
            .context("connecting runtime pool")?;
        Ok(Self { pool })
    }

    /// Open a transaction scoped to the principal. `set_config(..., true)` is
    /// transaction-local, so scopes can never bleed across pooled sessions.
    pub async fn scoped_tx(&self, principal: &Principal) -> Result<Tx<'static>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "SELECT set_config('app.org_id', $1, true), set_config('app.user_id', $2, true)",
        )
        .bind(principal.org_id.to_string())
        .bind(principal.user_id.to_string())
        .execute(&mut *tx)
        .await?;
        Ok(tx)
    }

    /// Raw pool access for non-tenant subsystems (the job queue).
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
