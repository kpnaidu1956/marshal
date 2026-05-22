use std::env;

/// BPE service configuration, loaded from environment variables.
#[derive(Debug, Clone)]
pub struct BpeConfig {
    /// PostgreSQL host (default: localhost)
    pub pg_host: String,
    /// PostgreSQL port (default: 5432)
    pub pg_port: u16,
    /// PostgreSQL database name (default: goalrag)
    pub pg_dbname: String,
    /// PostgreSQL user (default: postgres)
    pub pg_user: String,
    /// PostgreSQL password
    pub pg_password: String,
    /// Connection pool max size (default: 16)
    pub pg_pool_max_size: usize,
    /// BPE server listen host (default: 0.0.0.0)
    pub listen_host: String,
    /// BPE server listen port (default: 8090)
    pub listen_port: u16,
    /// JWT secret for token validation (shared with goal-rag)
    pub jwt_secret: String,
    /// goal-rag base URL for RAG queries (default: http://localhost:8080)
    pub rag_base_url: String,
    /// Credential encryption key (32 bytes, base64-encoded). Required for production.
    pub credential_encryption_key: Option<String>,
    /// Allowed CORS origins (comma-separated). Empty = same-origin only.
    pub cors_origins: Vec<String>,
    /// Ruflo sidecar base URL (default: http://localhost:8100)
    pub ruflo_base_url: String,
}

impl BpeConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            pg_host: env::var("BPE_PG_HOST")
                .or_else(|_| env::var("PG_HOST"))
                .unwrap_or_else(|_| "localhost".into()),
            pg_port: env::var("BPE_PG_PORT")
                .or_else(|_| env::var("PG_PORT"))
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5432),
            pg_dbname: env::var("BPE_PG_DBNAME")
                .or_else(|_| env::var("PG_DBNAME"))
                .unwrap_or_else(|_| "marshal_db".into()),
            pg_user: env::var("BPE_PG_USER")
                .or_else(|_| env::var("PG_USER"))
                .unwrap_or_else(|_| "postgres".into()),
            pg_password: env::var("BPE_PG_PASSWORD")
                .or_else(|_| env::var("PG_PASSWORD"))
                .unwrap_or_default(),
            pg_pool_max_size: env::var("BPE_PG_POOL_MAX_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(16),
            listen_host: env::var("BPE_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            listen_port: env::var("BPE_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8090),
            jwt_secret: env::var("POSTGREST_JWT_SECRET")
                .expect("POSTGREST_JWT_SECRET must be set"),
            rag_base_url: env::var("BPE_RAG_URL")
                .or_else(|_| env::var("RAG_BASE_URL"))
                .unwrap_or_else(|_| "http://localhost:8080".into()),
            credential_encryption_key: env::var("BPE_CREDENTIAL_KEY").ok(),
            cors_origins: env::var("BPE_CORS_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            ruflo_base_url: env::var("RUFLO_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8100".into()),
        }
    }

    pub fn listen_address(&self) -> String {
        format!("{}:{}", self.listen_host, self.listen_port)
    }
}
