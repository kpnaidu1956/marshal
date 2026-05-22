//! PostgreSQL connection pool

use deadpool_postgres::{Config, Pool, Runtime, ManagerConfig, RecyclingMethod};
use tokio_postgres::NoTls;

use super::config::PostgresConfig;
use crate::error::{Error, Result};

/// PostgreSQL connection pool wrapper
#[derive(Clone)]
pub struct PgPool {
    pool: Pool,
    config: PostgresConfig,
}

impl PgPool {
    /// Create a new connection pool
    pub async fn new(mut config: PostgresConfig) -> Result<Self> {
        // Allow env vars to override TOML config
        if let Ok(pw) = std::env::var("POSTGRES_PASSWORD") {
            if !pw.is_empty() {
                config.password = pw;
            }
        }
        if let Ok(val) = std::env::var("USE_PGVECTOR") {
            if let Ok(b) = val.parse::<bool>() {
                config.use_pgvector = b;
            }
        }

        let mut pg_config = Config::new();
        pg_config.host = Some(config.host.clone());
        pg_config.port = Some(config.port);
        pg_config.dbname = Some(config.database.clone());
        pg_config.user = Some(config.user.clone());
        pg_config.password = Some(config.password.clone());

        pg_config.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        pg_config.pool = Some(deadpool_postgres::PoolConfig {
            max_size: config.pool_size,
            ..Default::default()
        });

        let pool = pg_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| Error::Internal(format!("Failed to create PostgreSQL pool: {}", e)))?;

        // Test connection
        let client = pool.get().await
            .map_err(|e| Error::Internal(format!("Failed to connect to PostgreSQL: {}", e)))?;

        // Verify connection with simple query
        client.simple_query("SELECT 1").await
            .map_err(|e| Error::Internal(format!("PostgreSQL connection test failed: {}", e)))?;

        tracing::info!(
            host = %config.host,
            port = config.port,
            database = %config.database,
            pool_size = config.pool_size,
            "PostgreSQL connection pool initialized"
        );

        Ok(Self { pool, config })
    }

    /// Get a connection from the pool
    pub async fn get(&self) -> Result<deadpool_postgres::Client> {
        self.pool.get().await
            .map_err(|e| Error::Internal(format!("Failed to get PostgreSQL connection: {}", e)))
    }

    /// Get the configuration
    pub fn config(&self) -> &PostgresConfig {
        &self.config
    }

    /// Get pool status
    pub fn status(&self) -> PoolStatus {
        let status = self.pool.status();
        PoolStatus {
            size: status.size,
            available: status.available,
            waiting: status.waiting,
        }
    }

    /// Create a dedicated connection for LISTEN/NOTIFY
    /// Returns the client and a receiver for notifications
    pub async fn listen_connection(&self) -> Result<(tokio_postgres::Client, tokio::sync::mpsc::Receiver<tokio_postgres::Notification>)> {
        let (client, mut connection) = tokio_postgres::connect(
            &self.config.connection_string(),
            NoTls,
        ).await
            .map_err(|e| Error::Internal(format!("Failed to create listen connection: {}", e)))?;

        // Create channel for notifications
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Spawn the connection handler that forwards notifications
        tokio::spawn(async move {
            loop {
                let message = std::future::poll_fn(|cx| connection.poll_message(cx)).await;
                match message {
                    Some(Ok(tokio_postgres::AsyncMessage::Notification(notification))) => {
                        if tx.send(notification).await.is_err() {
                            tracing::debug!("Notification channel closed");
                            break;
                        }
                    }
                    Some(Ok(tokio_postgres::AsyncMessage::Notice(notice))) => {
                        tracing::debug!("PostgreSQL notice: {}", notice.message());
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::error!("PostgreSQL connection error: {}", e);
                        break;
                    }
                    None => {
                        tracing::info!("PostgreSQL connection closed");
                        break;
                    }
                }
            }
        });

        Ok((client, rx))
    }
}

/// Pool status information
#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub size: usize,
    pub available: usize,
    pub waiting: usize,
}
