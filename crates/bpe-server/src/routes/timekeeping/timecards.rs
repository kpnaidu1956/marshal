use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{timecards::TimecardManager, models::*},
};
use uuid::Uuid;
use crate::AppState;
use super::stations::OrgQuery;

pub async fn list_periods(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let periods = TimecardManager::list_periods(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": periods })))
}

pub async fn create_period(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreatePeriodRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let period = TimecardManager::create_period(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": period })))
}

pub async fn close_period(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    TimecardManager::close_period(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "closed" })))
}

pub async fn certify(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CertifyTimecardRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id".into()))?;
    TimecardManager::certify(state.pool(), org_id, user_id, &req).await?;
    Ok(Json(serde_json::json!({ "status": "certified" })))
}

pub async fn pending_approvals(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "approvals", "read").await?;
    let pending = TimecardManager::list_pending_approvals(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": pending })))
}

pub async fn decide(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<TimecardDecisionRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let supervisor_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id".into()))?;
    TimecardManager::decide(state.pool(), org_id, supervisor_id, &req).await?;
    Ok(Json(serde_json::json!({ "status": req.decision })))
}
