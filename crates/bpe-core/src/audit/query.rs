use crate::db::PgPool;
use crate::error::BpeError;
use super::logger::row_to_audit_event;
use super::models::{AuditEvent, AuditQueryParams, PaginatedAuditEvents};
use uuid::Uuid;

/// Audit trail query operations.
pub struct AuditQuery;

impl AuditQuery {
    /// Search audit events with filtering and pagination.
    pub async fn search(pool: &PgPool, org_id: Uuid, params: &AuditQueryParams) -> Result<PaginatedAuditEvents, BpeError> {
        let client = pool.get().await?;
        let page = params.page.unwrap_or(1).max(1);
        let per_page = params.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        // Build WHERE clauses dynamically but use static query branches for Send safety.
        // We always have organization_id. Optional filters: resource_type, resource_id,
        // event_type, actor_user_id, from, to.
        // For simplicity, build a single parameterized query with coalesce-style filtering.
        let count_row = client
            .query_one(
                "SELECT count(*) FROM bpe.audit_events
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR resource_type = $2)
                   AND ($3::uuid IS NULL OR resource_id = $3)
                   AND ($4::text IS NULL OR event_type = $4)
                   AND ($5::uuid IS NULL OR actor_user_id = $5)
                   AND ($6::timestamptz IS NULL OR created_at >= $6)
                   AND ($7::timestamptz IS NULL OR created_at <= $7)",
                &[
                    &org_id,
                    &params.resource_type,
                    &params.resource_id,
                    &params.event_type,
                    &params.actor_user_id,
                    &parse_ts(&params.from),
                    &parse_ts(&params.to),
                ],
            )
            .await?;
        let total: i64 = count_row.get(0);

        let rows = client
            .query(
                "SELECT id, organization_id, event_type, resource_type, resource_id,
                        actor_user_id, actor_type, before_state, after_state, metadata,
                        ip_address::text, is_reversed, reversed_by_event_id, reversal_reason, created_at
                 FROM bpe.audit_events
                 WHERE organization_id = $1
                   AND ($2::text IS NULL OR resource_type = $2)
                   AND ($3::uuid IS NULL OR resource_id = $3)
                   AND ($4::text IS NULL OR event_type = $4)
                   AND ($5::uuid IS NULL OR actor_user_id = $5)
                   AND ($6::timestamptz IS NULL OR created_at >= $6)
                   AND ($7::timestamptz IS NULL OR created_at <= $7)
                 ORDER BY created_at DESC
                 LIMIT $8 OFFSET $9",
                &[
                    &org_id,
                    &params.resource_type,
                    &params.resource_id,
                    &params.event_type,
                    &params.actor_user_id,
                    &parse_ts(&params.from),
                    &parse_ts(&params.to),
                    &per_page,
                    &offset,
                ],
            )
            .await?;

        let data = rows.iter().map(row_to_audit_event).collect();
        Ok(PaginatedAuditEvents { data, page, per_page, total })
    }

    /// Get all audit events for a specific resource.
    pub async fn by_resource(pool: &PgPool, resource_type: &str, resource_id: Uuid) -> Result<Vec<AuditEvent>, BpeError> {
        let client = pool.get().await?;
        let rows = client
            .query(
                "SELECT id, organization_id, event_type, resource_type, resource_id,
                        actor_user_id, actor_type, before_state, after_state, metadata,
                        ip_address::text, is_reversed, reversed_by_event_id, reversal_reason, created_at
                 FROM bpe.audit_events
                 WHERE resource_type = $1 AND resource_id = $2
                 ORDER BY created_at DESC",
                &[&resource_type, &resource_id],
            )
            .await?;

        Ok(rows.iter().map(row_to_audit_event).collect())
    }

    /// Get all audit events for an execution: execution-level events + step-level events.
    /// Step events store execution_id in their metadata JSON.
    pub async fn by_execution(pool: &PgPool, execution_id: Uuid) -> Result<Vec<AuditEvent>, BpeError> {
        let client = pool.get().await?;
        let exec_id_str = execution_id.to_string();
        let rows = client
            .query(
                "SELECT id, organization_id, event_type, resource_type, resource_id,
                        actor_user_id, actor_type, before_state, after_state, metadata,
                        ip_address::text, is_reversed, reversed_by_event_id, reversal_reason, created_at
                 FROM bpe.audit_events
                 WHERE (resource_type = 'workflow_execution' AND resource_id = $1)
                    OR (resource_type = 'workflow_step' AND metadata->>'execution_id' = $2)
                 ORDER BY created_at ASC",
                &[&execution_id, &exec_id_str],
            )
            .await?;

        Ok(rows.iter().map(row_to_audit_event).collect())
    }

    /// Get all audit events related to an entity (as resource or in metadata).
    pub async fn by_entity(pool: &PgPool, entity_id: Uuid) -> Result<Vec<AuditEvent>, BpeError> {
        let client = pool.get().await?;
        let rows = client
            .query(
                "SELECT id, organization_id, event_type, resource_type, resource_id,
                        actor_user_id, actor_type, before_state, after_state, metadata,
                        ip_address::text, is_reversed, reversed_by_event_id, reversal_reason, created_at
                 FROM bpe.audit_events
                 WHERE (resource_type = 'entity' AND resource_id = $1)
                    OR (metadata->>'entity_id' = $2)
                 ORDER BY created_at DESC",
                &[&entity_id, &entity_id.to_string()],
            )
            .await?;

        Ok(rows.iter().map(row_to_audit_event).collect())
    }

    /// Get a single audit event by ID.
    pub async fn get(pool: &PgPool, event_id: i64) -> Result<AuditEvent, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, event_type, resource_type, resource_id,
                        actor_user_id, actor_type, before_state, after_state, metadata,
                        ip_address::text, is_reversed, reversed_by_event_id, reversal_reason, created_at
                 FROM bpe.audit_events WHERE id = $1",
                &[&event_id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Audit event {event_id} not found")))?;

        Ok(row_to_audit_event(&row))
    }
}

fn parse_ts(s: &Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    s.as_ref().and_then(|v| {
        // Try full ISO 8601 first, then date-only
        chrono::DateTime::parse_from_rfc3339(v)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok()
            .or_else(|| {
                chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")
                    .ok()
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
            })
    })
}
