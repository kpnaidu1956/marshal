use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::AppState;

/// Public health check — verifies DB liveness.
pub async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let pool_status = state.pool().status();

    // Verify DB is actually responding
    let db_ok = match state.pool().get().await {
        Ok(client) => client.simple_query("SELECT 1").await.is_ok(),
        Err(_) => false,
    };

    let status = if db_ok { "ok" } else { "degraded" };

    Json(json!({
        "status": status,
        "service": "bpe-server",
        "version": env!("CARGO_PKG_VERSION"),
        "db_connected": db_ok,
        "pool": pool_status,
        "uptime_seconds": state.metrics().snapshot(None)["uptime_seconds"],
    }))
}

/// Authenticated health check — returns additional info.
pub async fn authenticated_health(State(state): State<AppState>) -> Json<Value> {
    let pool_status = state.pool().status();

    let db_ok = match state.pool().get().await {
        Ok(client) => client.simple_query("SELECT 1").await.is_ok(),
        Err(_) => false,
    };

    Json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "service": "bpe-server",
        "version": env!("CARGO_PKG_VERSION"),
        "db_connected": db_ok,
        "pool": pool_status,
        "rag_url": state.config().rag_base_url,
        "uptime_seconds": state.metrics().snapshot(None)["uptime_seconds"],
    }))
}

/// Public metrics endpoint — request counts, latency, error rates.
pub async fn metrics_endpoint(State(state): State<AppState>) -> Json<Value> {
    let pool_status = serde_json::to_value(state.pool().status()).ok();
    let snapshot = state.metrics().snapshot(pool_status);
    Json(snapshot)
}
