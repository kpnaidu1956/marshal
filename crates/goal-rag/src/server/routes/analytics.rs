//! Analytics API routes
//!
//! Provides endpoints for interaction analysis, timeline reconstruction,
//! pattern learning, and efficiency recommendations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
#[cfg(feature = "postgres")]
use chrono::DateTime;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::analytics::{
    AnalysisJob, AnalysisJobStatus, InteractionType, RecommendationStatus,
};
use crate::analytics::jobs::{AnalyticsJobProcessor, TaskAnalysisInput};
#[cfg(feature = "postgres")]
use crate::analytics::jobs::{TaskComment, RelatedMessage};
use crate::analytics::storage::AnalyticsDb;
#[cfg(feature = "postgres")]
use crate::analytics::timeline::ActivityEvent;
use crate::server::state::AppState;

// ==================== Request/Response Types ====================

#[derive(Debug, Deserialize)]
pub struct AnalyzeTaskRequest {
    pub organization_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeGoalRequest {
    pub organization_id: String,
}

/// Maximum allowed limit for queries to prevent DoS
const MAX_QUERY_LIMIT: usize = 500;

#[derive(Debug, Serialize)]
pub struct AnalysisJobResponse {
    pub job_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchInteractionsQuery {
    pub organization_id: String,
    #[serde(default)]
    pub interaction_type: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// Clamp limit to safe range
fn sanitize_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_QUERY_LIMIT)
}

/// Validate organization_id is non-empty and reasonable length
fn validate_org_id(org_id: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if org_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "organization_id is required" })),
        ));
    }
    if org_id.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "organization_id exceeds maximum length" })),
        ));
    }
    Ok(())
}

/// Validate entity_type is "task" or "goal"
/// Note: Reserved for future dynamic entity type support
#[allow(dead_code)]
fn validate_entity_type(entity_type: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if entity_type != "task" && entity_type != "goal" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "entity_type must be 'task' or 'goal'" })),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct OrgRecommendationsQuery {
    pub organization_id: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct PatternLearnRequest {
    pub organization_id: String,
}

#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    pub organization_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RecommendationFeedbackRequest {
    pub organization_id: String,
    pub status: String, // "accepted", "rejected", "implemented"
    #[serde(default)]
    pub feedback: Option<String>,
}

// ==================== Handlers ====================

/// Trigger analysis for a task
/// POST /api/analytics/analysis/task/:task_id
pub async fn analyze_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(request): Json<AnalyzeTaskRequest>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e;
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "task_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    // Create analysis job
    let job = AnalysisJob::new(
        request.organization_id.clone(),
        "task".to_string(),
        task_id.clone(),
    );

    if let Err(_e) = analytics_db.create_analysis_job(&job) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to create analysis job"
            })),
        );
    }

    // Spawn background task to fetch data from PostgreSQL and run analysis
    let job_id = job.id;
    spawn_task_analysis(state, job, analytics_db);

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "job_id": job_id.to_string(),
            "status": "pending",
            "message": "Analysis job created. Poll /api/analytics/jobs/{job_id} for status."
        })),
    )
}

/// Trigger analysis for a goal
/// POST /api/analytics/analysis/goal/:goal_id
pub async fn analyze_goal(
    State(state): State<AppState>,
    Path(goal_id): Path<String>,
    Json(request): Json<AnalyzeGoalRequest>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e;
    }
    if goal_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "goal_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    let job = AnalysisJob::new(
        request.organization_id.clone(),
        "goal".to_string(),
        goal_id.clone(),
    );

    if let Err(_e) = analytics_db.create_analysis_job(&job) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "Failed to create analysis job"
            })),
        );
    }

    // Spawn background task to fetch data from PostgreSQL and run analysis
    let job_id = job.id;
    spawn_goal_analysis(state, job, analytics_db);

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "job_id": job_id.to_string(),
            "status": "pending",
            "message": "Analysis job created. Poll /api/analytics/jobs/{job_id} for status."
        })),
    )
}

/// Get analysis job status
/// GET /api/analytics/jobs/:job_id?organization_id=xxx
pub async fn get_analysis_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    let uuid = match Uuid::parse_str(&job_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid job ID format" })),
            );
        }
    };

    match analytics_db.get_analysis_job(&uuid) {
        Ok(Some(job)) => {
            if job.organization_id != query.organization_id {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Job not found" })),
                );
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "id": job.id.to_string(),
                    "organization_id": job.organization_id,
                    "entity_type": job.entity_type,
                    "entity_id": job.entity_id,
                    "status": job.status.as_str(),
                    "progress_percent": job.progress_percent,
                    "current_stage": job.current_stage,
                    "interactions_found": job.interactions_found,
                    "interactions_classified": job.interactions_classified,
                    "patterns_matched": job.patterns_matched,
                    "recommendations_generated": job.recommendations_generated,
                    "error": job.error,
                    "created_at": job.created_at.to_rfc3339(),
                    "updated_at": job.updated_at.to_rfc3339(),
                    "completed_at": job.completed_at.map(|t| t.to_rfc3339()),
                })),
            )
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Job not found" })),
        ),
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to retrieve job" })),
        ),
    }
}

/// Get timeline for a task
/// GET /api/analytics/timeline/task/:task_id?organization_id=xxx
pub async fn get_task_timeline(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "task_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    match analytics_db.get_timeline("task", &task_id) {
        Ok(Some(timeline)) => {
            if timeline.organization_id != query.organization_id {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "Timeline not found. Trigger analysis first with POST /api/analytics/analysis/task/:task_id"
                    })),
                );
            }
            match serde_json::to_value(timeline) {
                Ok(value) => (StatusCode::OK, Json(value)),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to serialize timeline" })),
                ),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Timeline not found. Trigger analysis first with POST /api/analytics/analysis/task/:task_id"
            })),
        ),
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to retrieve timeline" })),
        ),
    }
}

/// Get timeline for a goal
/// GET /api/analytics/timeline/goal/:goal_id?organization_id=xxx
pub async fn get_goal_timeline(
    State(state): State<AppState>,
    Path(goal_id): Path<String>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }
    if goal_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "goal_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    match analytics_db.get_timeline("goal", &goal_id) {
        Ok(Some(timeline)) => {
            if timeline.organization_id != query.organization_id {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "Timeline not found. Trigger analysis first with POST /api/analytics/analysis/goal/:goal_id"
                    })),
                );
            }
            match serde_json::to_value(timeline) {
                Ok(value) => (StatusCode::OK, Json(value)),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to serialize timeline" })),
                ),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Timeline not found. Trigger analysis first with POST /api/analytics/analysis/goal/:goal_id"
            })),
        ),
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to retrieve timeline" })),
        ),
    }
}

/// Get classified interactions for a task
/// GET /api/analytics/interactions/task/:task_id?organization_id=xxx
pub async fn get_task_interactions(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "task_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    match analytics_db.get_classifications_for_task(&task_id) {
        Ok(classifications) => {
            // Filter to only return classifications belonging to the requested organization
            let response: Vec<serde_json::Value> = classifications
                .into_iter()
                .filter(|c| c.organization_id == query.organization_id)
                .map(|c| {
                    serde_json::json!({
                        "id": c.id.to_string(),
                        "source_type": c.source_type.as_str(),
                        "source_id": c.source_id,
                        "sender_id": c.sender_id,
                        "content": c.content,
                        "interaction_type": c.interaction_type.as_str(),
                        "secondary_types": c.secondary_types.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
                        "confidence_score": c.confidence_score,
                        "sentiment": c.sentiment,
                        "urgency_level": c.urgency_level.as_str(),
                        "entities": c.entities,
                        "original_created_at": c.original_created_at.to_rfc3339(),
                        "classified_at": c.classified_at.to_rfc3339(),
                    })
                })
                .collect();

            (StatusCode::OK, Json(serde_json::json!({ "interactions": response })))
        }
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to retrieve interactions" })),
        ),
    }
}

/// Search interactions by type
/// GET /api/analytics/interactions/search
pub async fn search_interactions(
    State(state): State<AppState>,
    Query(query): Query<SearchInteractionsQuery>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }

    let limit = sanitize_limit(query.limit);

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    // Parse interaction type - if not provided or invalid, search for "other" type
    // Note: A future enhancement could search all types when None is provided
    let interaction_type = query
        .interaction_type
        .as_ref()
        .map(|t| InteractionType::parse(t))
        .unwrap_or(InteractionType::Other);

    match analytics_db.search_classifications_by_type(
        &query.organization_id,
        interaction_type,
        limit,
    ) {
        Ok(classifications) => {
            let response: Vec<serde_json::Value> = classifications
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id.to_string(),
                        "task_id": c.task_id,
                        "goal_id": c.goal_id,
                        "source_type": c.source_type.as_str(),
                        "sender_id": c.sender_id,
                        "content": c.content,
                        "interaction_type": c.interaction_type.as_str(),
                        "confidence_score": c.confidence_score,
                        "urgency_level": c.urgency_level.as_str(),
                        "original_created_at": c.original_created_at.to_rfc3339(),
                    })
                })
                .collect();

            (StatusCode::OK, Json(serde_json::json!({ "interactions": response })))
        }
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to search interactions" })),
        ),
    }
}

/// List learned patterns
/// GET /api/analytics/patterns?organization_id=xxx
pub async fn list_patterns(
    State(state): State<AppState>,
    Query(query): Query<OrgRecommendationsQuery>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    match analytics_db.get_patterns(&query.organization_id) {
        Ok(patterns) => {
            let response: Vec<serde_json::Value> = patterns
                .into_iter()
                .filter_map(|p| serde_json::to_value(p).ok())
                .collect();

            (StatusCode::OK, Json(serde_json::json!({ "patterns": response })))
        }
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to retrieve patterns" })),
        ),
    }
}

/// Trigger pattern learning
/// POST /api/analytics/patterns/learn
pub async fn trigger_pattern_learning(
    State(state): State<AppState>,
    Json(request): Json<PatternLearnRequest>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e;
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    let org_id = request.organization_id.clone();
    #[cfg(feature = "postgres")]
    let entity_store = state.entity_embedding_store().cloned();

    tokio::spawn(async move {
        let pattern_learner = crate::analytics::pattern_learner::PatternLearner::default();

        // 1. Timeline-based patterns (existing PatternLearner)
        match analytics_db.get_timelines_for_org(&org_id) {
            Ok(timelines) if !timelines.is_empty() => {
                let patterns = pattern_learner.learn_patterns(&org_id, &timelines);
                for pattern in &patterns {
                    if pattern.confidence_score >= 0.5 {
                        if let Err(e) = analytics_db.upsert_pattern(pattern) {
                            tracing::warn!("Failed to store timeline pattern: {}", e);
                        }
                    }
                }
                tracing::info!(
                    org_id = %org_id,
                    timelines = timelines.len(),
                    patterns = patterns.len(),
                    "Timeline-based pattern learning complete"
                );
            }
            Ok(_) => {
                tracing::info!(org_id = %org_id, "No timelines found for pattern learning");
            }
            Err(e) => {
                tracing::warn!(org_id = %org_id, "Failed to get timelines: {}", e);
            }
        }

        // 2. Embedding-derived sentiment patterns from completed tasks
        #[cfg(feature = "postgres")]
        if let Some(ref store) = entity_store {
            let emb_org_id = match store.resolve_org_id(&org_id).await {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!(org_id = %org_id, "Failed to resolve org UUID for embeddings: {}", e);
                    return;
                }
            };
            match store.pool_client().await {
                Ok(client) => {
                    let result = client.query(
                        r#"
                        SELECT entity_id, sentiment
                        FROM entity_embeddings
                        WHERE organization_id = $1
                          AND entity_type = 'task'
                          AND status IN ('completed', 'done', 'Done', 'Completed')
                          AND sentiment IS NOT NULL
                        "#,
                        &[&emb_org_id],
                    ).await;

                    if let Ok(rows) = result {
                        let sentiments: Vec<f32> = rows.iter()
                            .filter_map(|r| r.get::<_, Option<f32>>("sentiment"))
                            .collect();

                        if !sentiments.is_empty() {
                            let avg: f32 = sentiments.iter().sum::<f32>() / sentiments.len() as f32;
                            let now = Utc::now();

                            if avg > 0.3 {
                                let pattern = crate::analytics::WorkflowPattern {
                                    id: Uuid::new_v4(),
                                    organization_id: org_id.clone(),
                                    pattern_type: crate::analytics::PatternType::Success,
                                    pattern_name: "positive_sentiment_completion".to_string(),
                                    description: format!(
                                        "Completed tasks have positive average sentiment ({:.2}). {} tasks analyzed.",
                                        avg, sentiments.len()
                                    ),
                                    criteria: serde_json::json!({
                                        "source": "entity_embeddings",
                                        "avg_sentiment": avg,
                                        "task_count": sentiments.len(),
                                        "threshold": 0.3
                                    }),
                                    occurrence_count: sentiments.len() as u32,
                                    success_correlation: Some(avg),
                                    avg_time_impact_hours: None,
                                    confidence_score: (sentiments.len() as f32 / 10.0).clamp(0.5, 1.0),
                                    examples: rows.iter().take(5)
                                        .map(|r| r.get::<_, Uuid>("entity_id").to_string())
                                        .collect(),
                                    is_active: true,
                                    created_at: now,
                                    updated_at: now,
                                };
                                if let Err(e) = analytics_db.upsert_pattern(&pattern) {
                                    tracing::warn!("Failed to store positive sentiment pattern: {}", e);
                                }
                            }

                            if avg < -0.2 {
                                let pattern = crate::analytics::WorkflowPattern {
                                    id: Uuid::new_v4(),
                                    organization_id: org_id.clone(),
                                    pattern_type: crate::analytics::PatternType::Failure,
                                    pattern_name: "negative_sentiment_risk".to_string(),
                                    description: format!(
                                        "Completed tasks show negative average sentiment ({:.2}). {} tasks analyzed.",
                                        avg, sentiments.len()
                                    ),
                                    criteria: serde_json::json!({
                                        "source": "entity_embeddings",
                                        "avg_sentiment": avg,
                                        "task_count": sentiments.len(),
                                        "threshold": -0.2
                                    }),
                                    occurrence_count: sentiments.len() as u32,
                                    success_correlation: Some(avg),
                                    avg_time_impact_hours: None,
                                    confidence_score: (sentiments.len() as f32 / 10.0).clamp(0.5, 1.0),
                                    examples: rows.iter().take(5)
                                        .map(|r| r.get::<_, Uuid>("entity_id").to_string())
                                        .collect(),
                                    is_active: true,
                                    created_at: now,
                                    updated_at: now,
                                };
                                if let Err(e) = analytics_db.upsert_pattern(&pattern) {
                                    tracing::warn!("Failed to store negative sentiment pattern: {}", e);
                                }
                            }

                            tracing::info!(
                                org_id = %org_id,
                                avg_sentiment = avg,
                                tasks_analyzed = sentiments.len(),
                                "Embedding sentiment pattern analysis complete"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get PG client for pattern learning: {}", e);
                }
            }
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "Pattern learning triggered",
            "organization_id": request.organization_id,
            "status": "processing"
        })),
    )
}

/// Get recommendations for a task
/// GET /api/analytics/recommendations/task/:task_id?organization_id=xxx
pub async fn get_task_recommendations(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "task_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    // Get stored recommendations
    let mut response: Vec<serde_json::Value> = match analytics_db.get_recommendations_for_target("task", &task_id) {
        Ok(recommendations) => recommendations
            .into_iter()
            .filter(|r| r.organization_id == query.organization_id)
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect(),
        Err(_e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve recommendations" })),
            );
        }
    };

    // Enhance with similar completed task lookup via entity embeddings
    #[cfg(feature = "postgres")]
    {
        if let Some(store) = state.entity_embedding_store() {
            if let Ok(task_uuid) = Uuid::parse_str(&task_id) {
                let emb_org = store.resolve_org_id(&query.organization_id).await
                    .unwrap_or_else(|_| query.organization_id.clone());
                if let Ok(similar) = store.search_similar_to_entity(
                    "task",
                    &task_uuid,
                    &emb_org,
                    Some("task"),
                    10,
                ).await {
                    let completed: Vec<_> = similar.iter()
                        .filter(|r| r.similarity > 0.7)
                        .filter(|r| matches!(r.status.as_deref(), Some("completed" | "done" | "Done" | "Completed")))
                        .collect();

                    if !completed.is_empty() {
                        let avg_similarity: f32 = completed.iter().map(|r| r.similarity).sum::<f32>() / completed.len() as f32;
                        let sentiment_vals: Vec<f32> = completed.iter().filter_map(|r| r.sentiment).collect();
                        let avg_sentiment: f32 = if sentiment_vals.is_empty() { 0.0 } else {
                            sentiment_vals.iter().sum::<f32>() / sentiment_vals.len() as f32
                        };
                        let similar_ids: Vec<String> = completed.iter()
                            .map(|r| r.entity_id.to_string())
                            .collect();

                        response.push(serde_json::json!({
                            "id": Uuid::new_v4().to_string(),
                            "organization_id": query.organization_id,
                            "target_type": "task",
                            "target_id": task_id,
                            "recommendation_type": "process",
                            "title": "Similar Completed Tasks Found",
                            "description": format!(
                                "{} similar completed tasks found (avg similarity: {:.0}%, avg sentiment: {:.2}). Review their outcomes for insights.",
                                completed.len(),
                                avg_similarity * 100.0,
                                avg_sentiment
                            ),
                            "suggested_actions": [
                                format!("Review completed tasks: {}", similar_ids.join(", ")),
                                "Apply successful patterns from similar tasks",
                            ],
                            "based_on_patterns": [],
                            "evidence": {
                                "source": "entity_embeddings",
                                "similar_task_ids": similar_ids,
                                "avg_similarity": avg_similarity,
                                "avg_sentiment": avg_sentiment,
                            },
                            "priority": "medium",
                            "status": "pending",
                            "generated_at": Utc::now().to_rfc3339(),
                        }));
                    }
                }
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!({ "recommendations": response })))
}

/// Get organization-wide recommendations
/// GET /api/analytics/recommendations/organization?organization_id=xxx
pub async fn get_org_recommendations(
    State(state): State<AppState>,
    Query(query): Query<OrgRecommendationsQuery>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }

    let limit = sanitize_limit(query.limit);

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    match analytics_db.get_org_recommendations(&query.organization_id, limit) {
        Ok(recommendations) => {
            let response: Vec<serde_json::Value> = recommendations
                .into_iter()
                .filter_map(|r| serde_json::to_value(r).ok())
                .collect();

            (StatusCode::OK, Json(serde_json::json!({ "recommendations": response })))
        }
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to retrieve recommendations" })),
        ),
    }
}

/// Submit feedback on a recommendation
/// POST /api/analytics/recommendations/:id/feedback
pub async fn submit_recommendation_feedback(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(feedback): Json<RecommendationFeedbackRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&feedback.organization_id) {
        return e;
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    let uuid = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid recommendation ID format" })),
            );
        }
    };

    let status = RecommendationStatus::parse(&feedback.status);

    match analytics_db.update_recommendation_feedback(&uuid, status, feedback.feedback.as_deref()) {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Feedback recorded",
                "status": feedback.status
            })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Recommendation not found" })),
        ),
        Err(_e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to update feedback" })),
        ),
    }
}

/// Analytics info endpoint
pub async fn analytics_info() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "Interaction Analytics API",
        "version": "1.0.0",
        "description": "Analyze team communications, reconstruct workflow timelines, and generate efficiency recommendations",
        "status": "active",
        "endpoints": {
            "POST /api/analytics/analysis/task/:task_id": "Trigger analysis for a task",
            "POST /api/analytics/analysis/goal/:goal_id": "Trigger analysis for a goal",
            "GET /api/analytics/jobs/:job_id": "Get analysis job status",
            "GET /api/analytics/timeline/task/:task_id": "Get task workflow timeline",
            "GET /api/analytics/timeline/goal/:goal_id": "Get goal workflow timeline",
            "GET /api/analytics/interactions/task/:task_id": "Get classified interactions for a task",
            "GET /api/analytics/interactions/search": "Search interactions by type (query params: organization_id, interaction_type, limit)",
            "GET /api/analytics/patterns": "List learned workflow patterns",
            "POST /api/analytics/patterns/learn": "Trigger pattern learning",
            "GET /api/analytics/recommendations/task/:task_id": "Get task recommendations",
            "GET /api/analytics/recommendations/organization": "Get org-wide recommendations",
            "POST /api/analytics/recommendations/:id/feedback": "Submit feedback on a recommendation",
            "GET /api/analytics/user/:user_id/performance": "Get user task/goal counts by status (query: organization_id, from_date?, to_date?)",
            "GET /api/analytics/user/:user_id/interactions": "Get user interaction aggregations (query: organization_id, days?)",
            "GET /api/analytics/user/:user_id/sentiment": "Get user sentiment time series (query: organization_id, from_date?, to_date?)",
            "POST /api/analytics/batch/comments": "Process all existing comments for sentiment analysis (body: organization_id)"
        },
        "interaction_types": [
            "request_clarification",
            "request_resources",
            "direction",
            "suggestion",
            "request_approval",
            "status_update",
            "acknowledgment",
            "escalation",
            "blocker",
            "question",
            "answer",
            "assignment",
            "feedback",
            "recognition",
            "other"
        ]
    }))
}

// ==================== Background Job Processing ====================

/// Spawn background task analysis job
fn spawn_task_analysis(
    state: AppState,
    mut job: AnalysisJob,
    analytics_db: Arc<AnalyticsDb>,
) {
    tokio::spawn(async move {
        let task_id = job.entity_id.clone();
        let org_id = job.organization_id.clone();

        tracing::info!(
            job_id = %job.id,
            task_id = %task_id,
            org_id = %org_id,
            "Starting background task analysis"
        );

        // Fetch task data from PostgreSQL
        let task_input = match fetch_task_data_from_pg(&state, &task_id, &org_id).await {
            Ok(input) => input,
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "Failed to fetch task data from PostgreSQL");
                job.status = AnalysisJobStatus::Failed;
                job.error = Some(format!("Failed to fetch task data: {}", e));
                job.updated_at = Utc::now();
                let _ = analytics_db.update_analysis_job(&job);
                return;
            }
        };

        // Create processor with fast rule-based classifier (instant)
        // Use with_ollama() instead if higher-quality LLM classification is needed
        let processor = AnalyticsJobProcessor::with_rule_based(Arc::clone(&analytics_db));

        match processor.process_task_analysis(&mut job, task_input).await {
            Ok(result) => {
                tracing::info!(
                    job_id = %job.id,
                    interactions = result.classifications.len(),
                    recommendations = result.recommendations.len(),
                    "Task analysis completed"
                );
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "Task analysis failed");
                job.status = AnalysisJobStatus::Failed;
                job.error = Some(format!("Analysis failed: {}", e));
                job.updated_at = Utc::now();
                let _ = analytics_db.update_analysis_job(&job);
            }
        }
    });
}

/// Spawn background goal analysis job
fn spawn_goal_analysis(
    state: AppState,
    mut job: AnalysisJob,
    analytics_db: Arc<AnalyticsDb>,
) {
    tokio::spawn(async move {
        let goal_id = job.entity_id.clone();
        let org_id = job.organization_id.clone();

        tracing::info!(
            job_id = %job.id,
            goal_id = %goal_id,
            org_id = %org_id,
            "Starting background goal analysis"
        );

        // Fetch goal's tasks from PostgreSQL and analyze them collectively
        let task_input = match fetch_goal_data_from_pg(&state, &goal_id, &org_id).await {
            Ok(input) => input,
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "Failed to fetch goal data from PostgreSQL");
                job.status = AnalysisJobStatus::Failed;
                job.error = Some(format!("Failed to fetch goal data: {}", e));
                job.updated_at = Utc::now();
                let _ = analytics_db.update_analysis_job(&job);
                return;
            }
        };

        // Create processor with fast rule-based classifier (instant)
        let processor = AnalyticsJobProcessor::with_rule_based(Arc::clone(&analytics_db));

        match processor.process_task_analysis(&mut job, task_input).await {
            Ok(result) => {
                tracing::info!(
                    job_id = %job.id,
                    interactions = result.classifications.len(),
                    recommendations = result.recommendations.len(),
                    "Goal analysis completed"
                );
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "Goal analysis failed");
                job.status = AnalysisJobStatus::Failed;
                job.error = Some(format!("Analysis failed: {}", e));
                job.updated_at = Utc::now();
                let _ = analytics_db.update_analysis_job(&job);
            }
        }
    });
}

/// Fetch task data from PostgreSQL for analysis
///
/// Schema notes:
/// - api.tasks has organization_id (UUID, NOT NULL)
/// - api.tasks has no completed_at column
/// - api.task_comments has organization_id and uses author_id (not user_id) and content (not body)
/// - api.task_activity_logs has organization_id and uses changed_by (not user_id), changes jsonb (not details text)
/// - api.messages has organization_id (UUID, NOT NULL)
#[cfg(feature = "postgres")]
async fn fetch_task_data_from_pg(
    state: &AppState,
    task_id: &str,
    org_id: &str,
) -> std::result::Result<TaskAnalysisInput, String> {
    let pool = state.pg_pool()
        .ok_or_else(|| "PostgreSQL pool not available".to_string())?;
    let client = pool.get().await
        .map_err(|e| format!("Failed to get PG connection: {}", e))?;

    // Parse task_id and org_id as UUIDs for parameterized queries
    let task_uuid = uuid::Uuid::parse_str(task_id)
        .map_err(|_| format!("Invalid task_id UUID: {}", task_id))?;
    let org_uuid = uuid::Uuid::parse_str(org_id)
        .map_err(|_| format!("Invalid organization_id UUID: {}", org_id))?;

    // Fetch task details with organization_id filter
    let task_row = client
        .query_opt(
            "SELECT id, title, status, goal_id, created_at, updated_at \
             FROM api.tasks WHERE id = $1 AND organization_id = $2",
            &[&task_uuid, &org_uuid],
        )
        .await
        .map_err(|e| format!("Failed to query task: {}", e))?
        .ok_or_else(|| format!("Task {} not found in organization {}", task_id, org_id))?;

    let task_title: String = task_row.get("title");
    let status: String = task_row.get("status");
    let goal_id: Option<String> = task_row.get::<_, Option<uuid::Uuid>>("goal_id").map(|u| u.to_string());
    let created_at: DateTime<Utc> = task_row.get("created_at");
    // Tasks have no completed_at; use updated_at if status is done
    let completed_at: Option<DateTime<Utc>> = if status == "Done" || status == "Completed" {
        Some(task_row.get("updated_at"))
    } else {
        None
    };

    // Run independent queries concurrently via tokio::join! (pipelining on same connection)
    // This saves ~50-60% latency vs sequential execution
    let search_pattern = format!("%{}%", task_id);
    // Pre-bind parameter slices so they outlive the tokio::join! futures
    let comment_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&task_uuid, &org_uuid];
    let message_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&search_pattern, &org_uuid];
    let activity_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&task_uuid, &org_uuid];
    let (comment_result, message_result, activity_result, goal_title) = tokio::join!(
        // Fetch task comments
        client.query(
            "SELECT id, author_id, content, created_at \
             FROM api.task_comments \
             WHERE task_id = $1 AND organization_id = $2 \
             ORDER BY created_at ASC \
             LIMIT 500",
            &comment_params,
        ),
        // Fetch related messages
        client.query(
            "SELECT id, sender_id, content, created_at \
             FROM api.messages \
             WHERE content ILIKE $1 AND organization_id = $2 \
             ORDER BY created_at ASC \
             LIMIT 100",
            &message_params,
        ),
        // Fetch activity logs
        client.query(
            "SELECT id, action, changed_by, changed_by_name, created_at, changes \
             FROM api.task_activity_logs \
             WHERE task_id = $1 AND organization_id = $2 \
             ORDER BY created_at ASC \
             LIMIT 500",
            &activity_params,
        ),
        // Fetch goal title
        async {
            if let Some(ref gid) = goal_id {
                if let Some(gu) = uuid::Uuid::parse_str(gid).ok() {
                    client
                        .query_opt(
                            "SELECT title FROM api.goals WHERE id = $1 AND organization_id = $2",
                            &[&gu, &org_uuid],
                        )
                        .await
                        .ok()
                        .flatten()
                        .map(|row| row.get::<_, String>("title"))
                } else {
                    None
                }
            } else {
                None
            }
        },
    );

    let comment_rows = comment_result.unwrap_or_default();
    let message_rows = message_result.unwrap_or_default();
    let activity_rows = activity_result.unwrap_or_default();

    let comments: Vec<TaskComment> = comment_rows
        .iter()
        .map(|row| TaskComment {
            id: row.get::<_, uuid::Uuid>("id").to_string(),
            author_id: row.get::<_, uuid::Uuid>("author_id").to_string(),
            content: row.get::<_, String>("content"),
            created_at: row.get("created_at"),
        })
        .collect();

    let related_messages: Vec<RelatedMessage> = message_rows
        .iter()
        .map(|row| RelatedMessage {
            id: row.get::<_, uuid::Uuid>("id").to_string(),
            sender_id: row.get::<_, uuid::Uuid>("sender_id").to_string(),
            content: row.get::<_, String>("content"),
            created_at: row.get("created_at"),
        })
        .collect();

    let activity_events: Vec<ActivityEvent> = activity_rows
        .iter()
        .map(|row| {
            let changes_json: Option<serde_json::Value> = row.get("changes");
            ActivityEvent {
                id: row.get::<_, uuid::Uuid>("id").to_string(),
                action: row.get::<_, String>("action"),
                description: changes_json.as_ref()
                    .and_then(|v| serde_json::to_string(v).ok())
                    .unwrap_or_default(),
                actor_id: row.get::<_, Option<uuid::Uuid>>("changed_by")
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
                actor_name: row.get("changed_by_name"),
                timestamp: row.get("created_at"),
                changes: changes_json,
            }
        })
        .collect();

    tracing::info!(
        task_id = %task_id,
        comments = comments.len(),
        messages = related_messages.len(),
        events = activity_events.len(),
        "Fetched task data from PostgreSQL"
    );

    Ok(TaskAnalysisInput {
        task_id: task_id.to_string(),
        task_title,
        goal_id,
        goal_title,
        status,
        created_at,
        completed_at,
        comments,
        related_messages,
        activity_events,
    })
}

/// Fetch task data without PostgreSQL - returns minimal input from available context
#[cfg(not(feature = "postgres"))]
async fn fetch_task_data_from_pg(
    _state: &AppState,
    task_id: &str,
    _org_id: &str,
) -> std::result::Result<TaskAnalysisInput, String> {
    Ok(TaskAnalysisInput {
        task_id: task_id.to_string(),
        task_title: String::new(),
        goal_id: None,
        goal_title: None,
        status: "unknown".to_string(),
        created_at: Utc::now(),
        completed_at: None,
        comments: vec![],
        related_messages: vec![],
        activity_events: vec![],
    })
}

/// Fetch goal data from PostgreSQL for analysis
///
/// Schema notes:
/// - api.goals has organization_id (UUID)
/// - api.tasks has organization_id (UUID, NOT NULL)
/// - api.task_comments has organization_id and uses author_id and content
/// - api.task_activity_logs has organization_id and uses changed_by, changed_by_name, changes (jsonb)
#[cfg(feature = "postgres")]
async fn fetch_goal_data_from_pg(
    state: &AppState,
    goal_id: &str,
    org_id: &str,
) -> std::result::Result<TaskAnalysisInput, String> {
    let pool = state.pg_pool()
        .ok_or_else(|| "PostgreSQL pool not available".to_string())?;
    let client = pool.get().await
        .map_err(|e| format!("Failed to get PG connection: {}", e))?;

    let goal_uuid = uuid::Uuid::parse_str(goal_id)
        .map_err(|_| format!("Invalid goal_id UUID: {}", goal_id))?;
    let org_uuid = uuid::Uuid::parse_str(org_id)
        .map_err(|_| format!("Invalid organization_id UUID: {}", org_id))?;

    // Fetch goal details (goals table has organization_id)
    let goal_row = client
        .query_opt(
            "SELECT id, title, status, created_at \
             FROM api.goals WHERE id = $1 AND organization_id = $2",
            &[&goal_uuid, &org_uuid],
        )
        .await
        .map_err(|e| format!("Failed to query goal: {}", e))?
        .ok_or_else(|| format!("Goal {} not found in organization {}", goal_id, org_id))?;

    let goal_title: String = goal_row.get("title");
    let status: String = goal_row.get::<_, Option<String>>("status").unwrap_or_else(|| "not_started".to_string());
    let created_at: DateTime<Utc> = goal_row.get::<_, Option<DateTime<Utc>>>("created_at").unwrap_or_else(Utc::now);

    // Run comment and activity log queries concurrently (independent of each other)
    // Pre-bind parameter slices so they outlive the tokio::join! futures
    let goal_comment_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&goal_uuid, &org_uuid];
    let goal_activity_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&goal_uuid, &org_uuid];
    let (comment_result, activity_result) = tokio::join!(
        client.query(
            "SELECT tc.id, tc.author_id, tc.content, tc.created_at \
             FROM api.task_comments tc \
             JOIN api.tasks t ON tc.task_id = t.id \
             WHERE t.goal_id = $1 AND tc.organization_id = $2 \
             ORDER BY tc.created_at ASC \
             LIMIT 1000",
            &goal_comment_params,
        ),
        client.query(
            "SELECT tal.id, tal.action, tal.changed_by, tal.changed_by_name, tal.created_at, tal.changes \
             FROM api.task_activity_logs tal \
             JOIN api.tasks t ON tal.task_id = t.id \
             WHERE t.goal_id = $1 AND tal.organization_id = $2 \
             ORDER BY tal.created_at ASC \
             LIMIT 1000",
            &goal_activity_params,
        ),
    );

    let comment_rows = comment_result.unwrap_or_default();
    let activity_rows = activity_result.unwrap_or_default();

    let comments: Vec<TaskComment> = comment_rows
        .iter()
        .map(|row| TaskComment {
            id: row.get::<_, uuid::Uuid>("id").to_string(),
            author_id: row.get::<_, uuid::Uuid>("author_id").to_string(),
            content: row.get::<_, String>("content"),
            created_at: row.get("created_at"),
        })
        .collect();

    let activity_events: Vec<ActivityEvent> = activity_rows
        .iter()
        .map(|row| {
            let changes_json: Option<serde_json::Value> = row.get("changes");
            ActivityEvent {
                id: row.get::<_, uuid::Uuid>("id").to_string(),
                action: row.get::<_, String>("action"),
                description: changes_json.as_ref()
                    .and_then(|v| serde_json::to_string(v).ok())
                    .unwrap_or_default(),
                actor_id: row.get::<_, Option<uuid::Uuid>>("changed_by")
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
                actor_name: row.get("changed_by_name"),
                timestamp: row.get("created_at"),
                changes: changes_json,
            }
        })
        .collect();

    tracing::info!(
        goal_id = %goal_id,
        comments = comments.len(),
        events = activity_events.len(),
        "Fetched goal data from PostgreSQL"
    );

    Ok(TaskAnalysisInput {
        task_id: goal_id.to_string(), // Reusing task_id field for goal
        task_title: goal_title.clone(),
        goal_id: Some(goal_id.to_string()),
        goal_title: Some(goal_title),
        status,
        created_at,
        completed_at: None,
        comments,
        related_messages: vec![],
        activity_events,
    })
}

/// Fetch goal data without PostgreSQL
#[cfg(not(feature = "postgres"))]
async fn fetch_goal_data_from_pg(
    _state: &AppState,
    goal_id: &str,
    _org_id: &str,
) -> std::result::Result<TaskAnalysisInput, String> {
    Ok(TaskAnalysisInput {
        task_id: goal_id.to_string(),
        task_title: String::new(),
        goal_id: Some(goal_id.to_string()),
        goal_title: None,
        status: "unknown".to_string(),
        created_at: Utc::now(),
        completed_at: None,
        comments: vec![],
        related_messages: vec![],
        activity_events: vec![],
    })
}

// ==================== User Analytics Endpoints ====================

#[derive(Debug, Deserialize)]
pub struct UserPerformanceQuery {
    pub organization_id: String,
    #[serde(default)]
    pub from_date: Option<String>,
    #[serde(default)]
    pub to_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserInteractionsQuery {
    pub organization_id: String,
    #[serde(default = "default_days")]
    pub days: u32,
}

fn default_days() -> u32 {
    30
}

#[derive(Debug, Deserialize)]
pub struct UserSentimentQuery {
    pub organization_id: String,
    #[serde(default)]
    pub from_date: Option<String>,
    #[serde(default)]
    pub to_date: Option<String>,
}

/// Get user performance metrics (task/goal counts by status)
/// GET /api/analytics/user/:user_id/performance?org=&from_date=&to_date=
pub async fn get_user_performance(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<UserPerformanceQuery>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }
    if user_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "user_id is required" })),
        );
    }

    #[cfg(feature = "postgres")]
    {
        let pool = match state.pg_pool() {
            Some(p) => p,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({ "error": "PostgreSQL not available" })),
                );
            }
        };

        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to get PG connection: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Database connection failed" })),
                );
            }
        };

        let user_uuid = match uuid::Uuid::parse_str(&user_id) {
            Ok(u) => u,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Invalid user_id UUID" })),
                );
            }
        };

        let org_uuid = match uuid::Uuid::parse_str(&query.organization_id) {
            Ok(u) => u,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Invalid organization_id UUID" })),
                );
            }
        };

        // Parse date range (default to last 30 days)
        let now = Utc::now();
        let from_date = query.from_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
            .unwrap_or_else(|| now - chrono::Duration::days(30));
        let to_date = query.to_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| d.and_hms_opt(23, 59, 59).unwrap().and_utc())
            .unwrap_or(now);

        // Query tasks assigned to user with status counts
        // Tasks use assigned_to column for user assignment
        // Org scoping: direct organization_id filter on tasks table
        let task_stats = client
            .query(
                r#"
                SELECT status, COUNT(*) as count
                FROM api.tasks
                WHERE assigned_to = $1
                  AND organization_id = $2
                  AND created_at >= $3
                  AND created_at <= $4
                GROUP BY status
                "#,
                &[&user_uuid, &org_uuid, &from_date, &to_date],
            )
            .await
            .unwrap_or_default();

        let mut tasks_by_status: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for row in &task_stats {
            let status: String = row.get("status");
            let count: i64 = row.get("count");
            tasks_by_status.insert(status, count);
        }

        // Query goals created by user with status counts
        let goal_stats = client
            .query(
                r#"
                SELECT status, COUNT(*) as count
                FROM api.goals
                WHERE created_by = $1
                  AND organization_id = $2
                  AND created_at >= $3
                  AND created_at <= $4
                GROUP BY status
                "#,
                &[&user_uuid, &org_uuid, &from_date, &to_date],
            )
            .await
            .unwrap_or_default();

        let mut goals_by_status: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for row in &goal_stats {
            let status: Option<String> = row.get("status");
            let count: i64 = row.get("count");
            goals_by_status.insert(status.unwrap_or_else(|| "not_started".to_string()), count);
        }

        // Calculate totals
        let total_tasks: i64 = tasks_by_status.values().sum();
        let completed_tasks = tasks_by_status.get("Done").copied().unwrap_or(0)
            + tasks_by_status.get("Completed").copied().unwrap_or(0);
        let total_goals: i64 = goals_by_status.values().sum();
        let completed_goals = goals_by_status.get("completed").copied().unwrap_or(0)
            + goals_by_status.get("done").copied().unwrap_or(0);

        (
            StatusCode::OK,
            Json(serde_json::json!({
                "user_id": user_id,
                "organization_id": query.organization_id,
                "period": {
                    "from": from_date.to_rfc3339(),
                    "to": to_date.to_rfc3339()
                },
                "tasks": {
                    "total": total_tasks,
                    "completed": completed_tasks,
                    "by_status": tasks_by_status
                },
                "goals": {
                    "total": total_goals,
                    "completed": completed_goals,
                    "by_status": goals_by_status
                },
                "completion_rate": if total_tasks > 0 {
                    (completed_tasks as f64 / total_tasks as f64 * 100.0).round()
                } else {
                    0.0
                }
            })),
        )
    }

    #[cfg(not(feature = "postgres"))]
    {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "PostgreSQL feature not enabled" })),
        )
    }
}

/// Get user interaction analytics (aggregated data with other users)
/// GET /api/analytics/user/:user_id/interactions?org=&days=
pub async fn get_user_interactions(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<UserInteractionsQuery>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }
    if user_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "user_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    // Calculate date range
    let now = Utc::now();
    let days = query.days.min(365); // Cap at 1 year
    let from_date = now - chrono::Duration::days(days as i64);

    // Get this user's classifications directly (pushes sender_id filter to SQL)
    let user_interactions = match analytics_db.get_classifications_for_user_in_range(&query.organization_id, &user_id, &from_date, &now) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get classifications: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve interaction data" })),
            );
        }
    };

    // Build collaborator map: find other users on the same tasks/goals
    // Only query entity IDs this user participated in, then find other participants
    let user_entity_ids: std::collections::HashSet<String> = user_interactions
        .iter()
        .filter_map(|c| c.task_id.clone().or_else(|| c.goal_id.clone()))
        .collect();

    // Only fetch full org classifications if user has entity interactions (for collaborator finding)
    let mut entity_to_users: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();
    if !user_entity_ids.is_empty() {
        // Fetch org-wide data for collaborator mapping
        if let Ok(all_classifications) = analytics_db.get_classifications_in_range(&query.organization_id, &from_date, &now) {
            for c in &all_classifications {
                let entity_key = c.task_id.clone().or_else(|| c.goal_id.clone());
                if let Some(ref key) = entity_key {
                    if user_entity_ids.contains(key) {
                        entity_to_users
                            .entry(key.clone())
                            .or_default()
                            .insert(c.sender_id.clone());
                    }
                }
            }
        }
    }

    // Aggregate interactions by recipient (other users on same entity or mentioned)
    let mut interaction_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut interaction_types: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut total_sentiment: f32 = 0.0;
    let mut sentiment_count: u32 = 0;

    for interaction in &user_interactions {
        // Count interaction types
        let type_key = interaction.interaction_type.as_str().to_string();
        *interaction_types.entry(type_key).or_insert(0) += 1;

        // Track sentiment
        total_sentiment += interaction.sentiment;
        sentiment_count += 1;

        // Track collaborators: other users who also interacted on the same task/goal
        let entity_key = interaction.task_id.clone().or_else(|| interaction.goal_id.clone());
        if let Some(key) = entity_key {
            if let Some(other_users) = entity_to_users.get(&key) {
                for other_user in other_users {
                    if other_user != &user_id {
                        *interaction_counts.entry(other_user.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        // Also track mentioned users (if the classifier extracts them in future)
        for mentioned in &interaction.entities.mentioned_users {
            if mentioned != &user_id {
                *interaction_counts.entry(mentioned.clone()).or_insert(0) += 1;
            }
        }
    }

    // Build collaborator list
    let mut collaborators: Vec<serde_json::Value> = interaction_counts
        .iter()
        .map(|(user, count)| {
            serde_json::json!({
                "user_id": user,
                "interaction_count": count
            })
        })
        .collect();
    collaborators.sort_by(|a, b| {
        b["interaction_count"].as_u64().cmp(&a["interaction_count"].as_u64())
    });

    let avg_sentiment = if sentiment_count > 0 {
        total_sentiment / sentiment_count as f32
    } else {
        0.0
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "user_id": user_id,
            "organization_id": query.organization_id,
            "period": {
                "days": days,
                "from": from_date.to_rfc3339(),
                "to": now.to_rfc3339()
            },
            "summary": {
                "total_interactions": user_interactions.len(),
                "average_sentiment": (avg_sentiment * 100.0).round() / 100.0,
                "unique_collaborators": interaction_counts.len()
            },
            "interaction_types": interaction_types,
            "top_collaborators": collaborators.into_iter().take(10).collect::<Vec<_>>()
        })),
    )
}

/// Get user sentiment time series
/// GET /api/analytics/user/:user_id/sentiment?org=&from_date=&to_date=
pub async fn get_user_sentiment(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<UserSentimentQuery>,
) -> impl IntoResponse {
    // Validate inputs
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e;
    }
    if user_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "user_id is required" })),
        );
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    // Parse date range (default to last 30 days)
    let now = Utc::now();
    let from_date = query.from_date
        .as_ref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
        .unwrap_or_else(|| now - chrono::Duration::days(30));
    let to_date = query.to_date
        .as_ref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .map(|d| d.and_hms_opt(23, 59, 59).unwrap().and_utc())
        .unwrap_or(now);

    // Get this user's classifications directly (pushes sender_id filter to SQL)
    let user_interactions = match analytics_db.get_classifications_for_user_in_range(&query.organization_id, &user_id, &from_date, &to_date) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get classifications: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve sentiment data" })),
            );
        }
    };

    // Group by date for time series
    let mut daily_sentiment: std::collections::BTreeMap<String, (f32, u32)> = std::collections::BTreeMap::new();

    for interaction in user_interactions.iter() {
        let date_key = interaction.original_created_at.format("%Y-%m-%d").to_string();
        let entry = daily_sentiment.entry(date_key).or_insert((0.0, 0));
        entry.0 += interaction.sentiment;
        entry.1 += 1;
    }

    // Convert to time series array
    let time_series: Vec<serde_json::Value> = daily_sentiment
        .iter()
        .map(|(date, (total, count))| {
            let avg = if *count > 0 { total / *count as f32 } else { 0.0 };
            serde_json::json!({
                "date": date,
                "average_sentiment": (avg * 100.0).round() / 100.0,
                "interaction_count": count
            })
        })
        .collect();

    // Calculate overall stats
    let total_interactions = user_interactions.len();
    let overall_sentiment: f32 = user_interactions.iter().map(|c| c.sentiment).sum();
    let avg_sentiment = if total_interactions > 0 {
        overall_sentiment / total_interactions as f32
    } else {
        0.0
    };

    // Count positive/negative/neutral
    let positive = user_interactions.iter().filter(|c| c.sentiment > 0.2).count();
    let negative = user_interactions.iter().filter(|c| c.sentiment < -0.2).count();
    let neutral = total_interactions - positive - negative;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "user_id": user_id,
            "organization_id": query.organization_id,
            "period": {
                "from": from_date.to_rfc3339(),
                "to": to_date.to_rfc3339()
            },
            "summary": {
                "total_interactions": total_interactions,
                "average_sentiment": (avg_sentiment * 100.0).round() / 100.0,
                "positive_count": positive,
                "neutral_count": neutral,
                "negative_count": negative
            },
            "time_series": time_series
        })),
    )
}

// ==================== Batch Processing ====================

/// Request to batch process all comments
#[derive(Debug, Deserialize)]
pub struct BatchCommentsRequest {
    pub organization_id: String,
}

/// Process all existing task comments to build sentiment analytics
/// POST /api/analytics/batch/comments
pub async fn batch_process_comments(
    State(state): State<AppState>,
    Json(request): Json<BatchCommentsRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e;
    }

    let analytics_db = match get_analytics_db(&state) {
        Ok(db) => db,
        Err(e) => return e,
    };

    #[cfg(feature = "postgres")]
    {
        let pool = match state.pg_pool() {
            Some(p) => p,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({ "error": "PostgreSQL not available" })),
                );
            }
        };

        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to get PG connection: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Database connection failed" })),
                );
            }
        };

        // Parse organization_id as UUID
        let org_uuid = match uuid::Uuid::parse_str(&request.organization_id) {
            Ok(u) => u,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Invalid organization_id UUID" })),
                );
            }
        };

        // Process task_comments in paginated batches of 500 to avoid OOM on large orgs
        const BATCH_SIZE: i64 = 500;
        let mut total_comments: i64 = 0;
        let mut processed: i64 = 0;
        let mut errors: i64 = 0;
        let mut offset: i64 = 0;

        loop {
            let batch_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&org_uuid, &BATCH_SIZE, &offset];
            let comments = match client
                .query(
                    r#"
                    SELECT
                        id,
                        task_id,
                        author_id,
                        content,
                        created_at
                    FROM api.task_comments
                    WHERE organization_id = $1
                    ORDER BY created_at
                    LIMIT $2 OFFSET $3
                    "#,
                    &batch_params,
                )
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("Failed to query task_comments (offset {}): {}", offset, e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": "Failed to query comments" })),
                    );
                }
            };

            let batch_len = comments.len() as i64;
            if batch_len == 0 {
                break;
            }
            total_comments += batch_len;

            for row in comments {
                let comment_id: uuid::Uuid = row.get("id");
                let task_id: uuid::Uuid = row.get("task_id");
                let author_id: uuid::Uuid = row.get("author_id");
                let content: String = row.get("content");
                let created_at: DateTime<Utc> = row.get("created_at");

                // Analyze sentiment (prefer embedding-based, fall back to keyword)
                let sentiment = match state.entity_embedding_store() {
                    Some(store) => store.compute_sentiment(&content).await
                        .unwrap_or_else(|_| analyze_comment_sentiment(&content)),
                    None => analyze_comment_sentiment(&content),
                };

                // Classify interaction type
                let interaction_type = classify_comment_type(&content);

                let classification = crate::analytics::InteractionClassification {
                    id: uuid::Uuid::new_v4(),
                    organization_id: request.organization_id.clone(),
                    source_type: crate::analytics::InteractionSource::TaskComment,
                    source_id: comment_id.to_string(),
                    task_id: Some(task_id.to_string()),
                    goal_id: None,
                    sender_id: author_id.to_string(),
                    content: content.clone(),
                    interaction_type,
                    secondary_types: Vec::new(),
                    confidence_score: 0.8,
                    entities: crate::analytics::ExtractedEntities::default(),
                    sentiment,
                    urgency_level: detect_comment_urgency(&content),
                    references_interaction_id: None,
                    original_created_at: created_at,
                    classified_at: Utc::now(),
                };

                match analytics_db.insert_classification(&classification) {
                    Ok(_) => processed += 1,
                    Err(e) => {
                        tracing::warn!("Failed to insert classification for comment {}: {}", comment_id, e);
                        errors += 1;
                    }
                }
            }

            tracing::info!(
                "Batch progress: processed {} comments so far for org {} (offset {})",
                total_comments, request.organization_id, offset
            );

            offset += BATCH_SIZE;

            // If we got fewer rows than the batch size, we've reached the end
            if batch_len < BATCH_SIZE {
                break;
            }
        }

        tracing::info!(
            "Batch completed: {} comments for org {} ({} processed, {} errors)",
            total_comments, request.organization_id, processed, errors
        );

        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "completed",
                "organization_id": request.organization_id,
                "total_comments": total_comments,
                "processed": processed,
                "errors": errors
            })),
        )
    }

    #[cfg(not(feature = "postgres"))]
    {
        let _ = analytics_db;
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "PostgreSQL feature not enabled" })),
        )
    }
}

/// Analyze sentiment from comment text
fn analyze_comment_sentiment(content: &str) -> f32 {
    let text = content.to_lowercase();

    let positive_words = [
        "great", "thanks", "thank", "good", "excellent", "awesome", "perfect",
        "wonderful", "amazing", "helpful", "appreciate", "well done", "nice",
        "love", "happy", "pleased", "fantastic", "brilliant", "superb", "agreed",
    ];

    let negative_words = [
        "problem", "issue", "bug", "error", "broken", "fail", "wrong",
        "bad", "terrible", "awful", "frustrated", "annoyed", "disappointed",
        "stuck", "blocked", "confused", "difficult", "impossible", "hate", "urgent",
    ];

    let mut score: f32 = 0.0;

    for word in &positive_words {
        if text.contains(word) {
            score += 0.25;
        }
    }

    for word in &negative_words {
        if text.contains(word) {
            score -= 0.25;
        }
    }

    score.clamp(-1.0, 1.0)
}

/// Classify comment interaction type
fn classify_comment_type(content: &str) -> crate::analytics::InteractionType {
    let lower = content.to_lowercase();

    if lower.contains('?') || lower.contains("how") || lower.contains("what") || lower.contains("when") || lower.contains("why") {
        crate::analytics::InteractionType::Question
    } else if lower.contains("blocked") || lower.contains("stuck") || lower.contains("cannot") || lower.contains("can't") {
        crate::analytics::InteractionType::Blocker
    } else if lower.contains("done") || lower.contains("completed") || lower.contains("finished") || lower.contains("updated") {
        crate::analytics::InteractionType::StatusUpdate
    } else if lower.contains("thanks") || lower.contains("great") || lower.contains("good job") || lower.contains("well done") {
        crate::analytics::InteractionType::Feedback
    } else if lower.contains("assign") || lower.contains("take over") || lower.contains("handle") {
        crate::analytics::InteractionType::Assignment
    } else if lower.contains("approve") || lower.contains("lgtm") || lower.contains("ship it") {
        crate::analytics::InteractionType::Acknowledgment
    } else {
        crate::analytics::InteractionType::Other
    }
}

/// Detect urgency from comment content
fn detect_comment_urgency(content: &str) -> crate::analytics::UrgencyLevel {
    let lower = content.to_lowercase();

    if lower.contains("urgent") || lower.contains("asap") || lower.contains("critical") || lower.contains("emergency") {
        crate::analytics::UrgencyLevel::Critical
    } else if lower.contains("important") || lower.contains("priority") || lower.contains("soon") {
        crate::analytics::UrgencyLevel::High
    } else {
        crate::analytics::UrgencyLevel::Medium
    }
}

// ==================== Helpers ====================

/// Get the analytics database from app state (pre-initialized at startup)
///
/// Falls back to opening a new connection if state doesn't have one,
/// but this should rarely happen since analytics_db is initialized in AppState::new.
fn get_analytics_db(state: &AppState) -> Result<Arc<AnalyticsDb>, (StatusCode, Json<serde_json::Value>)> {
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
                Json(serde_json::json!({
                    "error": "Analytics database not available"
                })),
            ))
        }
    }
}

// ==================== Entity Embedding Endpoints ====================

/// Request for semantic search across entity embeddings
#[derive(Debug, Deserialize)]
pub struct EmbeddingSearchRequest {
    pub organization_id: String,
    pub query: String,
    #[serde(default)]
    pub entity_type: Option<String>,
    #[serde(default = "default_embedding_top_k")]
    pub top_k: usize,
}

fn default_embedding_top_k() -> usize { 10 }

/// Request for backfilling entity embeddings
#[derive(Debug, Deserialize)]
pub struct BackfillRequest {
    pub organization_id: String,
    pub entity_types: Vec<String>,
    #[serde(default = "default_backfill_batch")]
    pub batch_size: usize,
}

fn default_backfill_batch() -> usize { 50 }

/// Request for finding similar workflow patterns
#[derive(Debug, Deserialize)]
pub struct PatternSearchRequest {
    pub organization_id: String,
    pub entity_type: String,
    pub entity_id: String,
    #[serde(default)]
    pub search_type: Option<String>,
    #[serde(default = "default_embedding_top_k")]
    pub top_k: usize,
}

/// POST /api/analytics/embeddings/search
///
/// Semantic search across entity embeddings (tasks, goals, comments, messages)
#[cfg(feature = "postgres")]
pub async fn search_entity_embeddings(
    State(state): State<AppState>,
    Json(request): Json<EmbeddingSearchRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e.into_response();
    }

    let store = match state.entity_embedding_store() {
        Some(s) => s,
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Entity embedding store not available" })),
        ).into_response(),
    };

    match store.search_similar(
        &request.query,
        &request.organization_id,
        request.entity_type.as_deref(),
        request.top_k,
    ).await {
        Ok(results) => (StatusCode::OK, Json(serde_json::json!({
            "results": results,
            "count": results.len(),
            "query": request.query,
        }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Search failed: {}", e) })),
        ).into_response(),
    }
}

/// POST /api/analytics/embeddings/backfill
///
/// Backfill entity embeddings from source PostgreSQL tables
#[cfg(feature = "postgres")]
pub async fn backfill_entity_embeddings(
    State(state): State<AppState>,
    Json(request): Json<BackfillRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e.into_response();
    }

    let store = match state.entity_embedding_store() {
        Some(s) => Arc::clone(s),
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Entity embedding store not available" })),
        ).into_response(),
    };

    let valid_types = ["task", "goal", "task_comment", "message", "chat_message"];
    for et in &request.entity_types {
        if !valid_types.contains(&et.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid entity type: '{}'. Valid: {:?}", et, valid_types)
                })),
            ).into_response();
        }
    }

    let org_id = request.organization_id.clone();
    let entity_types = request.entity_types.clone();
    let batch_size = request.batch_size;

    // Run backfill in background
    tokio::spawn(async move {
        for entity_type in &entity_types {
            tracing::info!(org_id = %org_id, entity_type = %entity_type, "Starting entity embedding backfill");
            match store.backfill(&org_id, entity_type, batch_size).await {
                Ok(result) => {
                    tracing::info!(
                        org_id = %org_id,
                        entity_type = %entity_type,
                        total = result.total_found,
                        embedded = result.embedded,
                        errors = result.errors,
                        "Backfill complete"
                    );
                }
                Err(e) => {
                    tracing::error!(org_id = %org_id, entity_type = %entity_type, "Backfill failed: {}", e);
                }
            }
        }
    });

    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "status": "processing",
        "message": format!("Backfill started for {} entity types", request.entity_types.len()),
        "entity_types": request.entity_types,
        "organization_id": request.organization_id,
    }))).into_response()
}

/// GET /api/analytics/embeddings/stats
///
/// Get entity embedding statistics by type
#[cfg(feature = "postgres")]
pub async fn get_embedding_stats(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&query.organization_id) {
        return e.into_response();
    }

    let store = match state.entity_embedding_store() {
        Some(s) => s,
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Entity embedding store not available" })),
        ).into_response(),
    };

    match store.get_stats(&query.organization_id).await {
        Ok(stats) => (StatusCode::OK, Json(serde_json::json!(stats))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to get stats: {}", e) })),
        ).into_response(),
    }
}

/// POST /api/analytics/embeddings/backfill-sentiment
///
/// Backfill sentiment scores for entity embeddings that have content but NULL sentiment.
/// Uses embedding-based sentiment (cosine similarity to cached anchor embeddings).
#[cfg(feature = "postgres")]
pub async fn backfill_entity_sentiment(
    State(state): State<AppState>,
    Json(request): Json<BackfillRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e.into_response();
    }

    let store = match state.entity_embedding_store() {
        Some(s) => Arc::clone(s),
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Entity embedding store not available" })),
        ).into_response(),
    };

    let org_id = request.organization_id.clone();
    let entity_types = request.entity_types.clone();
    let batch_size = request.batch_size;

    tokio::spawn(async move {
        for entity_type in &entity_types {
            tracing::info!(org_id = %org_id, entity_type = %entity_type, "Starting sentiment backfill");

            let client = match store.pool_client().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to get DB client for sentiment backfill: {}", e);
                    continue;
                }
            };

            // Get entities with content but NULL sentiment
            let rows = match client.query(
                "SELECT entity_id, content FROM entity_embeddings \
                 WHERE organization_id = $1 AND entity_type = $2 \
                 AND sentiment IS NULL AND content != '' \
                 ORDER BY embedded_at DESC NULLS LAST",
                &[&org_id.as_str(), &entity_type.as_str()],
            ).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(entity_type = %entity_type, "Failed to query entities for sentiment backfill: {:?}", e);
                    continue;
                }
            };

            let total = rows.len();
            tracing::info!(org_id = %org_id, entity_type = %entity_type, total, "Found entities needing sentiment");

            let mut updated = 0u32;
            let mut errors = 0u32;

            for (i, row) in rows.iter().enumerate() {
                let entity_id: uuid::Uuid = row.get("entity_id");
                let content: String = row.get("content");

                match store.compute_sentiment(&content).await {
                    Ok(score) => {
                        if let Err(e) = client.execute(
                            "UPDATE entity_embeddings SET sentiment = $1 \
                             WHERE entity_id = $2 AND entity_type = $3 AND organization_id = $4",
                            &[&score, &entity_id, &entity_type.as_str(), &org_id.as_str()],
                        ).await {
                            tracing::warn!(entity_id = %entity_id, "Failed to update sentiment: {}", e);
                            errors += 1;
                        } else {
                            updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(entity_id = %entity_id, "Failed to compute sentiment: {}", e);
                        errors += 1;
                    }
                }

                if (i + 1) % batch_size == 0 {
                    tracing::info!(
                        org_id = %org_id, entity_type = %entity_type,
                        progress = format!("{}/{}", i + 1, total),
                        updated, errors,
                        "Sentiment backfill progress"
                    );
                }
            }

            tracing::info!(
                org_id = %org_id, entity_type = %entity_type,
                total, updated, errors,
                "Sentiment backfill complete"
            );
        }
    });

    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "status": "processing",
        "message": format!("Sentiment backfill started for {} entity types", request.entity_types.len()),
        "entity_types": request.entity_types,
        "organization_id": request.organization_id,
    }))).into_response()
}

/// POST /api/analytics/embeddings/patterns
///
/// Find similar completed entities to detect workflow patterns
#[cfg(feature = "postgres")]
pub async fn find_entity_patterns(
    State(state): State<AppState>,
    Json(request): Json<PatternSearchRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_org_id(&request.organization_id) {
        return e.into_response();
    }

    let entity_id = match Uuid::parse_str(&request.entity_id) {
        Ok(id) => id,
        Err(_) => return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid entity_id UUID" })),
        ).into_response(),
    };

    let store = match state.entity_embedding_store() {
        Some(s) => s,
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Entity embedding store not available" })),
        ).into_response(),
    };

    match store.search_similar_to_entity(
        &request.entity_type,
        &entity_id,
        &request.organization_id,
        request.search_type.as_deref(),
        request.top_k,
    ).await {
        Ok(results) => (StatusCode::OK, Json(serde_json::json!({
            "reference": {
                "entity_type": request.entity_type,
                "entity_id": request.entity_id,
            },
            "similar_entities": results,
            "count": results.len(),
        }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Pattern search failed: {}", e) })),
        ).into_response(),
    }
}
