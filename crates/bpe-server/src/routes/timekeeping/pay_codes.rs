use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{pay_codes::PayCodeManager, models::*},
};
use uuid::Uuid;
use crate::AppState;
use super::stations::OrgQuery;

pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let codes = PayCodeManager::list(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": codes })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreatePayCodeRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let code = PayCodeManager::create(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": code })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePayCodeRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let code = PayCodeManager::update(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": code })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    PayCodeManager::delete(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deactivated" })))
}
