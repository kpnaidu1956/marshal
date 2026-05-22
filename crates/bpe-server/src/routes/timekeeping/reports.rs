use axum::{extract::{Query, State}, Extension, Json};
use bpe_core::{
    auth::AuthClaims,
    entity::registry::EntityTypeRegistry,
    error::BpeError,
    permissions::require_feature_access,
    timekeeping::{reports::ReportEngine, models::*},
};
use crate::AppState;

pub async fn hours_report(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "read").await?;
    let report = ReportEngine::hours_report(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!({ "data": report })))
}

pub async fn overtime_report(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "read").await?;
    let report = ReportEngine::overtime_report(state.pool(), org_id, &query).await?;
    Ok(Json(serde_json::json!({ "data": report })))
}

pub async fn flsa_report(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<FlsaReportQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "read").await?;
    let report = ReportEngine::flsa_report(state.pool(), org_id, query.cycle_start).await?;
    Ok(Json(serde_json::json!({ "data": report })))
}

pub async fn staffing_report(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<RosterQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "read").await?;
    let report = ReportEngine::staffing_report(state.pool(), org_id, query.date).await?;
    Ok(Json(serde_json::json!({ "data": report })))
}

pub async fn payroll_export(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<PayrollExportQuery>,
) -> Result<Json<serde_json::Value>, BpeError> {
    let org_id = EntityTypeRegistry::resolve_org_id(state.pool(), &query.organization_id).await?;
    require_feature_access(state.pool(), state.permissions(), &claims.user_id, org_id, claims.is_platform_admin, "reports", "admin").await?;
    let data = ReportEngine::payroll_export(state.pool(), org_id, query.period_id).await?;
    Ok(Json(serde_json::json!({ "data": data })))
}
