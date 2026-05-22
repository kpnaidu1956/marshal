//! PostgreSQL configuration

use serde::{Deserialize, Serialize};

/// PostgreSQL connection configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// Database host
    pub host: String,
    /// Database port
    pub port: u16,
    /// Database name
    pub database: String,
    /// Database user
    pub user: String,
    /// Database password
    #[serde(skip_serializing)]
    pub password: String,
    /// Connection pool size
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
    /// Tables to listen for changes (empty = all allowed tables)
    #[serde(default)]
    pub listen_tables: Vec<String>,
    /// Enable learning from database changes
    #[serde(default = "default_learning_enabled")]
    pub learning_enabled: bool,
    /// Batch size for learning (process N changes before running pattern analysis)
    #[serde(default = "default_batch_size")]
    pub learning_batch_size: usize,
    /// Schema name (default: "api")
    #[serde(default = "default_schema")]
    pub schema: String,
    /// Use pgvector for vector storage instead of local HNSW (default: false)
    /// When enabled, vectors are stored in PostgreSQL using pgvector extension.
    /// Benefits: instant startup (no index rebuild), persistent storage, scales with disk.
    #[serde(default)]
    pub use_pgvector: bool,
}

fn default_pool_size() -> usize {
    15
}

fn default_learning_enabled() -> bool {
    true
}

fn default_batch_size() -> usize {
    10
}

fn default_schema() -> String {
    "api".to_string()
}

impl std::fmt::Debug for PostgresConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password", &"[REDACTED]")
            .field("pool_size", &self.pool_size)
            .field("listen_tables", &self.listen_tables)
            .field("learning_enabled", &self.learning_enabled)
            .field("learning_batch_size", &self.learning_batch_size)
            .field("schema", &self.schema)
            .field("use_pgvector", &self.use_pgvector)
            .finish()
    }
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "marshal_db".to_string(),
            user: "postgres".to_string(),
            password: String::new(),
            pool_size: default_pool_size(),
            listen_tables: Vec::new(),
            learning_enabled: default_learning_enabled(),
            learning_batch_size: default_batch_size(),
            schema: default_schema(),
            use_pgvector: false,
        }
    }
}

impl PostgresConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("POSTGRES_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(5432),
            database: std::env::var("POSTGRES_DATABASE").unwrap_or_else(|_| "marshal_db".to_string()),
            user: std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
            password: std::env::var("POSTGRES_PASSWORD").unwrap_or_default(),
            pool_size: std::env::var("POSTGRES_POOL_SIZE")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(default_pool_size()),
            listen_tables: std::env::var("POSTGRES_LISTEN_TABLES")
                .ok()
                .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            learning_enabled: std::env::var("POSTGRES_LEARNING_ENABLED")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default_learning_enabled()),
            learning_batch_size: std::env::var("POSTGRES_LEARNING_BATCH_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default_batch_size()),
            schema: std::env::var("POSTGRES_SCHEMA").unwrap_or_else(|_| default_schema()),
            use_pgvector: std::env::var("USE_PGVECTOR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(false),
        }
    }

    /// Build connection string
    pub fn connection_string(&self) -> String {
        format!(
            "host={} port={} dbname={} user={} password={}",
            self.host, self.port, self.database, self.user, self.password
        )
    }

    /// Get tables to listen to (defaults if empty)
    pub fn tables_to_listen(&self) -> Vec<String> {
        if self.listen_tables.is_empty() {
            // Default tables from the application schema
            vec![
                "tasks".to_string(),
                "goals".to_string(),
                "users".to_string(),
                "organizations".to_string(),
                "documents".to_string(),
                "conversations".to_string(),
                "chat_messages".to_string(),
                "task_comments".to_string(),
                "task_attachments".to_string(),
                "messages".to_string(),
                "categories".to_string(),
                "groups".to_string(),
            ]
        } else {
            self.listen_tables.clone()
        }
    }
}
