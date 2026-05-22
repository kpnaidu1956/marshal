use axum::{extract::{Query, State}, Extension, Json};
use bpe_core::{
    auth::AuthClaims,
    permissions::require_feature_access,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    timekeeping::audit_trail::{AuditQueryParams, TimekeepingAudit},
};
use crate::AppState;

pub async fn list_audit_trail(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<AuditQueryParams>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "audit", "read").await?;
    let result = TimekeepingAudit::query(state.pool(), org_id, &query).await?;
    Ok(Json(result))
}

pub async fn audit_summary(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<AuditQueryParams>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "audit", "read").await?;
    let result = TimekeepingAudit::summary(state.pool(), org_id, query.start, query.end).await?;
    Ok(Json(result))
}
