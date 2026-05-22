//! Database schema types for the goalrag database
//!
//! These types mirror the PostgreSQL tables in the api schema.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task record from api.tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub organization_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub creator_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub due_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Goal record from api.goals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: Uuid,
    pub organization_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub owner_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub target_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// User record from api.users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub organization_id: Option<Uuid>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Organization record from api.organizations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Task comment from api.task_comments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComment {
    pub id: Uuid,
    pub task_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub content: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Chat message from api.chat_messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub organization_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub content: String,
    pub role: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Message from api.messages (general messages)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub organization_id: Option<Uuid>,
    pub sender_id: Option<Uuid>,
    pub recipient_id: Option<Uuid>,
    pub content: String,
    pub message_type: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Conversation from api.conversations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub organization_id: Option<Uuid>,
    pub title: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Interaction data extracted from a database change for learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionData {
    /// Type of entity (task, goal, message, etc.)
    pub entity_type: String,
    /// Entity ID
    pub entity_id: Uuid,
    /// Organization ID for multi-tenancy
    pub organization_id: Option<Uuid>,
    /// User who performed the action
    pub actor_id: Option<Uuid>,
    /// Type of change (insert, update, delete)
    pub change_type: String,
    /// Content or description of the change
    pub content: Option<String>,
    /// Additional metadata
    pub metadata: serde_json::Value,
    /// Timestamp of the change
    pub timestamp: DateTime<Utc>,
}

impl InteractionData {
    /// Create from a task change
    pub fn from_task(task: &Task, change_type: &str) -> Self {
        Self {
            entity_type: "task".to_string(),
            entity_id: task.id,
            organization_id: task.organization_id,
            actor_id: task.assignee_id.or(task.creator_id),
            change_type: change_type.to_string(),
            content: Some(format!("{}: {}", task.title, task.description.as_deref().unwrap_or(""))),
            metadata: serde_json::json!({
                "status": task.status,
                "priority": task.priority,
                "goal_id": task.goal_id,
            }),
            timestamp: Utc::now(),
        }
    }

    /// Create from a goal change
    pub fn from_goal(goal: &Goal, change_type: &str) -> Self {
        Self {
            entity_type: "goal".to_string(),
            entity_id: goal.id,
            organization_id: goal.organization_id,
            actor_id: goal.owner_id,
            change_type: change_type.to_string(),
            content: Some(format!("{}: {}", goal.title, goal.description.as_deref().unwrap_or(""))),
            metadata: serde_json::json!({
                "status": goal.status,
                "parent_id": goal.parent_id,
            }),
            timestamp: Utc::now(),
        }
    }

    /// Create from a message/comment
    pub fn from_message(
        entity_type: &str,
        id: Uuid,
        organization_id: Option<Uuid>,
        sender_id: Option<Uuid>,
        content: &str,
        change_type: &str,
    ) -> Self {
        Self {
            entity_type: entity_type.to_string(),
            entity_id: id,
            organization_id,
            actor_id: sender_id,
            change_type: change_type.to_string(),
            content: Some(content.to_string()),
            metadata: serde_json::json!({}),
            timestamp: Utc::now(),
        }
    }
}
