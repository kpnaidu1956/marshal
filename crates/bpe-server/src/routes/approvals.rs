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
    validation::{validate_name, validate_description, validate_org_slug},
    approval::{
        engine::ApprovalEngine,
        models::*,
    },
};
use uuid::Uuid;

use crate::AppState;

// ---- Rules ----

/// GET /bpe/api/approvals/rules?organization_id=slug&page=1&per_page=50
pub async fn list_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "approvals", "read").await?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(200);
    let result = ApprovalEngine::list_rules(state.pool(), org_id, page, per_page).await?;
    Ok(Json(serde_json::json!(result)))
}

/// POST /bpe/api/approvals/rules
pub async fn create_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("name", &req.name)?;
    validate_description(req.description.as_deref())?;
    if req.approver_user_ids.is_empty() {
        return Err(BpeError::BadRequest("approver_user_ids cannot be empty".into()));
    }
    if req.approver_user_ids.len() > 50 {
        return Err(BpeError::BadRequest("approver_user_ids cannot exceed 50 entries".into()));
    }

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "approvals", "write").await?;
    let rule = ApprovalEngine::create_rule(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": rule })))
}

/// GET /bpe/api/approvals/rules/:id
pub async fn get_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let rule = ApprovalEngine::get_rule(state.pool(), id).await?;
    verify_org_access(&claims, rule.organization_id)?;
    Ok(Json(serde_json::json!({ "data": rule })))
}

/// PUT /bpe/api/approvals/rules/:id
pub async fn update_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    if let Some(ref name) = req.name {
        validate_name("name", name)?;
    }
    if let Some(ref desc) = req.description {
        validate_description(Some(desc.as_str()))?;
    }
    if let Some(ref ids) = req.approver_user_ids {
        if ids.is_empty() {
            return Err(BpeError::BadRequest("approver_user_ids cannot be empty".into()));
        }
        if ids.len() > 50 {
            return Err(BpeError::BadRequest("approver_user_ids cannot exceed 50 entries".into()));
        }
    }
    let existing = ApprovalEngine::get_rule(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    let rule = ApprovalEngine::update_rule(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": rule })))
}

/// DELETE /bpe/api/approvals/rules/:id
pub async fn delete_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = ApprovalEngine::get_rule(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    ApprovalEngine::delete_rule(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

// ---- Requests ----

/// GET /bpe/api/approvals/requests?organization_id=slug&status=pending
pub async fn list_requests(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ListRequestsQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "approvals", "read").await?;
    let result = ApprovalEngine::list_requests(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!(result)))
}

/// POST /bpe/api/approvals/requests
pub async fn create_request(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateRequestPayload>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("title", &req.title)?;
    validate_description(req.description.as_deref())?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let request = ApprovalEngine::create_request(state.pool(), org_id, user_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": request })))
}

/// GET /bpe/api/approvals/requests/:id
pub async fn get_request(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let request = ApprovalEngine::get_request(state.pool(), id).await?;
    verify_org_access(&claims, request.organization_id)?;
    let decisions = ApprovalEngine::get_decisions(state.pool(), id).await?;
    Ok(Json(serde_json::json!({
        "data": request,
        "decisions": decisions
    })))
}

/// POST /bpe/api/approvals/requests/:id/cancel
pub async fn cancel_request(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let request = ApprovalEngine::cancel_request(state.pool(), id, user_id).await?;
    Ok(Json(serde_json::json!({ "data": request })))
}

/// GET /bpe/api/approvals/pending?organization_id=slug
pub async fn pending_for_me(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<PendingQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let requests = ApprovalEngine::pending_for_user(state.pool(), org_id, user_id).await?;
    Ok(Json(serde_json::json!({ "data": requests })))
}

// ---- Decisions ----

/// POST /bpe/api/approvals/requests/:id/decide
pub async fn decide(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<DecisionPayload>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let valid_decisions = ["approved", "rejected", "request_changes"];
    if !valid_decisions.contains(&payload.decision.as_str()) {
        return Err(BpeError::BadRequest(format!(
            "decision must be one of: {}", valid_decisions.join(", ")
        )));
    }

    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let decision = ApprovalEngine::decide(state.pool(), id, user_id, &payload).await?;
    Ok(Json(serde_json::json!({ "data": decision })))
}

/// GET /bpe/api/approvals/requests/:id/decisions
pub async fn list_decisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let decisions = ApprovalEngine::get_decisions(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "data": decisions })))
}
