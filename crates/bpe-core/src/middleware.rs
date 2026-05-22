//! BPE middleware: rate limiting, request IDs, org authorization.

use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Rate limiter state — simple token bucket per IP.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<RateLimiterInner>>,
    max_requests: u32,
    window_secs: u64,
}

struct RateLimiterInner {
    buckets: HashMap<IpAddr, (u32, Instant)>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    /// `max_requests` per `window_secs` seconds per IP.
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RateLimiterInner {
                buckets: HashMap::new(),
            })),
            max_requests,
            window_secs,
        }
    }

    /// Check if a request from this IP is allowed.
    fn check(&self, ip: IpAddr) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        let entry = inner.buckets.entry(ip).or_insert((0, now));
        if now.duration_since(entry.1) >= window {
            // Reset window
            entry.0 = 1;
            entry.1 = now;
            true
        } else if entry.0 < self.max_requests {
            entry.0 += 1;
            true
        } else {
            false
        }
    }

    /// Periodically clean up expired entries (call from a background task).
    pub fn cleanup(&self) {
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);
        inner.buckets.retain(|_, (_, started)| now.duration_since(*started) < window);
    }
}

/// Rate limiting middleware.
pub async fn rate_limit(
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let limiter = request
        .extensions()
        .get::<RateLimiter>()
        .cloned();

    if let Some(limiter) = limiter {
        // Extract IP from X-Forwarded-For or peer address
        let ip = request
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse::<IpAddr>().ok())
            .or_else(|| {
                request
                    .extensions()
                    .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                    .map(|ci| ci.0.ip())
            })
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

        if !limiter.check(ip) {
            let body = axum::Json(serde_json::json!({ "error": "Rate limit exceeded" }));
            return Err((StatusCode::TOO_MANY_REQUESTS, body).into_response());
        }
    }

    Ok(next.run(request).await)
}

/// Add a unique request ID header to responses for tracing.
pub async fn request_id(
    request: Request,
    next: Next,
) -> Response {
    let req_id = uuid::Uuid::new_v4().to_string();
    let mut response = next.run(request).await;
    if let Ok(val) = HeaderValue::from_str(&req_id) {
        response.headers_mut().insert("x-request-id", val);
    }
    response
}

/// Metrics-collecting middleware. Records request count, status, latency per endpoint.
pub async fn track_metrics(
    request: Request,
    next: Next,
) -> Response {
    let metrics = request
        .extensions()
        .get::<crate::metrics::Metrics>()
        .cloned();

    let path = request.uri().path().to_string();
    let start = Instant::now();

    if let Some(ref m) = metrics {
        m.request_start();
    }

    let response = next.run(request).await;

    if let Some(m) = metrics {
        let latency_us = start.elapsed().as_micros() as u64;
        let status = response.status().as_u16();
        m.request_end(status, &path, latency_us);
    }

    response
}

/// Verify the authenticated user belongs to the requested organization.
/// This checks that the `organization_id` in the JWT claims matches
/// the organization being accessed (when the org is resolved from the query).
///
/// Note: Platform admins bypass this check.
pub fn verify_org_access(
    claims: &crate::auth::AuthClaims,
    requested_org_id: uuid::Uuid,
) -> Result<(), crate::error::BpeError> {
    // Platform admins can access any org
    if claims.is_platform_admin {
        return Ok(());
    }

    // Parse the claims org ID
    let claims_org_id = claims.organization_id.parse::<uuid::Uuid>()
        .map_err(|_| crate::error::BpeError::Internal("Invalid organization_id in claims".into()))?;

    if claims_org_id != requested_org_id {
        return Err(crate::error::BpeError::Forbidden(
            "Access denied: you do not belong to this organization".into(),
        ));
    }

    Ok(())
}
