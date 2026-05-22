//! Core types for the analytics module

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Primary interaction classification types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    /// Request for more information or clarification
    RequestClarification,
    /// Request for additional resources (people, budget, tools)
    RequestResources,
    /// Giving directions or instructions
    Direction,
    /// Suggesting a different approach or improvement
    Suggestion,
    /// Requesting approval or sign-off
    RequestApproval,
    /// Providing status update on progress
    StatusUpdate,
    /// Acknowledging receipt or understanding
    Acknowledgment,
    /// Escalating an issue to higher authority
    Escalation,
    /// Reporting a blocker or impediment
    Blocker,
    /// Asking a question
    Question,
    /// Providing an answer to a question
    Answer,
    /// Assigning work to someone
    Assignment,
    /// Feedback on work done
    Feedback,
    /// Celebration or recognition
    Recognition,
    /// Other/uncategorized
    Other,
}

impl InteractionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RequestClarification => "request_clarification",
            Self::RequestResources => "request_resources",
            Self::Direction => "direction",
            Self::Suggestion => "suggestion",
            Self::RequestApproval => "request_approval",
            Self::StatusUpdate => "status_update",
            Self::Acknowledgment => "acknowledgment",
            Self::Escalation => "escalation",
            Self::Blocker => "blocker",
            Self::Question => "question",
            Self::Answer => "answer",
            Self::Assignment => "assignment",
            Self::Feedback => "feedback",
            Self::Recognition => "recognition",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "request_clarification" => Self::RequestClarification,
            "request_resources" => Self::RequestResources,
            "direction" => Self::Direction,
            "suggestion" => Self::Suggestion,
            "request_approval" => Self::RequestApproval,
            "status_update" => Self::StatusUpdate,
            "acknowledgment" => Self::Acknowledgment,
            "escalation" => Self::Escalation,
            "blocker" => Self::Blocker,
            "question" => Self::Question,
            "answer" => Self::Answer,
            "assignment" => Self::Assignment,
            "feedback" => Self::Feedback,
            "recognition" => Self::Recognition,
            _ => Self::Other,
        }
    }
}

/// Urgency level of an interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UrgencyLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl UrgencyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "critical" => Self::Critical,
            _ => Self::Medium,
        }
    }
}

/// Source of an interaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionSource {
    TaskComment,
    GoalComment,
    Message,
    ActivityLog,
}

impl InteractionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TaskComment => "task_comment",
            Self::GoalComment => "goal_comment",
            Self::Message => "message",
            Self::ActivityLog => "activity_log",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "task_comment" => Self::TaskComment,
            "goal_comment" => Self::GoalComment,
            "message" => Self::Message,
            "activity_log" => Self::ActivityLog,
            _ => Self::Message,
        }
    }
}

/// Entities extracted from an interaction
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedEntities {
    /// User IDs or names mentioned
    #[serde(default)]
    pub mentioned_users: Vec<String>,
    /// Deadline mentions (dates, times)
    #[serde(default)]
    pub mentioned_deadlines: Vec<String>,
    /// Action items identified
    #[serde(default)]
    pub action_items: Vec<String>,
    /// Blockers identified
    #[serde(default)]
    pub blockers: Vec<String>,
    /// Resources requested or mentioned
    #[serde(default)]
    pub resources: Vec<String>,
}

/// Result from classifying an interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// Primary interaction type
    pub primary_type: InteractionType,
    /// Secondary types (interaction may have multiple purposes)
    #[serde(default)]
    pub secondary_types: Vec<InteractionType>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Sentiment score (-1.0 negative to 1.0 positive)
    pub sentiment: f32,
    /// Urgency level
    pub urgency: UrgencyLevel,
    /// Extracted entities
    pub entities: ExtractedEntities,
    /// LLM reasoning (optional, for debugging)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

impl ClassificationResult {
    /// Create a fallback result when classification fails
    pub fn fallback(reason: &str) -> Self {
        Self {
            primary_type: InteractionType::Other,
            secondary_types: vec![],
            confidence: 0.0,
            sentiment: 0.0,
            urgency: UrgencyLevel::Medium,
            entities: ExtractedEntities::default(),
            reasoning: Some(format!("Classification failed: {}", reason)),
        }
    }
}

/// Context for classification (provides additional info to the classifier)
#[derive(Debug, Clone, Default)]
pub struct ClassificationContext {
    /// Task title if applicable
    pub task_title: Option<String>,
    /// Goal title if applicable
    pub goal_title: Option<String>,
    /// Sender name
    pub sender_name: Option<String>,
    /// Previous interactions in thread (for context)
    pub thread_history: Vec<String>,
}

/// Stored classification record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionClassification {
    pub id: Uuid,
    pub organization_id: String,
    pub source_type: InteractionSource,
    pub source_id: String,
    pub task_id: Option<String>,
    pub goal_id: Option<String>,
    pub sender_id: String,
    pub content: String,

    // Classification results
    pub interaction_type: InteractionType,
    pub secondary_types: Vec<InteractionType>,
    pub confidence_score: f32,
    pub entities: ExtractedEntities,
    pub sentiment: f32,
    pub urgency_level: UrgencyLevel,

    // Linking
    pub references_interaction_id: Option<String>,
    pub original_created_at: DateTime<Utc>,
    pub classified_at: DateTime<Utc>,
}

/// Timeline event in a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub description: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub interaction_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Workflow phase (e.g., "planning", "execution", "review")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub name: String,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
    pub interaction_count: u32,
    pub participants: Vec<String>,
}

/// Detected bottleneck in a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowBottleneck {
    pub bottleneck_type: String,
    pub duration_hours: f64,
    pub description: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub caused_by: Option<String>,
}

/// Complete workflow timeline for a task or goal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTimeline {
    pub id: Uuid,
    pub organization_id: String,
    pub entity_type: String, // "task" or "goal"
    pub entity_id: String,

    // Metrics
    pub total_interactions: u32,
    pub total_participants: u32,
    pub total_duration_hours: Option<f64>,

    // Timeline data
    pub phases: Vec<WorkflowPhase>,
    pub key_events: Vec<TimelineEvent>,
    pub bottlenecks: Vec<WorkflowBottleneck>,

    pub status: String,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub last_analyzed_at: DateTime<Utc>,
}

/// Pattern type for learned patterns
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    Success,
    Failure,
    Bottleneck,
    Efficiency,
}

impl PatternType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Bottleneck => "bottleneck",
            Self::Efficiency => "efficiency",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "success" => Self::Success,
            "failure" => Self::Failure,
            "bottleneck" => Self::Bottleneck,
            "efficiency" => Self::Efficiency,
            _ => Self::Efficiency,
        }
    }
}

/// Learned workflow pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPattern {
    pub id: Uuid,
    pub organization_id: String,
    pub pattern_type: PatternType,
    pub pattern_name: String,
    pub description: String,
    pub criteria: serde_json::Value, // Matching rules

    pub occurrence_count: u32,
    pub success_correlation: Option<f32>,
    pub avg_time_impact_hours: Option<f64>,
    pub confidence_score: f32,

    pub examples: Vec<String>, // Task/goal IDs
    pub is_active: bool,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Recommendation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationType {
    Process,
    Communication,
    Resource,
    Timing,
    Assignment,
}

impl RecommendationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Communication => "communication",
            Self::Resource => "resource",
            Self::Timing => "timing",
            Self::Assignment => "assignment",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "process" => Self::Process,
            "communication" => Self::Communication,
            "resource" => Self::Resource,
            "timing" => Self::Timing,
            "assignment" => Self::Assignment,
            _ => Self::Process,
        }
    }
}

/// Target type for recommendations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationTarget {
    Task,
    Goal,
    Team,
    Organization,
}

impl RecommendationTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Goal => "goal",
            Self::Team => "team",
            Self::Organization => "organization",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "task" => Self::Task,
            "goal" => Self::Goal,
            "team" => Self::Team,
            "organization" => Self::Organization,
            _ => Self::Task,
        }
    }
}

/// Recommendation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationStatus {
    Pending,
    Accepted,
    Rejected,
    Implemented,
}

impl RecommendationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Implemented => "implemented",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "accepted" => Self::Accepted,
            "rejected" => Self::Rejected,
            "implemented" => Self::Implemented,
            _ => Self::Pending,
        }
    }
}

/// Efficiency recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EfficiencyRecommendation {
    pub id: Uuid,
    pub organization_id: String,
    pub target_type: RecommendationTarget,
    pub target_id: Option<String>,

    pub recommendation_type: RecommendationType,
    pub title: String,
    pub description: String,
    pub suggested_actions: Vec<String>,

    pub based_on_patterns: Vec<String>, // Pattern IDs
    pub evidence: serde_json::Value,    // Supporting data

    pub priority: UrgencyLevel,
    pub estimated_time_savings_hours: Option<f64>,

    pub status: RecommendationStatus,
    pub user_feedback: Option<String>,
    pub generated_at: DateTime<Utc>,
}

/// Analysis job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisJobStatus {
    Pending,
    FetchingData,
    Classifying,
    BuildingTimeline,
    MatchingPatterns,
    GeneratingRecommendations,
    Complete,
    Failed,
}

impl AnalysisJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::FetchingData => "fetching_data",
            Self::Classifying => "classifying",
            Self::BuildingTimeline => "building_timeline",
            Self::MatchingPatterns => "matching_patterns",
            Self::GeneratingRecommendations => "generating_recommendations",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "fetching_data" => Self::FetchingData,
            "classifying" => Self::Classifying,
            "building_timeline" => Self::BuildingTimeline,
            "matching_patterns" => Self::MatchingPatterns,
            "generating_recommendations" => Self::GeneratingRecommendations,
            "complete" => Self::Complete,
            "failed" => Self::Failed,
            _ => Self::Failed,
        }
    }
}

/// Analysis job record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob {
    pub id: Uuid,
    pub organization_id: String,
    pub entity_type: String, // "task" or "goal"
    pub entity_id: String,

    pub status: AnalysisJobStatus,
    pub progress_percent: u8,
    pub current_stage: String,

    pub interactions_found: u32,
    pub interactions_classified: u32,
    pub patterns_matched: u32,
    pub recommendations_generated: u32,

    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl AnalysisJob {
    pub fn new(organization_id: String, entity_type: String, entity_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id,
            entity_type,
            entity_id,
            status: AnalysisJobStatus::Pending,
            progress_percent: 0,
            current_stage: "pending".to_string(),
            interactions_found: 0,
            interactions_classified: 0,
            patterns_matched: 0,
            recommendations_generated: 0,
            error: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }
}
