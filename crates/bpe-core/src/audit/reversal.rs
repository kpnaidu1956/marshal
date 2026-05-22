use crate::db::PgPool;
use crate::error::BpeError;
use super::logger::AuditLogger;
use super::models::{NewAuditEvent, ReversalResult};
use super::query::AuditQuery;

/// Handles reversals of audit events (soft reversal pattern).
pub struct ReversalEngine;

impl ReversalEngine {
    /// Check if an event can be reversed.
    pub async fn can_reverse(pool: &PgPool, event_id: i64) -> Result<bool, BpeError> {
        let event = AuditQuery::get(pool, event_id).await?;
        // Cannot reverse if already reversed or if it's a reversal event itself
        Ok(!event.is_reversed && !event.event_type.starts_with("reversal."))
    }

    /// Reverse an audit event. Marks original as reversed and creates a compensating event.
    pub async fn reverse(
        pool: &PgPool,
        event_id: i64,
        reason: &str,
        actor_user_id: Option<uuid::Uuid>,
    ) -> Result<ReversalResult, BpeError> {
        let original = AuditQuery::get(pool, event_id).await?;

        if original.is_reversed {
            return Err(BpeError::Conflict("Event has already been reversed".into()));
        }
        if original.event_type.starts_with("reversal.") {
            return Err(BpeError::BadRequest("Cannot reverse a reversal event".into()));
        }

        let client = pool.get().await?;

        // Mark original as reversed
        client
            .execute(
                "UPDATE bpe.audit_events SET is_reversed = true, reversal_reason = $1 WHERE id = $2",
                &[&reason, &event_id],
            )
            .await?;

        // Apply compensation for entity updates (restore before_state)
        let compensation_applied = if original.event_type == "entity.updated" {
            if let Some(ref before) = original.before_state {
                Self::restore_entity(pool, original.resource_id, before).await?;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Create the reversal audit event
        let reversal = AuditLogger::log(pool, &NewAuditEvent {
            organization_id: original.organization_id,
            event_type: format!("reversal.{}", original.event_type),
            resource_type: original.resource_type.clone(),
            resource_id: original.resource_id,
            actor_user_id,
            actor_type: if actor_user_id.is_some() { "user".into() } else { "system".into() },
            before_state: original.after_state,
            after_state: original.before_state,
            metadata: serde_json::json!({
                "reason": reason,
                "original_event_id": event_id,
                "compensation_applied": compensation_applied,
            }),
            ip_address: None,
        }).await?;

        Ok(ReversalResult {
            original_event_id: event_id,
            reversal_event_id: reversal.id,
            resource_type: original.resource_type,
            resource_id: original.resource_id,
            compensation_applied,
        })
    }

    /// Restore an entity to its before_state (for entity.updated reversals).
    async fn restore_entity(
        pool: &PgPool,
        entity_id: uuid::Uuid,
        before_state: &serde_json::Value,
    ) -> Result<(), BpeError> {
        let client = pool.get().await?;

        // Extract fields from before_state
        let display_name = before_state.get("display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let empty_obj = serde_json::json!({});
        let attributes = before_state.get("attributes")
            .unwrap_or(&empty_obj);
        let status = before_state.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("active");

        client
            .execute(
                "UPDATE bpe.entities SET display_name = $1, attributes = $2, status = $3, updated_at = now()
                 WHERE id = $4",
                &[&display_name, attributes, &status, &entity_id],
            )
            .await?;

        Ok(())
    }
}
