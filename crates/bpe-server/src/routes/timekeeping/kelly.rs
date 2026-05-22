use axum::{extract::{Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{kelly::KellySchedule, models::*},
};
use super::stations::OrgQuery;
use crate::AppState;

pub async fn get_config(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "roster", "read").await?;
    let config = KellySchedule::get_config(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": config })))
}

pub async fn upsert_config(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<UpsertKellyConfigRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let config = KellySchedule::upsert_config(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": config })))
}

pub async fn compute_schedule(
    State(state): State<AppState>,
    Query(query): Query<KellyScheduleQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    let config = KellySchedule::get_config(state.pool(), org_id).await?;
    let schedule = KellySchedule::compute_range(&config, query.start, query.end);
    Ok(Json(serde_json::json!({ "data": schedule, "config": config })))
}
