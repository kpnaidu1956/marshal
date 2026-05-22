use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bpe_core::{
    permissions::require_feature_access,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    validation::{validate_name, validate_org_slug},
    integration::{
        engine::IntegrationEngine,
        models::*,
    },
};
use uuid::Uuid;

use crate::AppState;

// ---- Types ----

/// GET /bpe/api/integrations/types
pub async fn list_types() -> Json<serde_json::Value> {
    let types = IntegrationEngine::list_types();
    Json(serde_json::json!({ "data": types }))
}

// ---- Credentials ----

/// GET /bpe/api/integrations/credentials?organization_id=slug&integration_type=slack&page=1&per_page=50
pub async fn list_credentials(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "read").await?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(200);
    let result = IntegrationEngine::list_credentials(state.pool(), org_id, query.integration_type.as_deref(), page, per_page).await?;
    Ok(Json(serde_json::json!(result)))
}

/// POST /bpe/api/integrations/credentials
pub async fn create_credential(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateCredentialRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("name", &req.name)?;
    // Validate integration_type against known types
    let valid_types: Vec<&str> = INTEGRATION_TYPES.iter().map(|t| t.name).collect();
    if !valid_types.contains(&req.integration_type.as_str()) {
        return Err(BpeError::BadRequest(format!(
            "Unknown integration_type. Valid types: {}", valid_types.join(", ")
        )));
    }

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let cred = IntegrationEngine::create_credential(state.pool(), org_id, user_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": cred })))
}

/// GET /bpe/api/integrations/credentials/:id
pub async fn get_credential(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let cred = IntegrationEngine::get_credential(state.pool(), id).await?;
    verify_org_access(&claims, cred.organization_id)?;
    Ok(Json(serde_json::json!({ "data": cred })))
}

/// PUT /bpe/api/integrations/credentials/:id
pub async fn update_credential(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCredentialRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    if let Some(ref name) = req.name {
        validate_name("name", name)?;
    }
    let existing = IntegrationEngine::get_credential(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    let cred = IntegrationEngine::update_credential(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": cred })))
}

/// DELETE /bpe/api/integrations/credentials/:id
pub async fn delete_credential(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = IntegrationEngine::get_credential(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    IntegrationEngine::delete_credential(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// POST /bpe/api/integrations/credentials/:id/test
pub async fn test_credential(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = IntegrationEngine::get_credential(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    let cred = IntegrationEngine::test_credential(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "data": cred })))
}

// ---- Execution ----

/// POST /bpe/api/integrations/execute
pub async fn execute(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<ExecuteIntegrationRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("action", &req.action)?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let user_id = claims.user_id.parse::<Uuid>().ok();
    let result = IntegrationEngine::execute(state.pool(), org_id, user_id, &req, &state.config().ruflo_base_url).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}
