use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An audit event recording a state change in the BPE system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: i64,
    pub organization_id: Uuid,
    pub event_type: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_type: String,
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub metadata: serde_json::Value,
    pub ip_address: Option<String>,
    pub is_reversed: bool,
    pub reversed_by_event_id: Option<i64>,
    pub reversal_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Input for creating a new audit event (no id/created_at).
#[derive(Debug, Clone)]
pub struct NewAuditEvent {
    pub organization_id: Uuid,
    pub event_type: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_type: String,
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub metadata: serde_json::Value,
    pub ip_address: Option<String>,
}

/// Result of a reversal operation.
#[derive(Debug, Serialize)]
pub struct ReversalResult {
    pub original_event_id: i64,
    pub reversal_event_id: i64,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub compensation_applied: bool,
}

/// Query filters for listing audit events.
#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub organization_id: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub event_type: Option<String>,
    pub actor_user_id: Option<Uuid>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Request body for initiating a reversal.
#[derive(Debug, Deserialize)]
pub struct ReversalRequest {
    pub event_id: i64,
    pub reason: String,
}

use crate::entity::models::PaginatedResponse;

pub type PaginatedAuditEvents = PaginatedResponse<AuditEvent>;
