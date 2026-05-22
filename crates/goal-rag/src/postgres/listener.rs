//! PostgreSQL LISTEN/NOTIFY change listener
//!
//! Listens for database changes in real-time using PostgreSQL's NOTIFY mechanism.
//! Requires triggers to be set up on the database tables (see setup_triggers.sql).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::pool::PgPool;
use super::config::PostgresConfig;
use crate::error::{Error, Result};

/// Type of database change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Insert,
    Update,
    Delete,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Insert => write!(f, "INSERT"),
            ChangeType::Update => write!(f, "UPDATE"),
            ChangeType::Delete => write!(f, "DELETE"),
        }
    }
}

impl TryFrom<&str> for ChangeType {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
        match s.to_uppercase().as_str() {
            "INSERT" => Ok(ChangeType::Insert),
            "UPDATE" => Ok(ChangeType::Update),
            "DELETE" => Ok(ChangeType::Delete),
            _ => Err(Error::Internal(format!("Unknown change type: {}", s))),
        }
    }
}

/// A database change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// Table name that changed
    pub table: String,
    /// Type of change
    pub change_type: ChangeType,
    /// Row ID (if available)
    pub row_id: Option<Uuid>,
    /// Organization ID (for multi-tenancy)
    pub organization_id: Option<Uuid>,
    /// The changed data as JSON
    pub data: serde_json::Value,
    /// Timestamp of the change
    pub timestamp: DateTime<Utc>,
}

/// Payload format from PostgreSQL NOTIFY
#[derive(Debug, Deserialize)]
struct NotifyPayload {
    table: String,
    action: String,
    row_id: Option<String>,
    organization_id: Option<String>,
    data: Option<serde_json::Value>,
}

/// Change listener that subscribes to PostgreSQL NOTIFY events
pub struct ChangeListener {
    config: PostgresConfig,
    pool: PgPool,
}

impl ChangeListener {
    /// Create a new change listener
    pub fn new(pool: PgPool) -> Self {
        let config = pool.config().clone();
        Self { config, pool }
    }

    /// Start listening for changes and send them to the provided channel
    pub async fn start(&self, tx: mpsc::Sender<ChangeEvent>) -> Result<()> {
        let (client, mut notification_rx) = self.pool.listen_connection().await?;
        let tables = self.config.tables_to_listen();
        let schema = &self.config.schema;

        // Validate schema name (prevents injection through config)
        if !schema.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::Internal(format!(
                "Invalid schema name '{}': only alphanumeric and underscore allowed",
                schema
            )));
        }

        // Subscribe to each table's notification channel
        // Channel names are sanitized to prevent SQL injection (only alphanumeric + underscore)
        for table in &tables {
            if !table.chars().all(|c| c.is_alphanumeric() || c == '_') {
                tracing::warn!(table = %table, "Skipping table with invalid characters in name");
                continue;
            }
            let channel = format!("{}_{}_changes", schema, table);
            // LISTEN channel names are identifiers, not values - quote them as identifiers
            let quoted_channel = format!("\"{}\"", channel.replace('"', "\"\""));
            client.execute(&format!("LISTEN {}", quoted_channel), &[]).await
                .map_err(|e| Error::Internal(format!("Failed to LISTEN on {}: {}", channel, e)))?;
            tracing::info!(channel = %channel, "Subscribed to PostgreSQL notifications");
        }

        // Also listen to a general channel for all changes
        let general_channel = format!("{}_all_changes", schema);
        let quoted_general = format!("\"{}\"", general_channel.replace('"', "\"\""));
        client.execute(&format!("LISTEN {}", quoted_general), &[]).await
            .map_err(|e| Error::Internal(format!("Failed to LISTEN on {}: {}", general_channel, e)))?;
        tracing::info!(channel = %general_channel, "Subscribed to general change notifications");

        tracing::info!(
            tables = ?tables,
            "PostgreSQL change listener started"
        );

        // Process notifications from the channel
        while let Some(notification) = notification_rx.recv().await {
            match self.parse_notification(&notification) {
                Ok(event) => {
                    tracing::debug!(
                        table = %event.table,
                        change_type = %event.change_type,
                        row_id = ?event.row_id,
                        "Received database change"
                    );

                    if tx.send(event).await.is_err() {
                        tracing::warn!("Change event channel closed, stopping listener");
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        channel = %notification.channel(),
                        payload = %notification.payload(),
                        error = %e,
                        "Failed to parse notification"
                    );
                }
            }
        }

        Ok(())
    }

    /// Parse a notification into a ChangeEvent
    fn parse_notification(&self, notification: &tokio_postgres::Notification) -> Result<ChangeEvent> {
        let payload: NotifyPayload = serde_json::from_str(notification.payload())
            .map_err(|e| Error::Internal(format!("Invalid notification payload: {}", e)))?;

        let change_type = ChangeType::try_from(payload.action.as_str())?;

        let row_id = payload.row_id
            .and_then(|s| Uuid::parse_str(&s).ok());

        let organization_id = payload.organization_id
            .and_then(|s| Uuid::parse_str(&s).ok());

        Ok(ChangeEvent {
            table: payload.table,
            change_type,
            row_id,
            organization_id,
            data: payload.data.unwrap_or(serde_json::Value::Null),
            timestamp: Utc::now(),
        })
    }

    /// Generate SQL to create notification triggers
    /// This should be run once to set up the database
    ///
    /// # Panics
    /// Panics if schema or table names contain characters other than alphanumeric/underscore.
    /// These names come from configuration and should be validated at startup.
    pub fn generate_trigger_sql(&self) -> String {
        let tables = self.config.tables_to_listen();
        let schema = &self.config.schema;

        // Validate schema name before using in SQL generation
        assert!(
            schema.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "Schema name '{}' contains invalid characters", schema
        );
        // Validate all table names
        for table in &tables {
            assert!(
                table.chars().all(|c| c.is_alphanumeric() || c == '_'),
                "Table name '{}' contains invalid characters", table
            );
        }

        let mut sql = String::new();

        // Create the notification function
        sql.push_str(&format!(r#"
-- Function to send notifications on table changes
CREATE OR REPLACE FUNCTION {schema}.notify_change()
RETURNS TRIGGER AS $$
DECLARE
    payload JSON;
    row_data JSON;
    org_id TEXT;
BEGIN
    -- Get the row data based on operation
    IF TG_OP = 'DELETE' THEN
        row_data := row_to_json(OLD);
        org_id := OLD.organization_id::TEXT;
    ELSE
        row_data := row_to_json(NEW);
        org_id := NEW.organization_id::TEXT;
    END IF;

    -- Build the payload
    payload := json_build_object(
        'table', TG_TABLE_NAME,
        'action', TG_OP,
        'row_id', CASE WHEN TG_OP = 'DELETE' THEN OLD.id::TEXT ELSE NEW.id::TEXT END,
        'organization_id', org_id,
        'data', row_data
    );

    -- Notify on table-specific channel
    PERFORM pg_notify('{schema}_' || TG_TABLE_NAME || '_changes', payload::TEXT);

    -- Also notify on general channel
    PERFORM pg_notify('{schema}_all_changes', payload::TEXT);

    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

"#));

        // Create triggers for each table
        for table in &tables {
            sql.push_str(&format!(r#"
-- Trigger for {schema}.{table}
DROP TRIGGER IF EXISTS {table}_notify_trigger ON {schema}.{table};
CREATE TRIGGER {table}_notify_trigger
    AFTER INSERT OR UPDATE OR DELETE ON {schema}.{table}
    FOR EACH ROW EXECUTE FUNCTION {schema}.notify_change();

"#));
        }

        sql
    }
}
