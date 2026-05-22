use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{roster::RosterManager, models::*},
};
use uuid::Uuid;
use crate::AppState;

pub async fn get_roster(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<RosterQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "roster", "read").await?;
    let roster = RosterManager::get_for_date(state.pool(), org_id, query.date).await?;
    match roster {
        Some(r) => Ok(Json(serde_json::json!({ "data": r }))),
        None => Ok(Json(serde_json::json!({ "data": null, "message": "No roster for this date" }))),
    }
}

pub async fn get_range(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<RosterRangeQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "roster", "read").await?;
    let rosters = RosterManager::get_range(state.pool(), org_id, query.start, query.end).await?;
    Ok(Json(serde_json::json!({ "data": rosters })))
}

pub async fn generate(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<GenerateRosterRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let result = RosterManager::generate(state.pool(), org_id, req.start, req.end).await?;
    Ok(Json(result))
}

pub async fn update_roster(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRosterRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    RosterManager::update(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "status": "updated" })))
}

pub async fn lock(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    RosterManager::lock(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "locked" })))
}

pub async fn unlock(
    State(state): State<AppState>,
    Extension(_claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    RosterManager::unlock(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "unlocked" })))
}

pub async fn get_assignments(
    State(state): State<AppState>,
    Query(query): Query<RosterQuery>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    // Re-fetch the full roster for this date
    let roster = RosterManager::get_for_date(state.pool(), org_id, query.date).await?;
    match roster {
        Some(r) => Ok(Json(serde_json::json!({ "data": r.assignments }))),
        None => Ok(Json(serde_json::json!({ "data": [], "roster_id": id.to_string() }))),
    }
}

pub async fn update_assignments(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAssignmentsRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &claims.organization_id).await?;
    let assignments = RosterManager::update_assignments(state.pool(), org_id, id, &req).await?;
    Ok(Json(serde_json::json!({ "data": assignments })))
}
