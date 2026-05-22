//! In-process metrics collection for BPE observability.
//!
//! Tracks request counts, latency histograms, error rates, and active connections
//! without requiring an external metrics backend.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Global metrics registry.
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<MetricsInner>,
}

struct MetricsInner {
    started_at: Instant,
    total_requests: AtomicU64,
    active_requests: AtomicI64,
    /// Counts by status code class: 2xx, 3xx, 4xx, 5xx
    status_2xx: AtomicU64,
    status_3xx: AtomicU64,
    status_4xx: AtomicU64,
    status_5xx: AtomicU64,
    /// Per-endpoint request counts and total latency (microseconds)
    endpoints: Mutex<HashMap<String, EndpointMetrics>>,
    /// Latency histogram buckets (in milliseconds)
    latency_buckets: Mutex<LatencyHistogram>,
}

#[derive(Default, Clone)]
struct EndpointMetrics {
    count: u64,
    total_latency_us: u64,
    errors: u64,
}

struct LatencyHistogram {
    /// Bucket boundaries in ms: [1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000]
    boundaries: Vec<u64>,
    counts: Vec<u64>,
    total_count: u64,
    total_sum_us: u64,
}

impl LatencyHistogram {
    fn new() -> Self {
        let boundaries = vec![1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000];
        let counts = vec![0u64; boundaries.len() + 1]; // +1 for overflow bucket
        Self {
            boundaries,
            counts,
            total_count: 0,
            total_sum_us: 0,
        }
    }

    fn record(&mut self, latency_us: u64) {
        let ms = latency_us / 1000;
        self.total_count += 1;
        self.total_sum_us += latency_us;

        for (i, &boundary) in self.boundaries.iter().enumerate() {
            if ms <= boundary {
                self.counts[i] += 1;
                return;
            }
        }
        // Overflow bucket
        *self.counts.last_mut().unwrap() += 1;
    }

    fn to_json(&self) -> serde_json::Value {
        let mut buckets = serde_json::Map::new();
        for (i, &boundary) in self.boundaries.iter().enumerate() {
            buckets.insert(format!("<={boundary}ms"), serde_json::json!(self.counts[i]));
        }
        buckets.insert(
            ">10000ms".into(),
            serde_json::json!(self.counts.last().unwrap_or(&0)),
        );

        let avg_ms = if self.total_count > 0 {
            (self.total_sum_us as f64 / self.total_count as f64) / 1000.0
        } else {
            0.0
        };

        serde_json::json!({
            "buckets": buckets,
            "count": self.total_count,
            "avg_ms": (avg_ms * 100.0).round() / 100.0,
        })
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                started_at: Instant::now(),
                total_requests: AtomicU64::new(0),
                active_requests: AtomicI64::new(0),
                status_2xx: AtomicU64::new(0),
                status_3xx: AtomicU64::new(0),
                status_4xx: AtomicU64::new(0),
                status_5xx: AtomicU64::new(0),
                endpoints: Mutex::new(HashMap::new()),
                latency_buckets: Mutex::new(LatencyHistogram::new()),
            }),
        }
    }

    /// Record the start of a request. Returns a guard that records completion.
    pub fn request_start(&self) {
        self.inner.total_requests.fetch_add(1, Ordering::Relaxed);
        self.inner.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record the completion of a request.
    pub fn request_end(&self, status: u16, path: &str, latency_us: u64) {
        self.inner.active_requests.fetch_sub(1, Ordering::Relaxed);

        // Status code class
        match status / 100 {
            2 => { self.inner.status_2xx.fetch_add(1, Ordering::Relaxed); }
            3 => { self.inner.status_3xx.fetch_add(1, Ordering::Relaxed); }
            4 => { self.inner.status_4xx.fetch_add(1, Ordering::Relaxed); }
            5 => { self.inner.status_5xx.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }

        // Per-endpoint metrics — normalize path (strip UUIDs)
        let normalized = normalize_path(path);
        if let Ok(mut endpoints) = self.inner.endpoints.lock() {
            let entry = endpoints.entry(normalized).or_default();
            entry.count += 1;
            entry.total_latency_us += latency_us;
            if status >= 500 {
                entry.errors += 1;
            }
        }

        // Latency histogram
        if let Ok(mut hist) = self.inner.latency_buckets.lock() {
            hist.record(latency_us);
        }
    }

    /// Generate a JSON snapshot of all metrics.
    pub fn snapshot(&self, pool_status: Option<serde_json::Value>) -> serde_json::Value {
        let uptime_secs = self.inner.started_at.elapsed().as_secs();
        let total = self.inner.total_requests.load(Ordering::Relaxed);
        let active = self.inner.active_requests.load(Ordering::Relaxed);

        let status_codes = serde_json::json!({
            "2xx": self.inner.status_2xx.load(Ordering::Relaxed),
            "3xx": self.inner.status_3xx.load(Ordering::Relaxed),
            "4xx": self.inner.status_4xx.load(Ordering::Relaxed),
            "5xx": self.inner.status_5xx.load(Ordering::Relaxed),
        });

        let latency = self.inner.latency_buckets.lock()
            .map(|h| h.to_json())
            .unwrap_or(serde_json::json!(null));

        // Top endpoints by count
        let endpoints: Vec<serde_json::Value> = self.inner.endpoints.lock()
            .map(|eps| {
                let mut sorted: Vec<_> = eps.iter()
                    .map(|(path, m)| {
                        let avg_ms = if m.count > 0 {
                            (m.total_latency_us as f64 / m.count as f64) / 1000.0
                        } else {
                            0.0
                        };
                        serde_json::json!({
                            "path": path,
                            "count": m.count,
                            "errors": m.errors,
                            "avg_ms": (avg_ms * 100.0).round() / 100.0,
                        })
                    })
                    .collect();
                sorted.sort_by(|a, b| {
                    b["count"].as_u64().cmp(&a["count"].as_u64())
                });
                sorted.truncate(20);
                sorted
            })
            .unwrap_or_default();

        let rps = if uptime_secs > 0 {
            (total as f64 / uptime_secs as f64 * 100.0).round() / 100.0
        } else {
            0.0
        };

        let mut result = serde_json::json!({
            "uptime_seconds": uptime_secs,
            "total_requests": total,
            "active_requests": active,
            "requests_per_second": rps,
            "status_codes": status_codes,
            "latency": latency,
            "top_endpoints": endpoints,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        });

        if let Some(pool) = pool_status {
            result["pool"] = pool;
        }

        result
    }
}

/// Normalize a path by replacing UUID segments with `:id`.
fn normalize_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.len() == 36 && segment.chars().filter(|c| *c == '-').count() == 4 {
                ":id"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path("/bpe/api/entities/550e8400-e29b-41d4-a716-446655440000"),
            "/bpe/api/entities/:id"
        );
        assert_eq!(normalize_path("/bpe/health"), "/bpe/health");
        assert_eq!(
            normalize_path("/bpe/api/workflows/executions/550e8400-e29b-41d4-a716-446655440000/timeline"),
            "/bpe/api/workflows/executions/:id/timeline"
        );
    }

    #[test]
    fn test_metrics_basic() {
        let m = Metrics::new();
        m.request_start();
        m.request_end(200, "/bpe/health", 1500); // 1.5ms
        m.request_start();
        m.request_end(404, "/bpe/api/entities/abc", 3200);

        let snap = m.snapshot(None);
        assert_eq!(snap["total_requests"], 2);
        assert_eq!(snap["status_codes"]["2xx"], 1);
        assert_eq!(snap["status_codes"]["4xx"], 1);
    }

    #[test]
    fn test_latency_histogram() {
        let mut h = LatencyHistogram::new();
        h.record(500);   // 0.5ms → <=1ms bucket
        h.record(5_000); // 5ms → <=5ms bucket
        h.record(50_000); // 50ms → <=50ms bucket
        h.record(15_000_000); // 15s → overflow

        assert_eq!(h.total_count, 4);
        let json = h.to_json();
        assert_eq!(json["count"], 4);
    }
}
