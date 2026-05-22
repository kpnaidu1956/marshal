use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Domain models ---

/// A learned step sequence extracted from a completed workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedSequence {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub task_category: String,
    pub entity_type_names: Vec<String>,
    pub steps: serde_json::Value,
    pub source_execution_id: Option<Uuid>,
    pub times_suggested: i32,
    pub times_accepted: i32,
    pub times_modified: i32,
    pub times_rejected: i32,
    pub avg_completion_minutes: Option<f64>,
    pub embedding_text: Option<String>,
    pub version: i32,
    pub superseded_by: Option<Uuid>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Compact suggestion returned to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceSuggestion {
    pub id: Uuid,
    pub task_category: String,
    pub steps: serde_json::Value,
    pub acceptance_rate: f64,
    pub avg_completion_minutes: Option<f64>,
    pub version: i32,
    pub score: f64,
}

// --- Request DTOs ---

#[derive(Debug, Deserialize)]
pub struct LearnFromExecutionRequest {
    pub organization_id: String,
    pub execution_id: Uuid,
    pub task_category: String,
    pub entity_type_names: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SuggestRequest {
    pub organization_id: String,
    pub task_category: Option<String>,
    pub prompt: Option<String>,
    pub entity_type_name: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct FeedbackRequest {
    pub outcome: String, // "accepted", "modified", "rejected"
}

#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    pub organization_id: String,
    pub task_category: Option<String>,
    pub active_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct LearnFromGoalRequest {
    pub organization_id: String,
    pub goal_id: String,
    pub goal_title: String,
    pub task_category: String,
    pub tasks: Vec<GoalTaskEntry>,
}

#[derive(Debug, Deserialize)]
pub struct GoalTaskEntry {
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    #[serde(default)]
    pub sequence_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct PromoteRequest {
    pub organization_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
}
