use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    permissions::require_feature_access,
    timekeeping::{time_entries::TimeEntryManager, models::*},
};
use uuid::Uuid;
use crate::AppState;

fn parse_user_id(claims: &AuthClaims) -> Result<Uuid, BpeError> {
    claims.user_id.parse::<Uuid>().map_err(|_| BpeError::Internal("Invalid user_id".into()))
}

pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<TimeEntryQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let result = TimeEntryManager::list(state.pool(), org_id, &query).await?;
    Ok(Json(result))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateTimeEntryRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "write").await?;
    let user_id = parse_user_id(&claims)?;
    let entry = TimeEntryManager::create(state.pool(), org_id, user_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": entry })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTimeEntryRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = parse_user_id(&claims)?;
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &claims.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "write").await?;
    let entry = TimeEntryManager::update(state.pool(), id, user_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": entry })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = parse_user_id(&claims)?;
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &claims.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "write").await?;
    TimeEntryManager::delete(state.pool(), id, user_id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn submit(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = parse_user_id(&claims)?;
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &claims.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "write").await?;
    TimeEntryManager::submit(state.pool(), id, user_id).await?;
    Ok(Json(serde_json::json!({ "status": "submitted" })))
}

pub async fn batch_create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<BatchTimeEntryRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "write").await?;
    let user_id = parse_user_id(&claims)?;
    let entries = TimeEntryManager::batch_create(state.pool(), org_id, user_id, &req.entries).await?;
    Ok(Json(serde_json::json!({ "data": entries, "count": entries.len() })))
}
