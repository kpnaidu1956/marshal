use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::models::PaginatedResponse;

// --- Enums ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    Manual,
    Automated,
    Approval,
    Integration,
    LlmAction,
    SubWorkflow,
}

impl StepType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepType::Manual => "manual",
            StepType::Automated => "automated",
            StepType::Approval => "approval",
            StepType::Integration => "integration",
            StepType::LlmAction => "llm_action",
            StepType::SubWorkflow => "sub_workflow",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, crate::error::BpeError> {
        match s {
            "manual" => Ok(StepType::Manual),
            "automated" => Ok(StepType::Automated),
            "approval" => Ok(StepType::Approval),
            "integration" => Ok(StepType::Integration),
            "llm_action" => Ok(StepType::LlmAction),
            "sub_workflow" => Ok(StepType::SubWorkflow),
            other => Err(crate::error::BpeError::BadRequest(format!("Unknown step type: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Draft,
    Confirmed,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Draft => "draft",
            ExecutionStatus::Confirmed => "confirmed",
            ExecutionStatus::Running => "running",
            ExecutionStatus::Paused => "paused",
            ExecutionStatus::Completed => "completed",
            ExecutionStatus::Failed => "failed",
            ExecutionStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, crate::error::BpeError> {
        match s {
            "draft" => Ok(ExecutionStatus::Draft),
            "confirmed" => Ok(ExecutionStatus::Confirmed),
            "running" => Ok(ExecutionStatus::Running),
            "paused" => Ok(ExecutionStatus::Paused),
            "completed" => Ok(ExecutionStatus::Completed),
            "failed" => Ok(ExecutionStatus::Failed),
            "cancelled" => Ok(ExecutionStatus::Cancelled),
            other => Err(crate::error::BpeError::BadRequest(format!("Unknown execution status: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Ready,
    InProgress,
    WaitingApproval,
    WaitingIntegration,
    Completed,
    Failed,
    Skipped,
}

impl StepStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepStatus::Pending => "pending",
            StepStatus::Ready => "ready",
            StepStatus::InProgress => "in_progress",
            StepStatus::WaitingApproval => "waiting_approval",
            StepStatus::WaitingIntegration => "waiting_integration",
            StepStatus::Completed => "completed",
            StepStatus::Failed => "failed",
            StepStatus::Skipped => "skipped",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, crate::error::BpeError> {
        match s {
            "pending" => Ok(StepStatus::Pending),
            "ready" => Ok(StepStatus::Ready),
            "in_progress" => Ok(StepStatus::InProgress),
            "waiting_approval" => Ok(StepStatus::WaitingApproval),
            "waiting_integration" => Ok(StepStatus::WaitingIntegration),
            "completed" => Ok(StepStatus::Completed),
            "failed" => Ok(StepStatus::Failed),
            "skipped" => Ok(StepStatus::Skipped),
            other => Err(crate::error::BpeError::BadRequest(format!("Unknown step status: {other}"))),
        }
    }
}

// --- Domain models ---

/// A step template within a workflow definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTemplate {
    pub name: String,
    pub description: Option<String>,
    pub step_type: String,
    #[serde(default)]
    pub dependencies: Vec<i32>,
    pub estimated_duration_minutes: Option<i32>,
    pub integration_type: Option<String>,
    pub integration_config: Option<serde_json::Value>,
    pub assigned_role: Option<String>,
}

/// A workflow definition (reusable template).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub step_templates: Vec<StepTemplate>,
    pub is_learned: bool,
    pub source: String,
    pub version: i32,
    pub is_active: bool,
    pub times_used: i32,
    pub avg_completion_minutes: Option<f64>,
    pub success_rate: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Option<Uuid>,
}

/// A workflow execution instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecution {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub definition_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub original_prompt: Option<String>,
    pub target_entity_id: Option<Uuid>,
    pub linked_task_id: Option<Uuid>,
    pub linked_goal_id: Option<Uuid>,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub initiated_by: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single step within a workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub execution_id: Uuid,
    pub step_order: i32,
    pub name: String,
    pub description: Option<String>,
    pub step_type: String,
    pub status: String,
    pub dependencies: Vec<i32>,
    pub estimated_duration_minutes: Option<i32>,
    pub actual_duration_minutes: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub integration_type: Option<String>,
    pub integration_config: Option<serde_json::Value>,
    pub integration_result: Option<serde_json::Value>,
    pub approval_rule_id: Option<Uuid>,
    pub approval_request_id: Option<Uuid>,
    pub assigned_to: Option<Uuid>,
    pub input_data: Option<serde_json::Value>,
    pub output_data: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// --- Request/Response DTOs ---

#[derive(Debug, Deserialize)]
pub struct CreateDefinitionRequest {
    pub organization_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub step_templates: Vec<StepTemplate>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDefinitionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub step_templates: Option<Vec<StepTemplate>>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteDefinitionRequest {
    pub organization_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub target_entity_id: Option<Uuid>,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmStep {
    pub step_order: i32,
    pub name: Option<String>,
    pub assigned_to: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    pub steps: Option<Vec<ConfirmStep>>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteStepRequest {
    pub output_data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SkipStepRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignStepRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct ListExecutionsQuery {
    pub organization_id: String,
    pub status: Option<String>,
    pub definition_id: Option<Uuid>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    pub organization_id: String,
    pub category: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

pub type PaginatedDefinitions = PaginatedResponse<WorkflowDefinition>;

pub type PaginatedExecutions = PaginatedResponse<WorkflowExecution>;
