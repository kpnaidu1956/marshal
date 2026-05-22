use axum::{extract::{Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{leave_balances::LeaveBalanceManager, models::*},
};
use crate::AppState;

pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<LeaveBalanceQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let balances = LeaveBalanceManager::list(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!({ "data": balances })))
}

pub async fn adjust(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<AdjustLeaveBalanceRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let balance = LeaveBalanceManager::adjust(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": balance })))
}
