use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::{
        models::{CreateEntityTypeRequest, UpdateEntityTypeRequest},
        registry::EntityTypeRegistry,
    },
    error::BpeError,
    middleware::verify_org_access,
    validation::{validate_name, validate_description, validate_org_slug},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

#[derive(Deserialize)]
pub struct OrgQuery {
    pub organization_id: String,
}

/// GET /bpe/api/entity-types?organization_id=slug
pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(q): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &q.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "read").await?;

    // Seed system types on first access
    EntityTypeRegistry::seed_system_types(state.pool(), org_id).await?;

    let types = EntityTypeRegistry::list(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": types })))
}

/// POST /bpe/api/entity-types
pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateEntityTypeRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("name", &req.name)?;
    validate_name("display_name", &req.display_name)?;
    validate_description(req.description.as_deref())?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "write").await?;
    let entity_type = EntityTypeRegistry::create(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": entity_type })))
}

/// PUT /bpe/api/entity-types/:id
pub async fn update(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEntityTypeRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = EntityTypeRegistry::get(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, existing.organization_id, claims.is_platform_admin, "admin", "write").await?;
    let entity_type = EntityTypeRegistry::update(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": entity_type })))
}

/// DELETE /bpe/api/entity-types/:id
pub async fn delete(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = EntityTypeRegistry::get(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, existing.organization_id, claims.is_platform_admin, "admin", "delete").await?;
    EntityTypeRegistry::delete(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
