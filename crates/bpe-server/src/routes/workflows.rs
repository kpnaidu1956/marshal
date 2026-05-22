use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use bpe_core::{
    permissions::require_feature_access,
    audit::query::AuditQuery,
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    middleware::verify_org_access,
    validation::{validate_name, validate_description, validate_category, validate_org_slug},
    workflow::{
        engine::WorkflowEngine,
        models::*,
    },
};
use uuid::Uuid;

use crate::AppState;

// ---- Definitions ----

/// GET /bpe/api/workflows/definitions?organization_id=slug&category=onboarding&page=1&per_page=50
pub async fn list_definitions(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "read").await?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(200);
    let result = WorkflowEngine::list_definitions(state.pool(), org_id, query.category.as_deref(), page, per_page).await?;
    Ok(Json(serde_json::json!(result)))
}

/// POST /bpe/api/workflows/definitions
pub async fn create_definition(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateDefinitionRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("name", &req.name)?;
    validate_description(req.description.as_deref())?;
    if let Some(ref cat) = req.category {
        validate_category(cat)?;
    }
    if req.step_templates.is_empty() {
        return Err(BpeError::BadRequest("step_templates cannot be empty".into()));
    }
    if req.step_templates.len() > 100 {
        return Err(BpeError::BadRequest("step_templates cannot exceed 100 steps".into()));
    }

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let def = WorkflowEngine::create_definition(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": def })))
}

/// PUT /bpe/api/workflows/definitions/:id
pub async fn update_definition(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDefinitionRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    if let Some(ref name) = req.name {
        validate_name("name", name)?;
    }
    if let Some(ref desc) = req.description {
        validate_description(Some(desc.as_str()))?;
    }
    if let Some(ref cat) = req.category {
        validate_category(cat)?;
    }
    if let Some(ref steps) = req.step_templates {
        if steps.is_empty() {
            return Err(BpeError::BadRequest("step_templates cannot be empty".into()));
        }
        if steps.len() > 100 {
            return Err(BpeError::BadRequest("step_templates cannot exceed 100 steps".into()));
        }
    }
    let existing = WorkflowEngine::get_definition(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    let def = WorkflowEngine::update_definition(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": def })))
}

/// DELETE /bpe/api/workflows/definitions/:id
pub async fn delete_definition(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = WorkflowEngine::get_definition(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    WorkflowEngine::delete_definition(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// POST /bpe/api/workflows/definitions/:id/execute
pub async fn execute_definition(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<ExecuteDefinitionRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;

    let execution = WorkflowEngine::execute_definition(
        state.pool(), org_id, user_id, id,
        req.target_entity_id, req.context.as_ref(),
    ).await?;

    Ok(Json(serde_json::json!({ "data": execution })))
}

// ---- Executions ----

/// GET /bpe/api/workflows/executions?organization_id=slug&status=running&page=1
pub async fn list_executions(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "admin", "read").await?;
    let result = WorkflowEngine::list_executions(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!(result)))
}

/// GET /bpe/api/workflows/executions/:id
pub async fn get_execution(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let execution = WorkflowEngine::get_execution(state.pool(), id).await?;
    verify_org_access(&claims, execution.organization_id)?;
    let steps = WorkflowEngine::get_steps(state.pool(), id).await?;
    Ok(Json(serde_json::json!({
        "data": execution,
        "steps": steps
    })))
}

/// POST /bpe/api/workflows/executions/:id/confirm
pub async fn confirm(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<ConfirmRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    // Verify org access with lightweight query (avoids double-fetch since engine methods re-fetch)
    let client = state.pool().get().await?;
    let org_row = client.query_opt(
        "SELECT organization_id FROM bpe.workflow_executions WHERE id = $1",
        &[&id],
    ).await?.ok_or_else(|| BpeError::NotFound(format!("Execution {id} not found")))?;
    let org_id: Uuid = org_row.get(0);
    verify_org_access(&claims, org_id)?;
    let execution = WorkflowEngine::confirm(state.pool(), id, req.steps).await?;
    Ok(Json(serde_json::json!({ "data": execution })))
}

/// POST /bpe/api/workflows/executions/:id/start
pub async fn start_execution(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    // Verify org access with lightweight query (avoids double-fetch since engine methods re-fetch)
    let client = state.pool().get().await?;
    let org_row = client.query_opt(
        "SELECT organization_id FROM bpe.workflow_executions WHERE id = $1",
        &[&id],
    ).await?.ok_or_else(|| BpeError::NotFound(format!("Execution {id} not found")))?;
    let org_id: Uuid = org_row.get(0);
    verify_org_access(&claims, org_id)?;
    let execution = WorkflowEngine::start(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "data": execution })))
}

/// POST /bpe/api/workflows/executions/:id/pause
pub async fn pause_execution(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    // Verify org access with lightweight query (avoids double-fetch since engine methods re-fetch)
    let client = state.pool().get().await?;
    let org_row = client.query_opt(
        "SELECT organization_id FROM bpe.workflow_executions WHERE id = $1",
        &[&id],
    ).await?.ok_or_else(|| BpeError::NotFound(format!("Execution {id} not found")))?;
    let org_id: Uuid = org_row.get(0);
    verify_org_access(&claims, org_id)?;
    let execution = WorkflowEngine::pause(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "data": execution })))
}

/// POST /bpe/api/workflows/executions/:id/resume
pub async fn resume_execution(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    // Verify org access with lightweight query (avoids double-fetch since engine methods re-fetch)
    let client = state.pool().get().await?;
    let org_row = client.query_opt(
        "SELECT organization_id FROM bpe.workflow_executions WHERE id = $1",
        &[&id],
    ).await?.ok_or_else(|| BpeError::NotFound(format!("Execution {id} not found")))?;
    let org_id: Uuid = org_row.get(0);
    verify_org_access(&claims, org_id)?;
    let execution = WorkflowEngine::resume(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "data": execution })))
}

/// POST /bpe/api/workflows/executions/:id/cancel
pub async fn cancel_execution(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    // Verify org access (was missing — security fix)
    let client = state.pool().get().await?;
    let org_row = client.query_opt(
        "SELECT organization_id FROM bpe.workflow_executions WHERE id = $1",
        &[&id],
    ).await?.ok_or_else(|| BpeError::NotFound(format!("Execution {id} not found")))?;
    let org_id: Uuid = org_row.get(0);
    verify_org_access(&claims, org_id)?;
    let user_id = claims.user_id.parse::<Uuid>().ok();
    let execution = WorkflowEngine::cancel(state.pool(), id, user_id).await?;
    Ok(Json(serde_json::json!({ "data": execution })))
}

/// GET /bpe/api/workflows/executions/:id/timeline
pub async fn timeline(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let execution = WorkflowEngine::get_execution(state.pool(), id).await?;
    verify_org_access(&claims, execution.organization_id)?;
    let events = AuditQuery::by_execution(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "data": events })))
}

// ---- Steps ----

/// POST /bpe/api/workflows/steps/:id/complete
pub async fn complete_step(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteStepRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = claims.user_id.parse::<Uuid>().ok();
    let step = WorkflowEngine::complete_step(state.pool(), id, req.output_data, user_id).await?;
    Ok(Json(serde_json::json!({ "data": step })))
}

/// POST /bpe/api/workflows/steps/:id/skip
pub async fn skip_step(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<SkipStepRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = claims.user_id.parse::<Uuid>().ok();
    let step = WorkflowEngine::skip_step(state.pool(), id, req.reason.as_deref(), user_id).await?;
    Ok(Json(serde_json::json!({ "data": step })))
}

/// POST /bpe/api/workflows/steps/:id/retry
pub async fn retry_step(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = WorkflowEngine::get_step_public(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    let user_id = claims.user_id.parse::<Uuid>().ok();
    let step = WorkflowEngine::retry_step(state.pool(), id, user_id).await?;
    Ok(Json(serde_json::json!({ "data": step })))
}

/// POST /bpe/api/workflows/steps/:id/assign
pub async fn assign_step(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<AssignStepRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = WorkflowEngine::get_step_public(state.pool(), id).await?;
    verify_org_access(&claims, existing.organization_id)?;
    let actor_id = claims.user_id.parse::<Uuid>().ok();
    let step = WorkflowEngine::assign_step(state.pool(), id, req.user_id, actor_id).await?;
    Ok(Json(serde_json::json!({ "data": step })))
}
