//! Production hardening middleware
//!
//! Provides rate limiting, circuit breaker, and concurrency control.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tokio::sync::Semaphore;

use crate::config::RateLimitConfig;

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    /// Tokens available
    tokens: AtomicUsize,
    /// Maximum tokens (bucket capacity)
    max_tokens: usize,
    /// Tokens per second refill rate
    refill_rate: u32,
    /// Last refill time
    last_refill: RwLock<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(requests_per_second: u32) -> Self {
        let max_tokens = (requests_per_second as usize).max(1);
        Self {
            tokens: AtomicUsize::new(max_tokens),
            max_tokens,
            refill_rate: requests_per_second,
            last_refill: RwLock::new(Instant::now()),
        }
    }

    /// Try to acquire a token. Returns true if allowed, false if rate limited.
    pub fn try_acquire(&self) -> bool {
        self.refill();

        // Try to decrement tokens
        loop {
            let current = self.tokens.load(Ordering::Acquire);
            if current == 0 {
                return false;
            }
            if self.tokens.compare_exchange(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ).is_ok() {
                return true;
            }
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let now = Instant::now();
        let mut last = self.last_refill.write();
        let elapsed = now.duration_since(*last);

        // Calculate tokens to add
        let tokens_to_add = (elapsed.as_secs_f64() * self.refill_rate as f64) as usize;
        if tokens_to_add > 0 {
            *last = now;
            let current = self.tokens.load(Ordering::Acquire);
            let new_tokens = (current + tokens_to_add).min(self.max_tokens);
            self.tokens.store(new_tokens, Ordering::Release);
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation
    Closed,
    /// Rejecting requests after failures
    Open,
    /// Testing if service recovered
    HalfOpen,
}

/// Circuit breaker for handling cascading failures
pub struct CircuitBreaker {
    /// Current state
    state: RwLock<CircuitState>,
    /// Consecutive failure count
    failure_count: AtomicUsize,
    /// Threshold before opening circuit
    threshold: usize,
    /// Time when circuit opened
    opened_at: RwLock<Option<Instant>>,
    /// Reset duration
    reset_duration: Duration,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(threshold: usize, reset_secs: u64) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            failure_count: AtomicUsize::new(0),
            threshold,
            opened_at: RwLock::new(None),
            reset_duration: Duration::from_secs(reset_secs),
        }
    }

    /// Check if request should be allowed
    pub fn allow_request(&self) -> bool {
        let state = *self.state.read();
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(opened) = *self.opened_at.read() {
                    if opened.elapsed() >= self.reset_duration {
                        *self.state.write() = CircuitState::HalfOpen;
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true, // Allow test request
        }
    }

    /// Record a successful operation
    pub fn record_success(&self) {
        let state = *self.state.read();
        match state {
            CircuitState::HalfOpen => {
                // Service recovered, close circuit
                *self.state.write() = CircuitState::Closed;
                self.failure_count.store(0, Ordering::Release);
                *self.opened_at.write() = None;
                tracing::info!("Circuit breaker: recovered, closing circuit");
            }
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Release);
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed operation
    pub fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::AcqRel) + 1;
        let state = *self.state.read();

        match state {
            CircuitState::Closed if failures >= self.threshold => {
                // Trip the circuit
                *self.state.write() = CircuitState::Open;
                *self.opened_at.write() = Some(Instant::now());
                tracing::warn!(
                    "Circuit breaker: opened after {} consecutive failures",
                    failures
                );
            }
            CircuitState::HalfOpen => {
                // Failed during recovery, open again
                *self.state.write() = CircuitState::Open;
                *self.opened_at.write() = Some(Instant::now());
                tracing::warn!("Circuit breaker: recovery failed, reopening");
            }
            _ => {}
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        *self.state.read()
    }
}

/// Backpressure manager for queue depth control
pub struct BackpressureManager {
    /// Current queue depth
    current_depth: AtomicUsize,
    /// Maximum allowed depth
    max_depth: usize,
}

impl BackpressureManager {
    /// Create a new backpressure manager
    pub fn new(max_depth: usize) -> Self {
        Self {
            current_depth: AtomicUsize::new(0),
            max_depth,
        }
    }

    /// Check if queue can accept more work
    pub fn can_accept(&self) -> bool {
        self.current_depth.load(Ordering::Acquire) < self.max_depth
    }

    /// Try to reserve a slot in the queue
    pub fn try_reserve(&self) -> bool {
        loop {
            let current = self.current_depth.load(Ordering::Acquire);
            if current >= self.max_depth {
                return false;
            }
            if self.current_depth.compare_exchange(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ).is_ok() {
                return true;
            }
        }
    }

    /// Release a slot when work completes
    pub fn release(&self) {
        self.current_depth.fetch_sub(1, Ordering::Release);
    }

    /// Get current depth
    pub fn current(&self) -> usize {
        self.current_depth.load(Ordering::Acquire)
    }

    /// Get maximum depth
    pub fn max(&self) -> usize {
        self.max_depth
    }
}

/// Concurrency limiter using semaphores
pub struct ConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
    max_permits: usize,
}

impl ConcurrencyLimiter {
    /// Create a new concurrency limiter
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_permits: max_concurrent,
        }
    }

    /// Try to acquire a permit (non-blocking)
    pub fn try_acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.semaphore.clone().try_acquire_owned().ok()
    }

    /// Acquire a permit (blocking)
    ///
    /// # Panics
    /// This should never panic in practice since the semaphore is held by an Arc
    /// and is never explicitly closed. If the semaphore is somehow closed, this
    /// will log an error and panic, which is intentional as it indicates a bug.
    pub async fn acquire(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("ConcurrencyLimiter semaphore unexpectedly closed - this is a bug")
    }

    /// Get available permits
    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Get maximum permits
    pub fn max(&self) -> usize {
        self.max_permits
    }
}

/// Combined production hardening controls
pub struct ProductionControls {
    /// Rate limiter for query endpoints
    pub query_rate_limiter: RateLimiter,
    /// Rate limiter for upload endpoints
    pub upload_rate_limiter: RateLimiter,
    /// Circuit breaker for external service calls
    pub circuit_breaker: CircuitBreaker,
    /// Backpressure for job queue
    pub backpressure: BackpressureManager,
    /// Concurrency limiter for uploads
    pub upload_limiter: ConcurrencyLimiter,
    /// Concurrency limiter for GCS operations
    pub gcs_limiter: ConcurrencyLimiter,
    /// Whether rate limiting is enabled
    pub enabled: bool,
}

impl ProductionControls {
    /// Create production controls from config
    pub fn from_config(config: &RateLimitConfig) -> Self {
        Self {
            query_rate_limiter: RateLimiter::new(config.query_requests_per_second),
            upload_rate_limiter: RateLimiter::new(config.upload_requests_per_second),
            circuit_breaker: CircuitBreaker::new(
                config.circuit_breaker_threshold,
                config.circuit_breaker_reset_secs,
            ),
            backpressure: BackpressureManager::new(config.max_queue_depth),
            upload_limiter: ConcurrencyLimiter::new(config.max_concurrent_uploads),
            gcs_limiter: ConcurrencyLimiter::new(config.max_concurrent_gcs_operations),
            enabled: config.enabled,
        }
    }

    /// Check if a query request should be allowed
    pub fn allow_query(&self) -> bool {
        if !self.enabled {
            return true;
        }
        self.query_rate_limiter.try_acquire()
    }

    /// Check if an upload request should be allowed
    pub fn allow_upload(&self) -> bool {
        if !self.enabled {
            return true;
        }
        self.upload_rate_limiter.try_acquire() && self.backpressure.can_accept()
    }

    /// Check if external service call should proceed (circuit breaker)
    pub fn allow_external_call(&self) -> bool {
        if !self.enabled {
            return true;
        }
        self.circuit_breaker.allow_request()
    }

    /// Try to acquire upload concurrency slot
    pub fn try_acquire_upload_slot(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        if !self.enabled {
            // When disabled, we still want to return a permit for consistency
            // but we use a high limit
            return self.upload_limiter.try_acquire();
        }
        self.upload_limiter.try_acquire()
    }

    /// Try to acquire GCS concurrency slot
    pub fn try_acquire_gcs_slot(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        if !self.enabled {
            return self.gcs_limiter.try_acquire();
        }
        self.gcs_limiter.try_acquire()
    }

    /// Acquire GCS slot (blocking)
    pub async fn acquire_gcs_slot(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.gcs_limiter.acquire().await
    }

    /// Reserve a job queue slot
    pub fn reserve_job_slot(&self) -> bool {
        if !self.enabled {
            return true;
        }
        self.backpressure.try_reserve()
    }

    /// Release a job queue slot
    pub fn release_job_slot(&self) {
        if self.enabled {
            self.backpressure.release();
        }
    }

    /// Record successful external call
    pub fn record_success(&self) {
        self.circuit_breaker.record_success();
    }

    /// Record failed external call
    pub fn record_failure(&self) {
        self.circuit_breaker.record_failure();
    }

    /// Get status info for health check
    pub fn status(&self) -> ProductionControlsStatus {
        ProductionControlsStatus {
            enabled: self.enabled,
            circuit_state: format!("{:?}", self.circuit_breaker.state()),
            queue_depth: self.backpressure.current(),
            max_queue_depth: self.backpressure.max(),
            upload_slots_available: self.upload_limiter.available(),
            gcs_slots_available: self.gcs_limiter.available(),
        }
    }
}

impl Default for ProductionControls {
    fn default() -> Self {
        Self::from_config(&RateLimitConfig::default())
    }
}

/// Status information for monitoring
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionControlsStatus {
    pub enabled: bool,
    pub circuit_state: String,
    pub queue_depth: usize,
    pub max_queue_depth: usize,
    pub upload_slots_available: usize,
    pub gcs_slots_available: usize,
}
