use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{stations::StationManager, models::*},
};
use serde::Deserialize;
use uuid::Uuid;
use crate::AppState;

#[derive(Deserialize)]
pub struct OrgQuery { pub organization_id: String }

pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let stations = StationManager::list(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": stations })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateStationRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let station = StationManager::create(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": station })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStationRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = StationManager::get(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    let station = StationManager::update(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": station })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = StationManager::get(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    StationManager::delete(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deactivated" })))
}
