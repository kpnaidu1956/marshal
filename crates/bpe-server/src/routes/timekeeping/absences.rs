use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{absences::AbsenceManager, models::*},
};
use uuid::Uuid;
use crate::AppState;

pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<AbsenceQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let absences = AbsenceManager::list(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!({ "data": absences })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateAbsenceRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let absence = AbsenceManager::create(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": absence })))
}

pub async fn approve(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let approver_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id".into()))?;
    let approve = body.get("approve").and_then(|v| v.as_bool()).unwrap_or(true);
    AbsenceManager::approve(state.pool(), id, approver_id, approve).await?;
    let status = if approve { "approved" } else { "denied" };
    Ok(Json(serde_json::json!({ "status": status })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    AbsenceManager::delete(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
