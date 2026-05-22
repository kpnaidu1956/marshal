use axum::{extract::{Path, Query, State}, Extension, Json};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    timekeeping::{validation::CrossValidator, models::*},
};
use uuid::Uuid;
use crate::AppState;

pub async fn validate(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "admin").await?;
    let result = CrossValidator::validate(state.pool(), org_id, req.start, req.end).await?;
    Ok(Json(result))
}

pub async fn list_flags(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<FlagsQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "timekeeping", "read").await?;
    let flags = CrossValidator::list_flags(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!({ "data": flags })))
}

pub async fn resolve_flag(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<ResolveFlagRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id".into()))?;
    CrossValidator::resolve_flag(state.pool(), id, user_id, req.resolution_note.as_deref()).await?;
    Ok(Json(serde_json::json!({ "status": "resolved" })))
}
