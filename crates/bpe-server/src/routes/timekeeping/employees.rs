use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    permissions::require_feature_access,
    timekeeping::{employees::EmployeeManager, models::*},
};
use uuid::Uuid;
use crate::AppState;

pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ListEmployeesQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let result = EmployeeManager::list(state.pool(), org_id, &query).await?;
    Ok(Json(result))
}

pub async fn get_one(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let emp = EmployeeManager::get(state.pool(), id).await?;
    verify_org_access(&claims, emp.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, emp.organization_id, claims.is_platform_admin, "timekeeping", "read").await?;
    Ok(Json(serde_json::json!({ "data": emp })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateEmployeeRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "write").await?;
    let emp = EmployeeManager::create(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": emp })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEmployeeRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = EmployeeManager::get(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, existing.organization_id, claims.is_platform_admin, "timekeeping", "write").await?;
    let emp = EmployeeManager::update(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": emp })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = EmployeeManager::get(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, existing.organization_id, claims.is_platform_admin, "timekeeping", "delete").await?;
    EmployeeManager::delete(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deactivated" })))
}

pub async fn import(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<ImportEmployeesRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "admin").await?;
    let result = EmployeeManager::import(state.pool(), org_id, &req.employees).await?;
    Ok(Json(result))
}
