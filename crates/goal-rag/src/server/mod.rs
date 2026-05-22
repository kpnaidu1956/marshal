//! HTTP server for the RAG system

pub mod middleware;
pub mod notifications;
pub mod routes;
pub mod state;
pub mod trial_lifecycle;

use axum::{routing::get, Router};
use std::net::SocketAddr;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tower_http::services::{ServeDir, ServeFile};

use crate::config::RagConfig;
use crate::error::Result;
use state::AppState;

/// RAG HTTP Server
pub struct RagServer {
    config: RagConfig,
    state: AppState,
}

impl RagServer {
    /// Create a new RAG server
    pub async fn new(config: RagConfig) -> Result<Self> {
        let state = AppState::new(config.clone()).await?;
        Ok(Self { config, state })
    }

    /// Create with default configuration
    pub async fn default() -> Result<Self> {
        Self::new(RagConfig::default()).await
    }

    /// Build the router with all routes
    fn build_router(&self) -> Router {
        // CORS — restrict to known origins in production, allow any in dev
        let trial_domain = std::env::var("TRIAL_DOMAIN").unwrap_or_default();
        let cors = if trial_domain.is_empty() {
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        } else {
            CorsLayer::new()
                .allow_origin([
                    format!("https://{}", trial_domain).parse().unwrap(),
                    "http://localhost:5173".parse().unwrap(), // vite dev
                ])
                .allow_methods(Any)
                .allow_headers(Any)
                .allow_credentials(true)
        };

        

        let router = Router::new()
            // Health check
            .route("/health", get(health_check))
            .route("/ready", get(readiness))
            // API routes with body limit for multipart uploads
            .nest("/api", routes::api_routes(self.config.server.max_upload_size))
            .with_state(self.state.clone());

        // Serve React dashboard from ./dashboard/dist/ (preferred) or ./dashboard/
        let dist_dir = std::path::PathBuf::from("dashboard/dist");
        let legacy_dir = std::path::PathBuf::from("dashboard");
        let router = if dist_dir.is_dir() {
            tracing::info!("Serving React dashboard from ./dashboard/dist/");
            router.nest_service("/dashboard", ServeDir::new(&dist_dir))
        } else if legacy_dir.is_dir() {
            tracing::info!("Serving dashboard from ./dashboard/");
            router.nest_service("/dashboard", ServeDir::new(&legacy_dir))
        } else {
            router
        };

        // Serve Marshal UI at /marshal — prefer React build over Leptos WASM
        let react_dist = std::path::PathBuf::from("marshal-ui-react/dist");
        let marshal_dist = std::path::PathBuf::from("marshal-ui/dist");
        let router = if react_dist.is_dir() {
            let index_html = react_dist.join("index.html");
            tracing::info!("Serving React Marshal UI from ./marshal-ui-react/dist/ at /marshal");
            router.nest_service(
                "/marshal",
                ServeDir::new(&react_dist)
                    .fallback(ServeFile::new(index_html)),
            )
        } else if marshal_dist.is_dir() {
            let index_html = marshal_dist.join("index.html");
            tracing::info!("Serving Leptos Marshal UI from ./marshal-ui/dist/ at /marshal");
            router.nest_service(
                "/marshal",
                ServeDir::new(&marshal_dist)
                    .fallback(ServeFile::new(index_html)),
            )
        } else {
            router
        };

        router
            // Middleware layers (order matters - applied bottom to top)
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(cors)
    }

    /// Start the server
    pub async fn start(self) -> Result<()> {
        let addr: SocketAddr = format!("{}:{}", self.config.server.host, self.config.server.port)
            .parse()
            .map_err(|e| crate::error::Error::Config(format!("Invalid address: {}", e)))?;

        // TRIAL: Spawn lifecycle background task
        #[cfg(feature = "postgres")]
        if let Some(pool) = self.state.pg_pool() {
            let resend = notifications::ResendClient::from_env()
                .map(std::sync::Arc::new);
            trial_lifecycle::spawn_lifecycle_task(pool.clone(), resend);
        }

        let router = self.build_router();

        tracing::info!("Starting RAG server on http://{}", addr);
        tracing::info!("API documentation: http://{}/api", addr);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| crate::error::Error::Config(format!("Failed to bind: {}", e)))?;

        axum::serve(listener, router)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Server error: {}", e)))?;

        Ok(())
    }

    /// Get the server address
    pub fn address(&self) -> String {
        format!("{}:{}", self.config.server.host, self.config.server.port)
    }
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// Readiness check endpoint
async fn readiness(state: axum::extract::State<AppState>) -> axum::http::StatusCode {
    if state.is_ready() {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    }
}
