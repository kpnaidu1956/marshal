use std::time::Duration;

use axum::http::StatusCode;
use axum::middleware;
use bpe_core::{
    auth::JwtSecret,
    config::BpeConfig,
    db::{migrations, PgPool},
    metrics::Metrics,
    middleware::{rate_limit, request_id, track_metrics, RateLimiter},
};
use bpe_server::{build_router, AppState};
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured JSON logging when BPE_LOG_FORMAT=json, plain text otherwise
    let json_logs = std::env::var("BPE_LOG_FORMAT")
        .map(|v| v == "json")
        .unwrap_or(false);

    if json_logs {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "bpe_server=info,bpe_core=info,tower_http=info".into()),
            )
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "bpe_server=info,bpe_core=info,tower_http=debug".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    println!(
        r#"
╔═══════════════════════════════════════════════════════════╗
║              Business Process Engine (BPE)                ║
║          Workflow Automation & Entity Management          ║
║                  Phase 12: Hardened                       ║
╚═══════════════════════════════════════════════════════════╝
"#
    );

    let config = BpeConfig::from_env();
    let listen_addr = config.listen_address();

    tracing::info!("Connecting to PostgreSQL at {}:{}/{}", config.pg_host, config.pg_port, config.pg_dbname);
    let pool = PgPool::new(&config).await?;

    // Run migrations
    let client = pool.get().await?;
    migrations::run_migrations(&client).await?;
    drop(client);

    let jwt_secret = JwtSecret(config.jwt_secret.clone());
    let metrics = Metrics::new();
    let state = AppState::new(pool, config, metrics.clone());

    // Rate limiter: 200 requests per 60 seconds per IP
    let limiter = RateLimiter::new(200, 60);

    // Background cleanup task for rate limiter
    let limiter_cleanup = limiter.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;
            limiter_cleanup.cleanup();
        }
    });

    // Build CORS layer from config (before state is moved)
    let cors_layer = {
        let cors = CorsLayer::new()
            .allow_methods(Any)
            .allow_headers(Any);
        if state.config().cors_origins.is_empty() {
            cors
        } else {
            let origins: Vec<_> = state.config().cors_origins.iter()
                .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
                .collect();
            cors.allow_origin(origins)
        }
    };

    // Build router (core routing from lib.rs) + production middleware layers
    let app = build_router(state, jwt_secret)
        // Request ID for tracing
        .layer(middleware::from_fn(request_id))
        // Metrics tracking
        .layer(middleware::from_fn(track_metrics))
        .layer(axum::Extension(metrics))
        // Rate limiting
        .layer(middleware::from_fn(rate_limit))
        .layer(axum::Extension(limiter))
        // Request body size limit (2 MB)
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024))
        // Request timeout (30 seconds)
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        // HTTP tracing
        .layer(TraceLayer::new_for_http())
        // CORS
        .layer(cors_layer);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("BPE server listening on {listen_addr}");
    println!("\nHardening:");
    println!("  Rate limit:     200 req/min per IP");
    println!("  Body limit:     2 MB");
    println!("  Timeout:        30 seconds");
    println!("  Request IDs:    x-request-id header");
    println!("  Metrics:        GET /bpe/api/metrics (auth required)");
    println!("  Logging:        {}", if json_logs { "JSON" } else { "plain text" });
    println!("\nEndpoints:");
    println!("  GET  /bpe/health         - Health check (public)");
    println!("  GET  /bpe/api/metrics     - Metrics (auth required)");
    println!("  GET  /bpe/api/...        - Protected API endpoints");
    println!("\nPress Ctrl+C to stop\n");

    // Graceful shutdown on SIGTERM
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Shutdown signal received, draining connections...");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    tracing::info!("BPE server stopped");
    Ok(())
}
