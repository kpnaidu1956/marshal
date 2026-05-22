use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bpe_core::{
    permissions::require_feature_access,
    audit::{
        logger::AuditLogger,
        models::{AuditQueryParams, NewAuditEvent, ReversalRequest},
        query::AuditQuery,
        reversal::ReversalEngine,
    },
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    validation::{validate_name, validate_org_slug},
};
use uuid::Uuid;

use crate::AppState;

/// GET /bpe/api/audit/events?organization_id=slug&resource_type=...&event_type=...&from=...&to=...&page=1&per_page=50
pub async fn list_events(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &params.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "audit", "read").await?;
    let result = AuditQuery::search(state.pool(), org_id, &params).await?;
    Ok(Json(serde_json::json!(result)))
}

/// GET /bpe/api/audit/entity/:entity_id
pub async fn entity_events(
    State(state): State<AppState>,
    Path(entity_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let events = AuditQuery::by_entity(state.pool(), entity_id).await?;
    Ok(Json(serde_json::json!({ "data": events })))
}

/// GET /bpe/api/audit/resource/:resource_type/:resource_id
pub async fn resource_events(
    State(state): State<AppState>,
    Path((resource_type, resource_id)): Path<(String, Uuid)>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let events = AuditQuery::by_resource(state.pool(), &resource_type, resource_id).await?;
    Ok(Json(serde_json::json!({ "data": events })))
}

/// POST /bpe/api/audit/reversal
pub async fn reverse_event(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<ReversalRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;

    let result = ReversalEngine::reverse(state.pool(), req.event_id, &req.reason, Some(user_id)).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

/// POST /bpe/api/audit/events (manual event logging, e.g. from integrations)
pub async fn create_event(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateEventRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("event_type", &req.event_type)?;
    validate_name("resource_type", &req.resource_type)?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    let user_id = claims.user_id.parse::<Uuid>().ok();

    let event = AuditLogger::log(state.pool(), &NewAuditEvent {
        organization_id: org_id,
        event_type: req.event_type,
        resource_type: req.resource_type,
        resource_id: req.resource_id,
        actor_user_id: user_id,
        actor_type: req.actor_type.unwrap_or_else(|| "user".into()),
        before_state: req.before_state,
        after_state: req.after_state,
        metadata: req.metadata.unwrap_or(serde_json::json!({})),
        ip_address: None,
    }).await?;

    Ok(Json(serde_json::json!({ "data": event })))
}

#[derive(serde::Deserialize)]
pub struct CreateEventRequest {
    pub organization_id: String,
    pub event_type: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub actor_type: Option<String>,
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
}
