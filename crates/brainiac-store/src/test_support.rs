//! Serialization for the Postgres integration-test binaries.
//!
//! Every `*_pg.rs` test binary in the workspace TRUNCATEs the one shared
//! database and re-seeds it, so two binaries running at once destroy each
//! other's fixtures mid-test. `cargo test` runs test binaries sequentially, so
//! a single `cargo test --workspace` is safe — but TWO of them are not, and two
//! is exactly what happens with parallel agent sessions on one machine, a local
//! run racing CI against a shared dev database, or a switch to a runner like
//! nextest that interleaves binaries. This bit us in practice (two concurrent
//! sessions, one database, flaky truncates).
//!
//! The fix serializes through the database itself, because the database is the
//! shared resource: one session-level advisory lock, taken once per test
//! binary on a dedicated single-connection pool and held until the process
//! exits. Whoever holds it owns the database; everyone else queues. On top of
//! that, an in-process mutex serializes the tests *within* a binary (the same
//! guard every test file previously hand-rolled).
//!
//! Usage, replacing each test file's hand-rolled `db_guard()`:
//!
//! ```ignore
//! let Some(url) = std::env::var("DATABASE_URL").ok() else { return };
//! let _guard = brainiac_store::test_support::serial_guard(&url).await;
//! ```
//!
//! This lives in the library (not a dev-dependency crate) because the other
//! crates' integration tests need to call it, and they already depend on
//! brainiac-store. It is inert in production: nothing outside `tests/` calls
//! it, and the lock key is namespaced to the test harness.

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::{Mutex, MutexGuard, OnceCell};

/// The advisory-lock key the whole test harness agrees on. Arbitrary but
/// fixed: every binary asks for the same lock, which is the point.
const TEST_HARNESS_LOCK_KEY: i64 = 0x00B1_21AC_7E57;

struct Harness {
    /// Holds the advisory lock for the life of the process. Never used for
    /// queries — it exists to keep one session open. Dropping it (process
    /// exit) releases the lock.
    _lock_holder: PgPool,
    /// Serializes tests within this binary.
    local: Mutex<()>,
}

static HARNESS: OnceCell<Harness> = OnceCell::const_new();

/// Acquire the cross-binary database lock (once per process, blocking until
/// any other test binary releases it) and the in-process test mutex (per
/// test). Hold the returned guard for the duration of the test.
pub async fn serial_guard(url: &str) -> MutexGuard<'static, ()> {
    let harness = HARNESS
        .get_or_init(|| async {
            init(url)
                .await
                .expect("acquiring the test-harness database lock")
        })
        .await;
    harness.local.lock().await
}

async fn init(url: &str) -> Result<Harness> {
    // A dedicated pool of exactly one connection: session advisory locks are
    // held by a session, and a shared pool could check the locking connection
    // back out to something else (or drop it under idle timeout), silently
    // releasing the lock while the binary still runs.
    let lock_holder = PgPoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .max_lifetime(None)
        .idle_timeout(None)
        .connect(url)
        .await
        .context("connecting the test-lock holder")?;

    // Blocks until the current holder (another binary, another session, CI)
    // finishes. That wait is the feature.
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(TEST_HARNESS_LOCK_KEY)
        .execute(&lock_holder)
        .await
        .context("taking the test-harness advisory lock")?;

    Ok(Harness {
        _lock_holder: lock_holder,
        local: Mutex::new(()),
    })
}
