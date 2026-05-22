use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    validation::{validate_name, validate_description, validate_category, validate_org_slug},
    knowledge::{
        engine::KnowledgeEngine,
        models::*,
    },
};
use uuid::Uuid;

use crate::AppState;

/// POST /bpe/api/knowledge/learn
pub async fn learn_from_execution(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<LearnFromExecutionRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_category(&req.task_category)?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let seq = KnowledgeEngine::learn_from_execution(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": seq })))
}

/// POST /bpe/api/knowledge/learn-from-goal
pub async fn learn_from_goal(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<LearnFromGoalRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_category(&req.task_category)?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let seq = KnowledgeEngine::learn_from_goal(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": seq })))
}

/// POST /bpe/api/knowledge/suggest
pub async fn suggest(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Json(req): Json<SuggestRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    let suggestions = KnowledgeEngine::suggest(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": suggestions })))
}

/// POST /bpe/api/knowledge/sequences/:id/feedback
pub async fn feedback(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<FeedbackRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let valid_outcomes = ["accepted", "modified", "rejected"];
    if !valid_outcomes.contains(&req.outcome.as_str()) {
        return Err(BpeError::BadRequest(format!(
            "outcome must be one of: {}", valid_outcomes.join(", ")
        )));
    }

    let seq = KnowledgeEngine::record_feedback(state.pool(), id, &req.outcome).await?;
    Ok(Json(serde_json::json!({ "data": seq })))
}

/// GET /bpe/api/knowledge/sequences?organization_id=slug&task_category=hr
pub async fn list_sequences(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "knowledge", "read").await?;
    let active_only = query.active_only.unwrap_or(true);
    let seqs = KnowledgeEngine::list(state.pool(), org_id, query.task_category.as_deref(), active_only).await?;
    Ok(Json(serde_json::json!({ "data": seqs })))
}

/// GET /bpe/api/knowledge/sequences/:id
pub async fn get_sequence(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let seq = KnowledgeEngine::get(state.pool(), id).await?;
    verify_org_access(&claims, seq.organization_id)?;
    Ok(Json(serde_json::json!({ "data": seq })))
}

/// DELETE /bpe/api/knowledge/sequences/:id
pub async fn deactivate_sequence(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let seq = KnowledgeEngine::get(state.pool(), id).await?;
    verify_org_access(&claims, seq.organization_id)?;
    KnowledgeEngine::deactivate(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deactivated" })))
}

/// POST /bpe/api/knowledge/sequences/:id/promote
pub async fn promote_to_definition(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<PromoteRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("name", &req.name)?;
    validate_description(req.description.as_deref())?;
    if let Some(ref cat) = req.category {
        validate_category(cat)?;
    }

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let def = KnowledgeEngine::promote_to_definition(
        state.pool(), id, org_id, &req.name,
        req.description.as_deref(), req.category.as_deref(),
    ).await?;
    Ok(Json(serde_json::json!({ "data": def })))
}
