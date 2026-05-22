use crate::config::BpeConfig;
use crate::error::BpeError;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

/// PostgreSQL connection pool wrapper.
#[derive(Clone)]
pub struct PgPool {
    pool: Pool,
}

impl PgPool {
    /// Create a new connection pool from BPE configuration.
    pub async fn new(config: &BpeConfig) -> Result<Self, BpeError> {
        let mut pg_config = tokio_postgres::Config::new();
        pg_config.host(&config.pg_host);
        pg_config.port(config.pg_port);
        pg_config.dbname(&config.pg_dbname);
        pg_config.user(&config.pg_user);
        if !config.pg_password.is_empty() {
            pg_config.password(&config.pg_password);
        }

        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, NoTls, mgr_config);

        let pool = Pool::builder(mgr)
            .max_size(config.pg_pool_max_size)
            .runtime(Runtime::Tokio1)
            .build()
            .map_err(|e| BpeError::Database(format!("Failed to build pool: {e}")))?;

        // Test connection
        let client = pool
            .get()
            .await
            .map_err(|e| BpeError::Database(format!("Failed to connect to PostgreSQL: {e}")))?;
        client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| BpeError::Database(format!("Connection test failed: {e}")))?;

        tracing::info!(
            "PostgreSQL pool created (host={}, db={}, max_size={})",
            config.pg_host,
            config.pg_dbname,
            config.pg_pool_max_size
        );

        Ok(Self { pool })
    }

    /// Get a connection from the pool.
    pub async fn get(&self) -> Result<deadpool_postgres::Client, BpeError> {
        self.pool
            .get()
            .await
            .map_err(|e| BpeError::Database(format!("Pool error: {e}")))
    }

    /// Pool status for health checks.
    pub fn status(&self) -> PoolStatus {
        let status = self.pool.status();
        PoolStatus {
            size: status.size as u32,
            available: status.available as u32,
            waiting: status.waiting as u32,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct PoolStatus {
    pub size: u32,
    pub available: u32,
    pub waiting: u32,
}
