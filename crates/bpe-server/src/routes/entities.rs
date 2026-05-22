use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bpe_core::{
    permissions::require_feature_access,
    audit::logger::AuditLogger,
    auth::AuthClaims,
    entity::{
        interactions::InteractionTracker,
        models::*,
        registry::EntityTypeRegistry,
        relationships::RelationshipManager,
        EntityManager,
    },
    error::BpeError,
    middleware::verify_org_access,
    validation::{validate_name, validate_org_slug},
};
use uuid::Uuid;

use crate::AppState;

/// GET /bpe/api/entities?organization_id=slug&entity_type=employee&status=active&page=1&per_page=50
pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ListEntitiesQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "marshal", "read").await?;

    // Note: seed_system_types should be called during org creation, not on every list request.
    let result = EntityManager::list(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!(result)))
}

/// GET /bpe/api/entities/:id
pub async fn get(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let entity = EntityManager::get(state.pool(), id).await?;
    verify_org_access(&claims, entity.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, entity.organization_id, claims.is_platform_admin, "marshal", "read").await?;
    let relationships = RelationshipManager::list_for_entity(state.pool(), id).await?;

    Ok(Json(serde_json::json!({
        "data": entity,
        "relationships": relationships
    })))
}

/// POST /bpe/api/entities
pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateEntityRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("display_name", &req.display_name)?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;

    let entity = EntityManager::create(state.pool(), org_id, user_id, &req).await?;

    // Audit log
    let after = serde_json::to_value(&entity).ok();
    if let Err(e) = AuditLogger::log_change(
        state.pool(), org_id, "entity.created", "entity", entity.id,
        Some(user_id), None, after.as_ref(), serde_json::json!({}),
    ).await {
        tracing::warn!("Audit log failed for entity.created: {e}");
    }

    Ok(Json(serde_json::json!({ "data": entity })))
}

/// PUT /bpe/api/entities/:id
pub async fn update(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEntityRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let before_entity = EntityManager::get(state.pool(), id).await?;
    verify_org_access(&claims, before_entity.organization_id)?;
    let before = serde_json::to_value(&before_entity).ok();

    require_feature_access(state.pool(), state.permissions(), &claims.user_id, before_entity.organization_id, claims.is_platform_admin, "marshal", "write").await?;
    let entity = EntityManager::update(state.pool(), id, &req).await?;
    let after = serde_json::to_value(&entity).ok();

    let user_id = claims.user_id.parse::<Uuid>().ok();
    if let Err(e) = AuditLogger::log_change(
        state.pool(), before_entity.organization_id, "entity.updated", "entity", id,
        user_id, before.as_ref(), after.as_ref(), serde_json::json!({}),
    ).await {
        tracing::warn!("Audit log failed for entity.updated: {e}");
    }

    Ok(Json(serde_json::json!({ "data": entity })))
}

/// DELETE /bpe/api/entities/:id (soft delete → archived)
pub async fn delete(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let before_entity = EntityManager::get(state.pool(), id).await?;
    verify_org_access(&claims, before_entity.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, before_entity.organization_id, claims.is_platform_admin, "marshal", "delete").await?;
    let before = serde_json::to_value(&before_entity).ok();

    EntityManager::delete(state.pool(), id).await?;

    let user_id = claims.user_id.parse::<Uuid>().ok();
    if let Err(e) = AuditLogger::log_change(
        state.pool(), before_entity.organization_id, "entity.archived", "entity", id,
        user_id, before.as_ref(), None, serde_json::json!({}),
    ).await {
        tracing::warn!("Audit log failed for entity.archived: {e}");
    }

    Ok(Json(serde_json::json!({ "status": "archived" })))
}

/// GET /bpe/api/entities/:id/relationships
pub async fn list_relationships(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let entity = EntityManager::get(state.pool(), id).await?;
    verify_org_access(&claims, entity.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, entity.organization_id, claims.is_platform_admin, "marshal", "read").await?;
    let relationships = RelationshipManager::list_for_entity(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "data": relationships })))
}

/// POST /bpe/api/entities/:id/relationships
pub async fn add_relationship(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateRelationshipRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_name("relationship_type", &req.relationship_type)?;

    let entity = EntityManager::get(state.pool(), id).await?;
    verify_org_access(&claims, entity.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, entity.organization_id, claims.is_platform_admin, "marshal", "write").await?;
    let rel = RelationshipManager::add(state.pool(), entity.organization_id, id, &req).await?;
    Ok(Json(serde_json::json!({ "data": rel })))
}

/// DELETE /bpe/api/entity-relationships/:id
pub async fn remove_relationship(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    // Look up the relationship's org via its source entity
    let client = state.pool().get().await?;
    let row = client.query_opt(
        "SELECT e.organization_id FROM bpe.entity_relationships r JOIN bpe.entities e ON e.id = r.source_entity_id WHERE r.id = $1",
        &[&id],
    ).await?.ok_or_else(|| BpeError::NotFound(format!("Relationship {id} not found")))?;
    let org_id: Uuid = row.get(0);
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "marshal", "delete").await?;
    RelationshipManager::remove(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "removed" })))
}

/// GET /bpe/api/entities/:id/interactions?page=1&per_page=20
pub async fn list_interactions(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Query(pag): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let entity = EntityManager::get(state.pool(), id).await?;
    verify_org_access(&claims, entity.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, entity.organization_id, claims.is_platform_admin, "marshal", "read").await?;
    let page = pag.page.unwrap_or(1).max(1);
    let per_page = pag.per_page.unwrap_or(20).min(100);
    let result = InteractionTracker::list(state.pool(), id, page, per_page).await?;
    Ok(Json(serde_json::json!(result)))
}

/// POST /bpe/api/entities/:id/interactions
pub async fn add_interaction(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateInteractionRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_name("interaction_type", &req.interaction_type)?;
    validate_name("summary", &req.summary)?;

    let entity = EntityManager::get(state.pool(), id).await?;
    verify_org_access(&claims, entity.organization_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, entity.organization_id, claims.is_platform_admin, "marshal", "write").await?;
    let user_id = claims.user_id.parse::<Uuid>().ok();
    let interaction = InteractionTracker::record(state.pool(), entity.organization_id, id, user_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": interaction })))
}

#[derive(serde::Deserialize)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}
