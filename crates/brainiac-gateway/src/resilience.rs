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

use std::sync::atomic::{AtomicU32, Ordering};
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

pub struct CircuitBreaker {
    consecutive_failures: AtomicU32,
    open_until: Mutex<Option<Instant>>,
    threshold: u32,
    cooldown: Duration,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            open_until: Mutex::new(None),
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

    /// Err when the circuit is open (fail fast). A cooldown expiry lets one
    /// call through half-open; its outcome re-closes or re-opens the circuit.
    fn check(&self) -> Result<()> {
        let mut open = self.open_until.lock().expect("breaker lock");
        if let Some(until) = *open {
            if Instant::now() < until {
                anyhow::bail!(
                    "gateway circuit open ({} consecutive failures); retrying after cooldown",
                    self.consecutive_failures.load(Ordering::Relaxed)
                );
            }
            *open = None; // half-open: allow this call to probe
        }
        Ok(())
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        let n = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= self.threshold {
            let until = Instant::now() + self.cooldown;
            *self.open_until.lock().expect("breaker lock") = Some(until);
            tracing::warn!(
                consecutive_failures = n,
                cooldown_secs = self.cooldown.as_secs(),
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
                    let body = resp
                        .text()
                        .await
                        .with_context(|| format!("{what} body"))?;
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

    #[test]
    fn breaker_opens_after_threshold_and_recovers() {
        let cb = CircuitBreaker {
            consecutive_failures: AtomicU32::new(0),
            open_until: Mutex::new(None),
            threshold: 2,
            cooldown: Duration::from_millis(10),
        };
        assert!(cb.check().is_ok());
        cb.record_failure();
        assert!(cb.check().is_ok(), "below threshold stays closed");
        cb.record_failure();
        assert!(cb.check().is_err(), "at threshold the circuit opens");
        std::thread::sleep(Duration::from_millis(15));
        assert!(cb.check().is_ok(), "cooldown expiry half-opens");
        cb.record_success();
        assert_eq!(cb.consecutive_failures.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn retry_config_defaults_are_sane() {
        let r = RetryConfig::default();
        assert!(r.max_attempts >= 2);
        assert!(r.base_delay < r.max_delay);
    }
}
