use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::models::PaginatedResponse;

// --- Domain models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub conditions: serde_json::Value,
    pub approval_type: String,
    pub approver_user_ids: Vec<Uuid>,
    pub required_approvals: i32,
    pub timeout_minutes: i32,
    pub escalation_user_id: Option<Uuid>,
    pub auto_approve_on_timeout: bool,
    pub allow_delegation: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub rule_id: Uuid,
    pub workflow_execution_id: Option<Uuid>,
    pub workflow_step_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub context_data: serde_json::Value,
    pub status: String,
    pub requested_by: Uuid,
    pub current_approver_index: i32,
    pub deadline_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub request_id: Uuid,
    pub decided_by: Uuid,
    pub delegated_from: Option<Uuid>,
    pub decision: String,
    pub notes: Option<String>,
    pub decided_at: DateTime<Utc>,
}

// --- Request DTOs ---

#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub organization_id: String,
    pub name: String,
    pub description: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub approval_type: Option<String>,
    pub approver_user_ids: Vec<Uuid>,
    pub required_approvals: Option<i32>,
    pub timeout_minutes: Option<i32>,
    pub escalation_user_id: Option<Uuid>,
    pub auto_approve_on_timeout: Option<bool>,
    pub allow_delegation: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub approval_type: Option<String>,
    pub approver_user_ids: Option<Vec<Uuid>>,
    pub required_approvals: Option<i32>,
    pub timeout_minutes: Option<i32>,
    pub escalation_user_id: Option<Uuid>,
    pub auto_approve_on_timeout: Option<bool>,
    pub allow_delegation: Option<bool>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRequestPayload {
    pub organization_id: String,
    pub rule_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub context_data: Option<serde_json::Value>,
    pub workflow_execution_id: Option<Uuid>,
    pub workflow_step_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionPayload {
    pub decision: String,
    pub notes: Option<String>,
    pub delegated_from: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListRequestsQuery {
    pub organization_id: String,
    pub status: Option<String>,
    pub rule_id: Option<Uuid>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    pub organization_id: String,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

pub type PaginatedRules = PaginatedResponse<ApprovalRule>;

#[derive(Debug, Deserialize)]
pub struct PendingQuery {
    pub organization_id: String,
}

pub type PaginatedRequests = PaginatedResponse<ApprovalRequest>;
