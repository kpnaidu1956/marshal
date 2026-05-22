use std::sync::Arc;

use axum::{middleware, routing::get, Router};
use bpe_core::{
    auth::{require_auth, JwtSecret},
    config::BpeConfig,
    db::PgPool,
    integration::ruflo::RufloClient,
    metrics::Metrics,
    permissions::PermissionService,
};

pub mod routes;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    pool: PgPool,
    config: BpeConfig,
    metrics: Metrics,
    ruflo_client: RufloClient,
    permissions: PermissionService,
}

impl AppState {
    /// Create a new AppState (used by main and tests).
    pub fn new(pool: PgPool, config: BpeConfig, metrics: Metrics) -> Self {
        let ruflo_client = RufloClient::new(&config.ruflo_base_url);
        Self {
            inner: Arc::new(AppStateInner {
                pool,
                config,
                metrics,
                ruflo_client,
                permissions: PermissionService::new(),
            }),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    pub fn config(&self) -> &BpeConfig {
        &self.inner.config
    }

    pub fn metrics(&self) -> &Metrics {
        &self.inner.metrics
    }

    pub fn ruflo_client(&self) -> &RufloClient {
        &self.inner.ruflo_client
    }

    pub fn permissions(&self) -> &PermissionService {
        &self.inner.permissions
    }
}

/// Build the full BPE application router.
///
/// Returns a `Router` with:
/// - `/bpe/health` — public health check
/// - `/bpe/api/*` — JWT-protected API endpoints
pub fn build_router(state: AppState, jwt_secret: JwtSecret) -> Router {
    let protected_routes = routes::api_routes()
        .layer(middleware::from_fn(require_auth))
        .layer(axum::Extension(jwt_secret));

    Router::new()
        .route("/bpe/health", get(routes::health::health_check))
        .nest("/bpe/api", protected_routes)
        .with_state(state)
}
