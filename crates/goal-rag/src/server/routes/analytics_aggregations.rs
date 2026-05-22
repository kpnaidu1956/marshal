//! Analytics Aggregation API routes (Phase 6)
//!
//! Provides endpoints for team and organization-level aggregations,
//! participation network analysis, and learning system.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::analytics::{
    aggregation_types::*,
    aggregator::{Aggregator, get_daily_period_start, get_weekly_period_start, get_monthly_period_start, get_period_end},
    network::NetworkAnalyzer,
    learning::LearningSystem,
    storage::AnalyticsDb,
    TeamManager,
};
use crate::server::state::AppState;

// ==================== Request/Response Types ====================

#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    pub org: String,
}

#[derive(Debug, Deserialize)]
pub struct TeamSyncBody {
    pub organization_id: String,
    pub teams: Vec<TeamSyncDef>,
}

#[derive(Debug, Deserialize)]
pub struct TeamSyncDef {
    pub manager_id: String,
    pub manager_name: String,
    pub member_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TeamSyncResponse {
    pub teams_synced: u32,
    pub members_added: u32,
}

#[derive(Debug, Deserialize)]
pub struct AggregationQuery {
    pub org: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default = "default_period")]
    pub period: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_period() -> String {
    "daily".to_string()
}

fn default_limit() -> usize {
    30
}

#[derive(Debug, Deserialize)]
pub struct TriggerAggregationBody {
    pub organization_id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default = "default_period")]
    pub period_type: String,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct TriggerAggregationResponse {
    pub interactions_processed: u32,
    pub timelines_processed: u32,
    pub aggregations_created: u32,
}

#[derive(Debug, Deserialize)]
pub struct NetworkQuery {
    pub org: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default = "default_days")]
    pub days: i64,
}

fn default_days() -> i64 {
    30
}

/// Validate and clamp days parameter to safe range (1-365)
fn validate_days(days: i64) -> i64 {
    days.clamp(1, 365)
}

#[derive(Debug, Deserialize)]
pub struct RecordInterventionBody {
    pub organization_id: String,
    pub recommendation_id: String,
    pub intervention_type: String,
    pub pre_metrics: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct RecordOutcomeBody {
    pub organization_id: String,
    pub outcome_type: String,
    pub outcome_value: f64,
    pub post_metrics: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ==================== Helper Functions ====================

/// Get the analytics database from app state (pre-initialized at startup)
///
/// Falls back to opening a new connection if state doesn't have one,
/// but this should rarely happen since analytics_db is initialized in AppState::new.
fn get_analytics_db(state: &AppState) -> Result<Arc<AnalyticsDb>, (StatusCode, Json<ErrorResponse>)> {
    // Prefer the pre-initialized instance from AppState (avoids re-opening on every request)
    if let Some(db) = state.analytics_db() {
        return Ok(Arc::clone(db));
    }

    // Fallback: open a new connection (should rarely happen)
    let data_dir = state.config().vector_db.storage_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let analytics_db_path = data_dir.join("analytics.db");

    match AnalyticsDb::new(&analytics_db_path) {
        Ok(db) => Ok(Arc::new(db)),
        Err(_e) => {
            tracing::error!("Failed to open analytics database");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Analytics database not available".to_string() }),
            ))
        }
    }
}

fn validate_org_id(org_id: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if org_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "organization_id is required".to_string() }),
        ));
    }
    if org_id.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "organization_id exceeds maximum length".to_string() }),
        ));
    }
    Ok(())
}

fn parse_period_type(s: &str) -> PeriodType {
    PeriodType::parse(s)
}

fn sanitize_limit(limit: usize) -> usize {
    limit.clamp(1, 500)
}

// ==================== Team Management ====================

/// List teams in an organization
pub async fn list_teams(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    validate_org_id(&query.org)?;

    let analytics_db = get_analytics_db(&state)?;

    let manager = TeamManager::new(&analytics_db);
    match manager.list_teams(&query.org) {
        Ok(teams) => Ok(Json(teams)),
        Err(e) => {
            tracing::error!("Failed to list teams: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to list teams".to_string() }),
            ))
        }
    }
}

/// Sync teams from external source
pub async fn sync_teams(
    State(state): State<AppState>,
    Json(body): Json<TeamSyncBody>,
) -> impl IntoResponse {
    validate_org_id(&body.organization_id)?;

    let analytics_db = get_analytics_db(&state)?;

    let request = TeamSyncRequest {
        organization_id: body.organization_id,
        teams: body.teams.into_iter().map(|t| TeamSyncDefinition {
            manager_id: t.manager_id,
            manager_name: t.manager_name,
            member_ids: t.member_ids,
        }).collect(),
    };

    let manager = TeamManager::new(&analytics_db);
    match manager.sync_teams(&request) {
        Ok(result) => Ok(Json(TeamSyncResponse {
            teams_synced: result.teams_synced,
            members_added: result.members_added,
        })),
        Err(e) => {
            tracing::error!("Failed to sync teams: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to sync teams".to_string() }),
            ))
        }
    }
}

/// Get team members
pub async fn get_team_members(
    State(state): State<AppState>,
    Path(team_id): Path<String>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    validate_org_id(&query.org)?;

    let analytics_db = get_analytics_db(&state)?;

    let manager = TeamManager::new(&analytics_db);
    match manager.get_team_members(&query.org, &team_id) {
        Ok(members) => Ok(Json(members)),
        Err(e) => {
            tracing::error!("Failed to get team members: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to get team members".to_string() }),
            ))
        }
    }
}

// ==================== Interaction Type Aggregations ====================

/// Get interaction type aggregations
pub async fn get_interaction_aggregations(
    State(state): State<AppState>,
    Query(query): Query<AggregationQuery>,
) -> impl IntoResponse {
    validate_org_id(&query.org)?;

    let analytics_db = get_analytics_db(&state)?;

    let period_type = parse_period_type(&query.period);
    match analytics_db.get_interaction_type_aggregations(
        &query.org,
        query.team_id.as_deref(),
        period_type.as_str(),
        sanitize_limit(query.limit),
    ) {
        Ok(aggs) => Ok(Json(aggs)),
        Err(e) => {
            tracing::error!("Failed to get interaction aggregations: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to get aggregations".to_string() }),
            ))
        }
    }
}

/// Get interaction type aggregations for a specific team
pub async fn get_team_interaction_aggregations(
    State(state): State<AppState>,
    Path(team_id): Path<String>,
    Query(query): Query<AggregationQuery>,
) -> impl IntoResponse {
    validate_org_id(&query.org)?;

    let analytics_db = get_analytics_db(&state)?;

    let period_type = parse_period_type(&query.period);
    match analytics_db.get_interaction_type_aggregations(
        &query.org,
        Some(&team_id),
        period_type.as_str(),
        sanitize_limit(query.limit),
    ) {
        Ok(aggs) => Ok(Json(aggs)),
        Err(e) => {
            tracing::error!("Failed to get team interaction aggregations: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to get team aggregations".to_string() }),
            ))
        }
    }
}

// ==================== Trigger Aggregations ====================

/// Trigger aggregation computation
pub async fn trigger_aggregation(
    State(state): State<AppState>,
    Json(body): Json<TriggerAggregationBody>,
) -> impl IntoResponse {
    validate_org_id(&body.organization_id)?;

    let analytics_db = get_analytics_db(&state)?;

    let period_type = parse_period_type(&body.period_type);
    let now = Utc::now();

    let period_start = body.period_start.unwrap_or_else(|| {
        match period_type {
            PeriodType::Daily => get_daily_period_start(now),
            PeriodType::Weekly => get_weekly_period_start(now),
            PeriodType::Monthly => get_monthly_period_start(now),
        }
    });

    let period_end = body.period_end.unwrap_or_else(|| get_period_end(period_start, period_type));

    let aggregator = Aggregator::new(&analytics_db);
    match aggregator.run_aggregations(
        &body.organization_id,
        body.team_id.as_deref(),
        period_start,
        period_end,
        period_type,
    ) {
        Ok(result) => Ok(Json(TriggerAggregationResponse {
            interactions_processed: result.interactions_processed,
            timelines_processed: result.timelines_processed,
            aggregations_created: result.aggregations_created,
        })),
        Err(e) => {
            tracing::error!("Failed to run aggregations: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to run aggregations".to_string() }),
            ))
        }
    }
}

// ==================== Participation Network ====================

/// Get participation network graph
pub async fn get_participation_network(
    State(state): State<AppState>,
    Query(query): Query<NetworkQuery>,
) -> impl IntoResponse {
    validate_org_id(&query.org)?;

    let analytics_db = get_analytics_db(&state)?;

    // Validate days parameter to prevent DateTime overflow
    let days = validate_days(query.days);
    let end = Utc::now();
    let start = end - Duration::days(days);

    let classifications = match analytics_db.get_classifications_in_range(&query.org, &start, &end) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get classifications: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to get classifications".to_string() }),
            ));
        }
    };

    let analyzer = NetworkAnalyzer::new(&analytics_db);
    match analyzer.build_network(&query.org, &classifications, start, end) {
        Ok(network) => Ok(Json(serde_json::json!({
            "organization_id": network.organization_id,
            "nodes": network.nodes,
            "edges": network.edges,
            "period_start": network.period_start,
            "period_end": network.period_end,
        }))),
        Err(e) => {
            tracing::error!("Failed to build network: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to build network".to_string() }),
            ))
        }
    }
}

/// Get top connectors (cross-team bridges)
pub async fn get_connectors(
    State(state): State<AppState>,
    Query(query): Query<NetworkQuery>,
) -> impl IntoResponse {
    validate_org_id(&query.org)?;

    let analytics_db = get_analytics_db(&state)?;

    // Validate days parameter to prevent DateTime overflow
    let days = validate_days(query.days);
    let end = Utc::now();
    let start = end - Duration::days(days);

    let classifications = match analytics_db.get_classifications_in_range(&query.org, &start, &end) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get classifications: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to get classifications".to_string() }),
            ));
        }
    };

    let analyzer = NetworkAnalyzer::new(&analytics_db);
    let network = match analyzer.build_network(&query.org, &classifications, start, end) {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("Failed to build network: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to build network".to_string() }),
            ));
        }
    };

    match analyzer.get_top_connectors(&query.org, &network, 10) {
        Ok(connectors) => Ok(Json(connectors)),
        Err(e) => {
            tracing::error!("Failed to get connectors: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to get connectors".to_string() }),
            ))
        }
    }
}

// ==================== Learning System ====================

/// Record intervention
pub async fn record_intervention(
    State(state): State<AppState>,
    Json(body): Json<RecordInterventionBody>,
) -> impl IntoResponse {
    validate_org_id(&body.organization_id)?;

    let analytics_db = get_analytics_db(&state)?;

    let recommendation_id = match Uuid::parse_str(&body.recommendation_id) {
        Ok(id) => id,
        Err(_) => return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "Invalid recommendation_id".to_string() }),
        )),
    };

    let learning = LearningSystem::new(&analytics_db);
    match learning.record_intervention(
        &body.organization_id,
        recommendation_id,
        &body.intervention_type,
        body.pre_metrics,
    ) {
        Ok(outcome) => Ok(Json(serde_json::json!({
            "intervention_id": outcome.id,
            "success": true,
        }))),
        Err(e) => {
            tracing::error!("Failed to record intervention: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to record intervention".to_string() }),
            ))
        }
    }
}

/// Record outcome of an intervention
pub async fn record_outcome(
    State(state): State<AppState>,
    Path(intervention_id): Path<String>,
    Json(body): Json<RecordOutcomeBody>,
) -> impl IntoResponse {
    validate_org_id(&body.organization_id)?;

    let analytics_db = get_analytics_db(&state)?;

    let intervention_uuid = match Uuid::parse_str(&intervention_id) {
        Ok(id) => id,
        Err(_) => return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "Invalid intervention_id".to_string() }),
        )),
    };

    let learning = LearningSystem::new(&analytics_db);
    match learning.record_outcome(&intervention_uuid, &body.outcome_type, body.outcome_value, &body.post_metrics) {
        Ok(updated) => Ok(Json(serde_json::json!({
            "success": updated,
        }))),
        Err(e) => {
            tracing::error!("Failed to record outcome: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to record outcome".to_string() }),
            ))
        }
    }
}

/// Get learning effectiveness
pub async fn get_learning_effectiveness(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    validate_org_id(&query.org)?;

    let analytics_db = get_analytics_db(&state)?;

    let learning = LearningSystem::new(&analytics_db);
    match learning.get_organization_effectiveness(&query.org) {
        Ok(effectiveness) => Ok(Json(effectiveness)),
        Err(e) => {
            tracing::error!("Failed to get learning effectiveness: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to get effectiveness".to_string() }),
            ))
        }
    }
}

/// Apply learning adjustments to patterns
pub async fn apply_learning_adjustments(
    State(state): State<AppState>,
    Json(body): Json<OrgQuery>,
) -> impl IntoResponse {
    validate_org_id(&body.org)?;

    let analytics_db = get_analytics_db(&state)?;

    let learning = LearningSystem::new(&analytics_db);
    match learning.apply_learning_adjustments(&body.org) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            tracing::error!("Failed to apply learning adjustments: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Failed to apply adjustments".to_string() }),
            ))
        }
    }
}
