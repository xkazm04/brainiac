//! Product-funnel analytics (Plausible Events API) — the SERVER half.
//!
//! WHY THIS EXISTS AT ALL, GIVEN THE CONSOLE ALREADY HAS ONE
//! ---------------------------------------------------------
//! The launch funnel is `landing_view → signup_complete → activation`, and the
//! console instruments all three (`console/src/analytics/`). But the console can
//! only observe an activation it performs itself, and the CLOUD-SIGNUP user
//! never touches the console again after signing up: `/v1/provision` hands them a
//! project and a scoped device key, and their local agent then POSTs straight to
//! `/v1/memories` here. Signup grants no console access. So for the exact cohort
//! this launch is built around, `console/src/lib/api.ts`'s `trackActivation`
//! cannot fire — the funnel would show sign-ups converting to nothing forever.
//! This module closes that hole at the only place that sees the write.
//!
//! WHAT IT SENDS, AND WHAT IT DELIBERATELY DOES NOT
//! -----------------------------------------------
//! One event, `activation`, with one low-cardinality prop, `via=memory_add` —
//! byte-identical to what the console sends for the same meaning, so the two
//! halves aggregate into one goal instead of two. It carries NO org, project,
//! user, token or memory identifier, and it forwards NO end-user IP (Plausible
//! would use `x-forwarded-for` to derive a visitor hash; supplying a caller's
//! real address would ship personal data to a third party to buy attribution
//! that is meaningless for a server-sent event anyway). That matches the
//! contract `console/src/analytics/server.ts` documents and the claim
//! `console/app/privacy` publishes — the two must not drift.
//!
//! CONFIGURATION (off unless asked for)
//!   - `PLAUSIBLE_DOMAIN`  the site registered in Plausible. UNSET/blank ⇒ this
//!     module is permanently disabled: no thread, no network, no cost. Same
//!     posture the console takes with `NEXT_PUBLIC_PLAUSIBLE_DOMAIN` and the
//!     gateway takes with `LIGHTTRACK_URL`. The name has no `NEXT_PUBLIC_`
//!     prefix on purpose: that prefix means "inline into a JS bundle at build
//!     time", which is exactly what a Rust process must NOT imitate — this one
//!     is read at runtime, so a PULLED image picks it up from `.env.deploy`.
//!   - `PLAUSIBLE_HOST`    optional; point at a self-hosted instance. Defaults
//!     to `https://plausible.io`.
//!
//! DESIGN — mirrors `brainiac-gateway`'s `lighttrack.rs` rather than inventing a
//! second telemetry shape: one `OnceLock` client resolved from the environment
//! once, a single background OS thread draining an mpsc channel with a blocking
//! HTTP client and a short timeout, every failure swallowed. A memory write is
//! never allowed to wait on, or fail because of, a beacon.

use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

/// Identifies the emitter to Plausible. The Events API rejects a request with
/// no user-agent outright, so this is required, not decorative.
const SERVER_UA: &str = "brainiac-server/1.0";

/// A beacon that has not completed in this long is abandoned. Nothing waits on
/// it, but an unbounded socket would leak the sender thread.
const EVENT_TIMEOUT: Duration = Duration::from_secs(3);

/// The one funnel event this process emits. Named once so it cannot drift from
/// `FUNNEL.activation` in `console/src/analytics/config.ts`.
const EVENT_ACTIVATION: &str = "activation";

/// The `via` value for "content actually arrived", identical to the console's.
const VIA_MEMORY_ADD: &str = "memory_add";

struct Client {
    tx: Sender<Value>,
    /// The Plausible site name, resolved once.
    domain: String,
}

static CLIENT: OnceLock<Option<Client>> = OnceLock::new();

fn client() -> Option<&'static Client> {
    CLIENT.get_or_init(init).as_ref()
}

/// Read the environment once. `None` (permanently disabled) when
/// `PLAUSIBLE_DOMAIN` is unset or blank — the default, and the posture every
/// local dev box and CI run stays in.
fn init() -> Option<Client> {
    let domain = env_value("PLAUSIBLE_DOMAIN")?;
    let host = env_value("PLAUSIBLE_HOST")
        .map(|h| h.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "https://plausible.io".to_string());
    let endpoint = format!("{host}/api/event");

    let (tx, rx) = mpsc::channel::<Value>();
    // A dedicated thread with a blocking client, exactly as the gateway does:
    // independent of any tokio runtime, so a caller can fire from anywhere and
    // nothing about the request path changes. If the thread cannot spawn we
    // simply have no analytics — never a reason to fail the host.
    let spawned = std::thread::Builder::new()
        .name("plausible".into())
        .spawn(move || {
            let http = reqwest::blocking::Client::builder()
                .timeout(EVENT_TIMEOUT)
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            while let Ok(body) = rx.recv() {
                let _ = http
                    .post(&endpoint)
                    .header("content-type", "application/json")
                    .header("user-agent", SERVER_UA)
                    // NO x-forwarded-for — see the module header.
                    .json(&body)
                    .send();
            }
        })
        .is_ok();
    if !spawned {
        return None;
    }
    Some(Client { tx, domain })
}

/// A non-blank environment value, trimmed. Blank is treated as unset so a
/// half-filled `.env` can never half-enable telemetry.
fn env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// True when a domain is configured. Call sites use this to skip the work that
/// only exists to feed an event (the "is this the org's first memory?" query),
/// so an unconfigured deployment pays literally nothing.
pub fn enabled() -> bool {
    client().is_some()
}

/// Record the activation event: a subject's knowledge actually moved.
///
/// Fire-and-forget and infallible by construction — it hands a value to a
/// channel and returns. A full/closed channel, a dead thread, an unreachable
/// Plausible, a 500 from Plausible: all are silence. Callers MUST NOT await,
/// retry, or branch on this.
pub fn track_activation_memory_add() {
    let Some(client) = client() else { return };
    let _ = client.tx.send(activation_event(&client.domain));
}

/// Is `source_id` the first source this org has ever held?
///
/// Call it INSIDE the transaction that just inserted the row and BEFORE the
/// commit, then fire [`track_activation_memory_add`] only if it returned true
/// AND the commit succeeded — an activation is a committed write, never an
/// attempted one.
///
/// "First" is derived entirely from existing data (`sources`, which every
/// ingest path already writes); no migration, no counter, no extra state to
/// keep consistent. Three deliberate properties:
///
/// - **Off ⇒ free.** When no domain is configured this returns `false` without
///   touching the database, so an un-instrumented deployment pays nothing per
///   write.
/// - **A failed check is not an activation.** A query error logs at debug and
///   returns `false`. Telemetry never fails, slows, or retries a memory write.
/// - **The one imprecision, stated.** Two ingests racing as an org's very first
///   two, in overlapping transactions, can each see no other row and each fire.
///   Making that impossible needs persisted state (a column or a marker row);
///   for a funnel counter a once-per-org-lifetime duplicate window is not worth
///   a migration. Every non-concurrent subsequent write is exactly and always
///   `false`.
pub async fn is_first_source(
    conn: &mut sqlx::PgConnection,
    org_id: uuid::Uuid,
    source_id: uuid::Uuid,
) -> bool {
    if !enabled() {
        return false;
    }
    match brainiac_store::governance::org_has_other_sources(conn, org_id, source_id).await {
        Ok(has_other) => !has_other,
        Err(err) => {
            tracing::debug!(error = %err, "activation first-source check failed; not reporting");
            false
        }
    }
}

/// Build the event body. Pure (no env reads, no I/O) so the wire shape — and
/// specifically the absence of any identifier — is unit-testable.
///
/// `url` is synthetic: the server has no page context, and inventing a real
/// path would pollute the pageview report with traffic no human made.
fn activation_event(domain: &str) -> Value {
    json!({
        "name": EVENT_ACTIVATION,
        "domain": domain,
        "url": format!("https://{domain}/server/{EVENT_ACTIVATION}"),
        "props": { "via": VIA_MEMORY_ADD },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published privacy claim in code form: the beacon body is four fixed
    /// keys and one low-cardinality prop. If someone ever adds an org/project/
    /// user id "just for debugging", this fails.
    #[test]
    fn the_event_body_carries_no_identifier_of_any_kind() {
        let ev = activation_event("brainiac.example");
        let obj = ev.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["domain", "name", "props", "url"]);
        assert_eq!(ev["name"], "activation");
        assert_eq!(ev["domain"], "brainiac.example");
        assert_eq!(ev["url"], "https://brainiac.example/server/activation");
        assert_eq!(ev["props"], json!({ "via": "memory_add" }));
    }

    /// The console sends `via: "memory_add"` for the same meaning
    /// (`console/src/lib/api.ts::submitMemory`). Same string ⇒ one goal in
    /// Plausible, not two that have to be added up by hand.
    #[test]
    fn via_matches_the_console_wording() {
        assert_eq!(VIA_MEMORY_ADD, "memory_add");
        assert_eq!(EVENT_ACTIVATION, "activation");
    }

    /// Unconfigured must mean OFF, and a half-filled `.env` (`PLAUSIBLE_DOMAIN=`
    /// with nothing after it, or a stray space) must count as unconfigured
    /// rather than as a site named `" "`.
    #[test]
    fn blank_and_whitespace_env_values_read_as_unset() {
        // A key nothing else in the process reads, so this cannot race the
        // real configuration.
        const KEY: &str = "BRAINIAC_TEST_ANALYTICS_BLANK";
        std::env::set_var(KEY, "   ");
        assert_eq!(env_value(KEY), None);
        std::env::set_var(KEY, "  value  ");
        assert_eq!(env_value(KEY).as_deref(), Some("value"));
        std::env::remove_var(KEY);
        assert_eq!(env_value(KEY), None);
    }

    /// The whole module is inert unless a domain is configured. The test
    /// process sets none, so `enabled()` is false and `track_activation_*` is a
    /// no-op that cannot throw, block, or open a socket.
    #[test]
    fn unconfigured_is_silent_and_never_panics() {
        assert!(!enabled(), "no PLAUSIBLE_DOMAIN in the test process");
        track_activation_memory_add();
    }
}
