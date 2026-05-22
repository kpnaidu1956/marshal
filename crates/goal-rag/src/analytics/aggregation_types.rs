//! Types for team and organization-level aggregations
//!
//! Provides data structures for:
//! - Team membership mapping
//! - Interaction type aggregations
//! - Sentiment trend aggregations
//! - Bottleneck aggregations
//! - Participation network metrics

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Period type for aggregations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeriodType {
    Daily,
    Weekly,
    Monthly,
}

impl PeriodType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "daily" => Self::Daily,
            "weekly" => Self::Weekly,
            "monthly" => Self::Monthly,
            _ => Self::Daily,
        }
    }
}

/// Trend direction indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrendDirection {
    Improving,
    Worsening,
    Stable,
}

impl TrendDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Improving => "improving",
            Self::Worsening => "worsening",
            Self::Stable => "stable",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "improving" => Self::Improving,
            "worsening" => Self::Worsening,
            "stable" => Self::Stable,
            _ => Self::Stable,
        }
    }
}

/// Team membership role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    Manager,
    Member,
}

impl TeamRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manager => "manager",
            Self::Member => "member",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "manager" => Self::Manager,
            _ => Self::Member,
        }
    }
}

/// Team membership record (team = manager + direct reports)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMembership {
    pub id: Uuid,
    pub organization_id: String,
    /// team_id is the manager's user_id
    pub team_id: String,
    /// team_name is the manager's name
    pub team_name: String,
    pub user_id: String,
    pub role: TeamRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Team summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSummary {
    pub team_id: String,
    pub team_name: String,
    pub member_count: u32,
    pub total_interactions: u32,
    pub avg_sentiment: f32,
    pub bottleneck_hours: f64,
    /// Composite health score (0-100)
    pub health_score: f32,
}

/// Interaction type aggregation for a period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionTypeAggregation {
    pub id: Uuid,
    pub organization_id: String,
    /// None for org-level aggregation
    pub team_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub period_type: PeriodType,
    /// Counts per interaction type
    pub type_counts: HashMap<String, u32>,
    pub total_interactions: u32,
    /// Ratio of clarification requests to total
    pub clarification_ratio: f32,
    /// Ratio of blockers to total
    pub blocker_ratio: f32,
    /// Ratio of escalations to total
    pub escalation_ratio: f32,
    pub computed_at: DateTime<Utc>,
}

/// Sentiment aggregation for a period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentAggregation {
    pub id: Uuid,
    pub organization_id: String,
    pub team_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub period_type: PeriodType,
    /// Average sentiment (-1.0 to 1.0)
    pub avg_sentiment: f32,
    pub min_sentiment: f32,
    pub max_sentiment: f32,
    /// Standard deviation
    pub sentiment_std_dev: f32,
    /// Count with sentiment > 0.3
    pub positive_count: u32,
    /// Count with sentiment between -0.3 and 0.3
    pub neutral_count: u32,
    /// Count with sentiment < -0.3
    pub negative_count: u32,
    /// Average sentiment by interaction type
    pub sentiment_by_type: HashMap<String, f32>,
    /// 7-day rolling average
    pub rolling_7day_avg: Option<f32>,
    /// 30-day rolling average
    pub rolling_30day_avg: Option<f32>,
    pub total_interactions: u32,
    pub computed_at: DateTime<Utc>,
}

/// Bottleneck aggregation for a period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckAggregation {
    pub id: Uuid,
    pub organization_id: String,
    pub team_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub period_type: PeriodType,
    /// Count per bottleneck type
    pub type_counts: HashMap<String, u32>,
    /// Total hours per bottleneck type
    pub type_total_hours: HashMap<String, f64>,
    /// Average hours per bottleneck type
    pub type_avg_hours: HashMap<String, f64>,
    pub total_bottlenecks: u32,
    pub total_hours_lost: f64,
    pub avg_bottleneck_duration: f64,
    /// Trend compared to previous period
    pub trend_direction: TrendDirection,
    pub trend_percent_change: f32,
    pub computed_at: DateTime<Utc>,
}

/// Edge in the participation network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipationEdge {
    pub id: Uuid,
    pub organization_id: String,
    /// None for cross-team edges
    pub team_id: Option<String>,
    pub from_user_id: String,
    pub to_user_id: String,
    pub interaction_count: u32,
    pub avg_sentiment: f32,
    /// Breakdown by interaction type
    pub type_breakdown: HashMap<String, u32>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// Normalized interaction weight
    pub weight: f32,
    pub computed_at: DateTime<Utc>,
}

/// User-level participation metrics (centrality, activity)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipationMetrics {
    pub id: Uuid,
    pub organization_id: String,
    pub team_id: Option<String>,
    pub user_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub period_type: PeriodType,
    /// Number of unique connections
    pub degree_centrality: f32,
    /// How often user is on shortest paths between others
    pub betweenness_centrality: f32,
    /// Average distance to all other users
    pub closeness_centrality: f32,
    pub total_interactions_sent: u32,
    pub total_interactions_received: u32,
    pub unique_collaborators: u32,
    /// True if user bridges multiple teams
    pub is_connector: bool,
    /// True if many users depend on this user
    pub is_bottleneck: bool,
    pub computed_at: DateTime<Utc>,
}

/// Intervention outcome for learning from recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionOutcome {
    pub id: Uuid,
    pub organization_id: String,
    pub recommendation_id: Uuid,
    /// Type of intervention taken
    pub intervention_type: String,
    pub intervention_date: DateTime<Utc>,
    /// When outcome was measured (after some time passes)
    pub outcome_measured_date: Option<DateTime<Utc>>,
    /// Type of outcome measured (bottleneck_reduction, sentiment_improvement, etc.)
    pub outcome_type: Option<String>,
    /// Measured change (negative for reduction, positive for increase)
    pub outcome_value: Option<f64>,
    /// Snapshot of metrics before intervention
    pub pre_intervention_metrics: Option<serde_json::Value>,
    /// Snapshot of metrics after intervention
    pub post_intervention_metrics: Option<serde_json::Value>,
    /// Confidence in the measured outcome
    pub confidence_score: f32,
    /// If a new pattern was learned from this outcome
    pub learned_pattern_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Aggregation job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationJobStatus {
    Pending,
    Running,
    Complete,
    Failed,
}

impl AggregationJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "complete" => Self::Complete,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// Aggregation job type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationJobType {
    InteractionTypes,
    Sentiment,
    Bottlenecks,
    Network,
    All,
}

impl AggregationJobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InteractionTypes => "interaction_types",
            Self::Sentiment => "sentiment",
            Self::Bottlenecks => "bottlenecks",
            Self::Network => "network",
            Self::All => "all",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "interaction_types" => Self::InteractionTypes,
            "sentiment" => Self::Sentiment,
            "bottlenecks" => Self::Bottlenecks,
            "network" => Self::Network,
            "all" => Self::All,
            _ => Self::All,
        }
    }
}

/// Aggregation scope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationScope {
    Organization,
    Team,
    AllTeams,
}

impl AggregationScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Organization => "organization",
            Self::Team => "team",
            Self::AllTeams => "all_teams",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "organization" => Self::Organization,
            "team" => Self::Team,
            "all_teams" => Self::AllTeams,
            _ => Self::Organization,
        }
    }
}

/// Aggregation job record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationJob {
    pub id: Uuid,
    pub organization_id: String,
    pub job_type: AggregationJobType,
    pub scope: AggregationScope,
    pub team_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub period_type: PeriodType,
    pub status: AggregationJobStatus,
    pub progress_percent: u8,
    pub records_processed: u32,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Organization dashboard summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationDashboard {
    pub organization_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub team_summaries: Vec<TeamSummary>,
    pub org_interaction_types: Option<InteractionTypeAggregation>,
    pub org_sentiment: Option<SentimentAggregation>,
    pub org_bottlenecks: Option<BottleneckAggregation>,
    pub top_connectors: Vec<ParticipationMetrics>,
    pub cross_team_edges: Vec<ParticipationEdge>,
    pub generated_at: DateTime<Utc>,
}

/// Team dashboard summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDashboard {
    pub organization_id: String,
    pub team_id: String,
    pub team_name: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub member_count: u32,
    pub interaction_types: Option<InteractionTypeAggregation>,
    pub sentiment: Option<SentimentAggregation>,
    pub bottlenecks: Option<BottleneckAggregation>,
    pub member_metrics: Vec<ParticipationMetrics>,
    pub internal_edges: Vec<ParticipationEdge>,
    pub external_edges: Vec<ParticipationEdge>,
    pub generated_at: DateTime<Utc>,
}

/// Request to sync teams from external source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSyncRequest {
    pub organization_id: String,
    /// List of team definitions to sync
    pub teams: Vec<TeamSyncDefinition>,
}

/// Single team definition for syncing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSyncDefinition {
    /// Manager's user_id (becomes team_id)
    pub manager_id: String,
    /// Manager's name (becomes team_name)
    pub manager_name: String,
    /// Direct reports' user_ids
    pub member_ids: Vec<String>,
}

/// Request to trigger aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerAggregationRequest {
    pub organization_id: String,
    pub job_type: AggregationJobType,
    pub scope: AggregationScope,
    pub team_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub period_type: PeriodType,
}

/// Request to record an intervention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordInterventionRequest {
    pub organization_id: String,
    pub recommendation_id: Uuid,
    pub intervention_type: String,
}

/// Request to record intervention outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordOutcomeRequest {
    pub outcome_type: String,
    pub outcome_value: f64,
    pub post_intervention_metrics: Option<serde_json::Value>,
}
