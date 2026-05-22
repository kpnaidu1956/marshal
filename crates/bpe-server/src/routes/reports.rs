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
    validation::{validate_name, validate_description, validate_category, validate_org_slug, validate_sql_template},
    reporting::{
        engine::ReportEngine,
        models::*,
        notifications::NotificationEngine,
    },
};
use uuid::Uuid;

use crate::AppState;

// ---- Report Templates ----

/// GET /bpe/api/reports/templates?organization_id=slug&category=workflow&page=1&per_page=50
pub async fn list_templates(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ReportListQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = if let Some(ref slug) = query.organization_id {
        let oid = EntityTypeRegistry::resolve_org_id(state.pool(), slug).await?;
        require_feature_access(state.pool(), state.permissions(), &claims.user_id, oid, claims.is_platform_admin, "reports", "read").await?;
        Some(oid)
    } else {
        None
    };
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(200);
    let result = ReportEngine::list_templates(state.pool(), org_id, query.category.as_deref(), page, per_page).await?;
    Ok(Json(serde_json::json!(result)))
}

/// POST /bpe/api/reports/templates
pub async fn create_template(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateReportTemplateRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_name("name", &req.name)?;
    validate_description(req.description.as_deref())?;
    validate_category(&req.category)?;
    validate_sql_template(&req.sql_template)?;

    let org_id = if let Some(ref slug) = req.organization_id {
        validate_org_slug(slug)?;
        let oid = EntityTypeRegistry::resolve_org_id(state.pool(), slug).await?;
        verify_org_access(&claims, oid)?;
        require_feature_access(state.pool(), state.permissions(), &claims.user_id, oid, claims.is_platform_admin, "reports", "write").await?;
        Some(oid)
    } else {
        None
    };
    let template = ReportEngine::create_template(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": template })))
}

/// GET /bpe/api/reports/templates/:id
pub async fn get_template(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let template = ReportEngine::get_template(state.pool(), id).await?;
    if let Some(org_id) = template.organization_id {
        verify_org_access(&claims, org_id)?;
    }
    Ok(Json(serde_json::json!({ "data": template })))
}

/// PUT /bpe/api/reports/templates/:id
pub async fn update_template(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateReportTemplateRequest>,
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
    if let Some(ref sql) = req.sql_template {
        validate_sql_template(sql)?;
    }
    let existing = ReportEngine::get_template(state.pool(), id).await?;
    if let Some(org_id) = existing.organization_id {
        verify_org_access(&claims, org_id)?;
    }
    let template = ReportEngine::update_template(state.pool(), id, &req).await?;
    Ok(Json(serde_json::json!({ "data": template })))
}

/// DELETE /bpe/api/reports/templates/:id
pub async fn delete_template(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let existing = ReportEngine::get_template(state.pool(), id).await?;
    if let Some(org_id) = existing.organization_id {
        verify_org_access(&claims, org_id)?;
    }
    ReportEngine::delete_template(state.pool(), id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// POST /bpe/api/reports/templates/:id/run
pub async fn run_report(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Path(id): Path<Uuid>,
    Json(req): Json<RunReportRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "read").await?;
    let result = ReportEngine::run_report(state.pool(), id, org_id, req.parameters.as_ref()).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

// ---- Built-in Reports ----

/// GET /bpe/api/reports/dashboard?organization_id=slug
pub async fn dashboard(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<super::reports::DashboardQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "read").await?;
    let data = ReportEngine::dashboard(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!({ "data": data })))
}

/// GET /bpe/api/reports/workflow-performance?organization_id=slug
pub async fn workflow_performance(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<super::reports::DashboardQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "read").await?;
    let data = ReportEngine::workflow_performance(state.pool(), org_id).await?;
    Ok(Json(serde_json::json!(data)))
}

#[derive(Debug, serde::Deserialize)]
pub struct DashboardQuery {
    pub organization_id: String,
}

// ---- Notifications ----

/// GET /bpe/api/notifications?organization_id=slug&unread_only=true
pub async fn list_notifications(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<NotificationListQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(200);
    let unread_only = query.unread_only.unwrap_or(false);

    let (data, total) = NotificationEngine::list_for_user(
        state.pool(), org_id, user_id, unread_only, page, per_page,
    ).await?;

    Ok(Json(serde_json::json!({
        "data": data,
        "page": page,
        "per_page": per_page,
        "total": total,
    })))
}

/// GET /bpe/api/notifications/unread-count?organization_id=slug
pub async fn unread_count(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<DashboardQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let count = NotificationEngine::unread_count(state.pool(), org_id, user_id).await?;
    Ok(Json(serde_json::json!({ "unread_count": count })))
}

/// POST /bpe/api/notifications
pub async fn create_notification(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<CreateNotificationRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    validate_org_slug(&req.organization_id)?;
    validate_name("title", &req.title)?;

    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &req.organization_id).await?;
    verify_org_access(&claims, org_id)?;
    let notif = NotificationEngine::create(state.pool(), org_id, &req).await?;
    Ok(Json(serde_json::json!({ "data": notif })))
}

/// POST /bpe/api/notifications/mark-read
pub async fn mark_read(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Json(req): Json<MarkReadRequest>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let count = NotificationEngine::mark_read(state.pool(), user_id, &req.notification_ids).await?;
    Ok(Json(serde_json::json!({ "marked_read": count })))
}

/// POST /bpe/api/notifications/mark-all-read?organization_id=slug
pub async fn mark_all_read(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<DashboardQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    let user_id = claims.user_id.parse::<Uuid>()
        .map_err(|_| BpeError::Internal("Invalid user_id in claims".into()))?;
    let count = NotificationEngine::mark_all_read(state.pool(), org_id, user_id).await?;
    Ok(Json(serde_json::json!({ "marked_read": count })))
}
