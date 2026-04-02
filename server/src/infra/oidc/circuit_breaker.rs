//! Circuit breaker for OIDC provider calls

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

/// Threshold: trip after 10 consecutive failures.
const FAILURE_THRESHOLD: usize = 10;
/// After tripping, auto-recover after 5 minutes since last failure.
const RESET_WINDOW_SECS: i64 = 300; // 5 minutes

/// Circuit breaker to prevent cascading failures when the OIDC provider is down.
#[derive(Debug)]
pub struct OidcCircuitBreaker {
    recent_failures: AtomicUsize,
    /// Timestamp of the most recent failure (unix seconds).
    last_failure_time: AtomicI64,
}

impl OidcCircuitBreaker {
    pub fn new() -> Self {
        Self {
            recent_failures: AtomicUsize::new(0),
            last_failure_time: AtomicI64::new(0),
        }
    }

    /// Record a successful call. Resets failure count.
    pub fn record_success(&self) {
        self.recent_failures.store(0, Ordering::Release);
    }

    /// Record a failed call. Increments failure count and records timestamp.
    pub fn record_failure(&self) {
        let failures = self.recent_failures.fetch_add(1, Ordering::AcqRel) + 1;
        self.last_failure_time
            .store(chrono::Utc::now().timestamp(), Ordering::Release);
        if failures >= FAILURE_THRESHOLD {
            tracing::warn!(
                "OIDC circuit breaker: {} consecutive failures (threshold: {})",
                failures,
                FAILURE_THRESHOLD
            );
        }
    }

    /// Check if the circuit breaker is tripped.
    ///
    /// Returns true when:
    /// 1. There have been >= FAILURE_THRESHOLD consecutive failures, AND
    /// 2. Either:
    ///    a. The time since the last failure is less than RESET_WINDOW_SECS (staying tripped), OR
    ///    b. There have been no recovery attempts (i.e., last_success was never reset)
    ///
    /// Auto-recovery: after RESET_WINDOW_SECS since the last failure,
    /// the breaker allows a single "probe" request through. If it succeeds,
    /// `record_success()` resets the failure count. If it fails, the
    /// failure count remains and the breaker stays tripped.
    pub fn is_tripped(&self) -> bool {
        let failures = self.recent_failures.load(Ordering::Acquire);
        if failures < FAILURE_THRESHOLD {
            return false;
        }

        let last_failure = self.last_failure_time.load(Ordering::Acquire);
        let now = chrono::Utc::now().timestamp();
        let elapsed = now - last_failure;

        // If enough time has passed since last failure, allow a probe through
        // (half-open state). The next record_failure() will re-trip, or
        // record_success() will fully reset.
        if elapsed >= RESET_WINDOW_SECS {
            return false;
        }

        true
    }

    /// Check if password login should be forced (circuit breaker tripped)
    pub fn should_force_password_login(&self) -> bool {
        self.is_tripped()
    }
}

impl Default for OidcCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_not_tripped() {
        let cb = OidcCircuitBreaker::new();
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_success_resets_failures() {
        let cb = OidcCircuitBreaker::new();
        for _ in 0..15 {
            cb.record_failure();
        }
        cb.record_success();
        assert_eq!(cb.recent_failures.load(Ordering::Acquire), 0);
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_trips_after_threshold() {
        let cb = OidcCircuitBreaker::new();
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        // Just tripped — not enough time has passed for recovery
        assert!(cb.is_tripped());
    }

    #[test]
    fn test_not_tripped_below_threshold() {
        let cb = OidcCircuitBreaker::new();
        for _ in 0..(FAILURE_THRESHOLD - 1) {
            cb.record_failure();
        }
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_auto_recovery_after_window() {
        let cb = OidcCircuitBreaker::new();
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_tripped());

        // Simulate time passing beyond reset window
        cb.last_failure_time
            .store(chrono::Utc::now().timestamp() - RESET_WINDOW_SECS - 1, Ordering::Release);
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_retrip_on_failure_after_recovery() {
        let cb = OidcCircuitBreaker::new();
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        // Simulate recovery
        cb.last_failure_time
            .store(chrono::Utc::now().timestamp() - RESET_WINDOW_SECS - 1, Ordering::Release);
        assert!(!cb.is_tripped());

        // New failure after recovery — failure count is still high, but last_failure is now
        // Reset failure count first (simulating a successful attempt during half-open)
        cb.record_success();
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_tripped());
    }
}
