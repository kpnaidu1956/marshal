use crate::db::PgPool;
use crate::error::BpeError;
use super::models::{AuditEvent, NewAuditEvent};

/// Records audit events into bpe.audit_events.
pub struct AuditLogger;

impl AuditLogger {
    /// Log a single audit event.
    pub async fn log(pool: &PgPool, event: &NewAuditEvent) -> Result<AuditEvent, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.audit_events
                    (organization_id, event_type, resource_type, resource_id,
                     actor_user_id, actor_type, before_state, after_state, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id, organization_id, event_type, resource_type, resource_id,
                           actor_user_id, actor_type, before_state, after_state, metadata,
                           ip_address::text, is_reversed, reversed_by_event_id, reversal_reason, created_at",
                &[
                    &event.organization_id, &event.event_type, &event.resource_type,
                    &event.resource_id, &event.actor_user_id, &event.actor_type,
                    &event.before_state, &event.after_state, &event.metadata,
                ],
            )
            .await?;

        Ok(row_to_audit_event(&row))
    }

    /// Log a state change with automatic before/after capture.
    pub async fn log_change(
        pool: &PgPool,
        org_id: uuid::Uuid,
        event_type: &str,
        resource_type: &str,
        resource_id: uuid::Uuid,
        actor_user_id: Option<uuid::Uuid>,
        before: Option<&serde_json::Value>,
        after: Option<&serde_json::Value>,
        metadata: serde_json::Value,
    ) -> Result<AuditEvent, BpeError> {
        Self::log(pool, &NewAuditEvent {
            organization_id: org_id,
            event_type: event_type.into(),
            resource_type: resource_type.into(),
            resource_id,
            actor_user_id,
            actor_type: if actor_user_id.is_some() { "user".into() } else { "system".into() },
            before_state: before.cloned(),
            after_state: after.cloned(),
            metadata,
            ip_address: None,
        }).await
    }

    /// Bulk insert audit events using a transaction with a prepared statement.
    /// This pipelines inserts and avoids N pool checkouts.
    pub async fn batch_log(pool: &PgPool, events: &[NewAuditEvent]) -> Result<Vec<i64>, BpeError> {
        if events.is_empty() {
            return Ok(vec![]);
        }
        let mut client = pool.get().await?;
        let txn = client.transaction().await?;
        let stmt = txn
            .prepare(
                "INSERT INTO bpe.audit_events
                    (organization_id, event_type, resource_type, resource_id,
                     actor_user_id, actor_type, before_state, after_state, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id",
            )
            .await?;

        let mut ids = Vec::with_capacity(events.len());
        for event in events {
            let row = txn
                .query_one(
                    &stmt,
                    &[
                        &event.organization_id, &event.event_type, &event.resource_type,
                        &event.resource_id, &event.actor_user_id, &event.actor_type,
                        &event.before_state, &event.after_state, &event.metadata,
                    ],
                )
                .await?;
            ids.push(row.get::<_, i64>(0));
        }
        txn.commit().await?;

        Ok(ids)
    }
}

pub(crate) fn row_to_audit_event(row: &tokio_postgres::Row) -> AuditEvent {
    AuditEvent {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        event_type: row.get("event_type"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        actor_user_id: row.get("actor_user_id"),
        actor_type: row.get("actor_type"),
        before_state: row.get("before_state"),
        after_state: row.get("after_state"),
        metadata: row.get("metadata"),
        ip_address: row.try_get::<_, String>("ip_address").ok(),
        is_reversed: row.get("is_reversed"),
        reversed_by_event_id: row.get("reversed_by_event_id"),
        reversal_reason: row.get("reversal_reason"),
        created_at: row.get("created_at"),
    }
}
