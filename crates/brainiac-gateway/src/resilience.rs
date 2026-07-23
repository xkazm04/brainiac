//! Retry + circuit breaking for provider HTTP calls.
//!
//! DashScope (and any hosted model API) throttles with 429 and hiccups with
//! transient 5xx; before this module a single such response failed the whole
//! ingest job. Policy:
//!
//! - **Retry** transport errors, 429 and 5xx with exponential backoff +
//!   jitter (defaults: 3 attempts, 500ms base, 8s cap). Other 4xx are
//!   permanent (bad key, bad request) and fail immediately.
//! - **Circuit breaker** per provider instance: after N consecutive
//!   ultimately-failed calls the circuit opens and calls fail fast for a
//!   cooldown window, so a dead upstream doesn't burn the retry budget of
//!   every queued job (defaults: 5 failures, 30s cooldown).
//!
//! Env overrides: `BRAINIAC_GATEWAY_RETRIES`, `BRAINIAC_GATEWAY_BASE_DELAY_MS`,
//! `BRAINIAC_GATEWAY_BREAKER_THRESHOLD`, `BRAINIAC_GATEWAY_BREAKER_COOLDOWN_SECS`.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(8),
        }
    }
}

impl RetryConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Some(n) = env_parse::<u32>("BRAINIAC_GATEWAY_RETRIES") {
            cfg.max_attempts = n.max(1);
        }
        if let Some(ms) = env_parse::<u64>("BRAINIAC_GATEWAY_BASE_DELAY_MS") {
            cfg.base_delay = Duration::from_millis(ms.max(1));
        }
        cfg
    }
}

fn env_parse<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok()?.trim().parse().ok()
}

/// Cheap jitter without a rand dependency: subsecond wall-clock noise.
fn jitter(upto: Duration) -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
        ^ (std::process::id() as u64);
    Duration::from_millis(if upto.as_millis() == 0 {
        0
    } else {
        nanos % (upto.as_millis() as u64)
    })
}

/// All breaker state under ONE lock.
///
/// It used to live in an `open_until` Mutex plus a separate `consecutive_failures`
/// atomic, so `check` and `record_failure` never saw a consistent snapshot.
#[derive(Default)]
struct BreakerState {
    consecutive_failures: u32,
    /// `Some(t)` ⇒ circuit open until `t`.
    open_until: Option<Instant>,
    /// When the current half-open probe was admitted. Exactly one caller may
    /// probe; everyone else fails fast until its outcome resolves the circuit.
    /// Timestamped rather than a bool so an abandoned probe (the caller's future
    /// dropped, or `send` bailing on a body-read before it records an outcome)
    /// cannot wedge the breaker half-open forever — a stale probe is taken over.
    probing_since: Option<Instant>,
}

pub struct CircuitBreaker {
    state: Mutex<BreakerState>,
    threshold: u32,
    cooldown: Duration,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            state: Mutex::new(BreakerState::default()),
            threshold: 5,
            cooldown: Duration::from_secs(30),
        }
    }
}

impl CircuitBreaker {
    pub fn from_env() -> Self {
        let mut cb = Self::default();
        if let Some(n) = env_parse::<u32>("BRAINIAC_GATEWAY_BREAKER_THRESHOLD") {
            cb.threshold = n.max(1);
        }
        if let Some(secs) = env_parse::<u64>("BRAINIAC_GATEWAY_BREAKER_COOLDOWN_SECS") {
            cb.cooldown = Duration::from_secs(secs.max(1));
        }
        cb
    }

    /// Err when the circuit is open (fail fast). A cooldown expiry admits exactly
    /// ONE call through half-open; its outcome re-closes or re-opens the circuit.
    ///
    /// The old version modelled half-open by clearing `open_until`, so the first
    /// caller past the cooldown released the whole waiting herd: every concurrent
    /// `check()` then saw `None` and proceeded, each burning a full max_attempts
    /// retry storm against a still-dead upstream before any of them reached
    /// `record_failure()` to re-open. That defeats the breaker in precisely the
    /// scenario it exists for (dead upstream + backpressured queue) and hammers a
    /// recovering provider. `open_until` now stays set until an outcome resolves
    /// it, and admission is a single probe token.
    fn check(&self) -> Result<()> {
        let mut s = self.state.lock().expect("breaker lock");
        let Some(until) = s.open_until else {
            return Ok(()); // closed — the common path
        };
        let now = Instant::now();
        if now < until {
            anyhow::bail!(
                "gateway circuit open ({} consecutive failures); retrying after cooldown",
                s.consecutive_failures
            );
        }
        // Cooldown expired ⇒ half-open. Admit one probe.
        if let Some(since) = s.probing_since {
            if now.duration_since(since) < self.cooldown {
                anyhow::bail!("gateway circuit half-open; a probe is already in flight");
            }
            // The previous probe never reported back (dropped future, or a bail
            // before record_*). Don't stay wedged — take it over.
            tracing::warn!("half-open probe abandoned; taking over");
        }
        s.probing_since = Some(now);
        // Clear the tally so a stale pre-cooldown count can't instantly re-trip
        // the breaker on the probe's first failure.
        s.consecutive_failures = 0;
        Ok(())
    }

    fn record_success(&self) {
        let mut s = self.state.lock().expect("breaker lock");
        s.consecutive_failures = 0;
        // A successful probe closes the circuit and releases the herd.
        s.open_until = None;
        s.probing_since = None;
    }

    fn record_failure(&self) {
        let mut s = self.state.lock().expect("breaker lock");
        s.consecutive_failures += 1;
        let n = s.consecutive_failures;
        // A failed half-open probe re-opens for a full cooldown on its own — the
        // upstream is still dead, so don't wait for `threshold` more failures.
        let was_probe = s.probing_since.take().is_some();
        if was_probe || n >= self.threshold {
            s.open_until = Some(Instant::now() + self.cooldown);
            tracing::warn!(
                consecutive_failures = n,
                cooldown_secs = self.cooldown.as_secs(),
                probe = was_probe,
                "gateway circuit opened"
            );
        }
    }
}

pub struct Resilience {
    pub retry: RetryConfig,
    pub breaker: CircuitBreaker,
}

impl Resilience {
    pub fn from_env() -> Self {
        Self {
            retry: RetryConfig::from_env(),
            breaker: CircuitBreaker::from_env(),
        }
    }

    /// Send `req` with retry/backoff under the breaker. Returns the response
    /// body text of the first 2xx; retries transport errors, 429 and 5xx;
    /// fails immediately on other statuses (auth/validation are permanent).
    pub async fn send(&self, req: reqwest::RequestBuilder, what: &str) -> Result<String> {
        self.breaker.check()?;
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            let this_try = req
                .try_clone()
                .context("gateway request is not retryable (streaming body?)")?;
            let err: anyhow::Error = match this_try.send().await {
                Err(e) => anyhow::Error::new(e).context(format!("{what} request")),
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.with_context(|| format!("{what} body"))?;
                    if status.is_success() {
                        self.breaker.record_success();
                        return Ok(body);
                    }
                    let head: String = body.chars().take(400).collect();
                    if status.as_u16() == 429 || status.is_server_error() {
                        anyhow::anyhow!("{what} {status}: {head}")
                    } else {
                        // Permanent (auth, validation) — do not retry.
                        self.breaker.record_failure();
                        anyhow::bail!("{what} {status}: {head}");
                    }
                }
            };
            if attempt >= self.retry.max_attempts {
                self.breaker.record_failure();
                return Err(err.context(format!("{what} failed after {attempt} attempts")));
            }
            let backoff = self
                .retry
                .base_delay
                .saturating_mul(2u32.saturating_pow(attempt - 1))
                .min(self.retry.max_delay);
            let delay = backoff + jitter(backoff / 2);
            tracing::warn!(%err, attempt, delay_ms = delay.as_millis() as u64, "{what} retrying");
            tokio::time::sleep(delay).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn breaker(threshold: u32, cooldown_ms: u64) -> CircuitBreaker {
        CircuitBreaker {
            state: Mutex::new(BreakerState::default()),
            threshold,
            cooldown: Duration::from_millis(cooldown_ms),
        }
    }

    /// A `Resilience` with an explicit (fast) retry config + the given breaker —
    /// no env, no wall-clock beyond the millisecond backoffs under test.
    fn resilience(max_attempts: u32, base_ms: u64, cb: CircuitBreaker) -> Resilience {
        Resilience {
            retry: RetryConfig {
                max_attempts,
                base_delay: Duration::from_millis(base_ms),
                max_delay: Duration::from_millis(base_ms.max(1) * 4),
            },
            breaker: cb,
        }
    }

    /// A fresh GET builder at the mock server's root. `send` re-clones per
    /// attempt, so one builder drives the whole retry loop.
    fn get(server: &MockServer) -> reqwest::RequestBuilder {
        reqwest::Client::new().get(server.uri())
    }

    #[test]
    fn breaker_opens_after_threshold_and_recovers() {
        let cb = breaker(2, 10);
        assert!(cb.check().is_ok());
        cb.record_failure();
        assert!(cb.check().is_ok(), "below threshold stays closed");
        cb.record_failure();
        assert!(cb.check().is_err(), "at threshold the circuit opens");
        std::thread::sleep(Duration::from_millis(15));
        assert!(cb.check().is_ok(), "cooldown expiry half-opens");
        cb.record_success();
        assert_eq!(cb.state.lock().expect("lock").consecutive_failures, 0);
        assert!(cb.check().is_ok(), "a successful probe closes the circuit");
    }

    #[test]
    fn half_open_admits_exactly_one_probe() {
        // The herd bug: at cooldown expiry the first caller used to clear
        // open_until, so every concurrent caller was let through at once against a
        // still-dead upstream, each burning a full retry budget.
        let cb = breaker(1, 30);
        cb.record_failure();
        assert!(cb.check().is_err(), "open");
        std::thread::sleep(Duration::from_millis(35));
        assert!(cb.check().is_ok(), "first caller is the probe");
        assert!(
            cb.check().is_err(),
            "the rest of the herd must still fail fast while a probe is in flight"
        );
        assert!(cb.check().is_err(), "and stay failing fast");
    }

    #[test]
    fn a_failed_probe_reopens_without_waiting_for_threshold() {
        let cb = breaker(5, 20); // threshold 5, but one failed probe must re-open
        for _ in 0..5 {
            cb.record_failure();
        }
        assert!(cb.check().is_err(), "open");
        std::thread::sleep(Duration::from_millis(25));
        assert!(cb.check().is_ok(), "probe admitted");
        cb.record_failure(); // the probe failed
        assert!(
            cb.check().is_err(),
            "a failed probe re-opens immediately — the upstream is still dead"
        );
    }

    #[test]
    fn an_abandoned_probe_does_not_wedge_the_breaker() {
        // send() can exit without recording an outcome (a dropped future, or a
        // bail on the body read), so a probe token that is never returned must not
        // block the circuit forever.
        let cb = breaker(1, 10);
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(15));
        assert!(cb.check().is_ok(), "probe admitted");
        // ...and never reports back. After the staleness window, take it over.
        std::thread::sleep(Duration::from_millis(15));
        assert!(
            cb.check().is_ok(),
            "a stale probe is taken over, not deadlocked"
        );
    }

    #[test]
    fn retry_config_defaults_are_sane() {
        let r = RetryConfig::default();
        assert!(r.max_attempts >= 2);
        assert!(r.base_delay < r.max_delay);
    }

    // ── send(): retry + breaker integration against a mock upstream ──────────

    #[tokio::test]
    async fn send_retries_5xx_up_to_the_budget_then_errors_with_backoff() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503).set_body_string("upstream on fire"))
            .mount(&server)
            .await;

        let r = resilience(3, 10, CircuitBreaker::default());
        let start = Instant::now();
        let err = r
            .send(get(&server), "probe")
            .await
            .expect_err("a persistent 503 exhausts the attempt budget");

        // Exactly max_attempts requests reached the upstream — the retry count.
        let hits = server.received_requests().await.expect("recorded");
        assert_eq!(hits.len(), 3, "3 attempts against a persistent 5xx");

        let chain = format!("{err:#}");
        assert!(chain.contains("503"), "names the 5xx status, got: {chain}");
        assert!(
            chain.contains("failed after 3 attempts"),
            "reports the exhausted budget, got: {chain}"
        );

        // Backoff actually slept between the 3 attempts (two gaps ≥ base_delay).
        // Only a conservative lower bound is asserted — exact timing is jittered
        // and would be flaky under CI load; `send` uses tokio sleeps that never
        // undershoot, so base_delay is a hard floor.
        assert!(
            start.elapsed() >= Duration::from_millis(10),
            "exponential backoff slept at least base_delay between attempts"
        );
    }

    #[tokio::test]
    async fn send_recovers_when_a_5xx_burst_clears_before_the_budget() {
        let server = MockServer::start().await;
        // Two 500s, then healthy — priority 1 (higher) wins until it is spent.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(2)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("recovered"))
            .with_priority(2)
            .mount(&server)
            .await;

        let r = resilience(5, 5, CircuitBreaker::default());
        let body = r
            .send(get(&server), "probe")
            .await
            .expect("recovers on the third attempt, within budget");
        assert_eq!(body, "recovered");

        let hits = server.received_requests().await.expect("recorded");
        assert_eq!(hits.len(), 3, "two 5xx then the 200 that succeeds");
    }

    #[tokio::test]
    async fn send_does_not_retry_a_permanent_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let r = resilience(4, 5, CircuitBreaker::default());
        let err = r
            .send(get(&server), "probe")
            .await
            .expect_err("400 is permanent");

        let hits = server.received_requests().await.expect("recorded");
        assert_eq!(hits.len(), 1, "a permanent 4xx is not retried");
        assert!(format!("{err:#}").contains("400"));
    }

    #[tokio::test]
    async fn send_opens_the_breaker_then_fails_fast_without_touching_upstream() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        // max_attempts = 1 ⇒ each failed send records exactly one breaker
        // failure; threshold 3 opens after three; a 500ms cooldown keeps it open
        // across the immediate fourth call.
        let r = resilience(1, 1, breaker(3, 500));
        for _ in 0..3 {
            assert!(
                r.send(get(&server), "probe").await.is_err(),
                "each 503 send fails and records a breaker failure"
            );
        }

        let err = r
            .send(get(&server), "probe")
            .await
            .expect_err("circuit is open");
        assert!(
            format!("{err:#}").contains("circuit open"),
            "the fourth call is a fail-fast, got: {err:#}"
        );

        let hits = server.received_requests().await.expect("recorded");
        assert_eq!(
            hits.len(),
            3,
            "the fail-fast call never reached the upstream"
        );
    }

    #[tokio::test]
    async fn send_probe_recovers_the_breaker_after_cooldown() {
        let server = MockServer::start().await;
        // One 503 to trip a threshold-1 breaker, then healthy forever.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("healthy"))
            .with_priority(2)
            .mount(&server)
            .await;

        let r = resilience(1, 1, breaker(1, 40));

        // 1) open the breaker.
        assert!(r.send(get(&server), "probe").await.is_err(), "opens");

        // 2) immediately: fail fast, upstream untouched.
        let fast = r
            .send(get(&server), "probe")
            .await
            .expect_err("open circuit fails fast");
        assert!(format!("{fast:#}").contains("circuit open"));

        // 3) after cooldown: the half-open probe reaches the now-healthy upstream
        //    and closes the circuit.
        tokio::time::sleep(Duration::from_millis(55)).await;
        let body = r
            .send(get(&server), "probe")
            .await
            .expect("the probe succeeds and closes the breaker");
        assert_eq!(body, "healthy");

        // 4) closed: calls pass straight through.
        let body2 = r
            .send(get(&server), "probe")
            .await
            .expect("a closed circuit passes through");
        assert_eq!(body2, "healthy");

        // open(1) + probe(1) + passthrough(1); the step-2 fail-fast hit nothing.
        let hits = server.received_requests().await.expect("recorded");
        assert_eq!(hits.len(), 3);
    }
}
