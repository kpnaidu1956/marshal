//! GDPR compliance and trial conversion endpoints.
//!
//!   GET  /api/trial/export        — Export all org data as JSON
//!   POST /api/trial/delete-account — Schedule org deletion
//!   POST /api/trial/convert       — Convert trial to paid (stub)

#[cfg(feature = "postgres")]
use axum::{extract::State, http::StatusCode, Json};
#[cfg(feature = "postgres")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "postgres")]
use uuid::Uuid;
#[cfg(feature = "postgres")]
use crate::server::state::AppState;
#[cfg(feature = "postgres")]
use super::auth::AuthClaims;

#[cfg(feature = "postgres")]
#[derive(Serialize)]
pub struct ErrorResp { pub error: String }

#[cfg(feature = "postgres")]
type ApiError = (StatusCode, Json<ErrorResp>);

#[cfg(feature = "postgres")]
fn err(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(ErrorResp { error: msg.into() }))
}

// ---------------------------------------------------------------------------
// GET /api/trial/export — Export all org data
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn export_org_data(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| {
        tracing::error!("DB error: {}", e);
        err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    })?;

    let org_id: Uuid = claims.organization_id.parse()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;
    let user_id: Uuid = claims.user_id.parse()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;

    // Verify admin
    let admin = client.query_opt(
        "SELECT 1 FROM api.user_roles WHERE user_id = $1 AND role = 'admin' AND organization_id = $2",
        &[&user_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;
    if admin.is_none() {
        return Err(err(StatusCode::FORBIDDEN, "Admin access required for data export"));
    }

    // Export org info
    let org = client.query_opt(
        "SELECT name, display_name, email_domain, trial_started_at::text, trial_expires_at::text, trial_status
         FROM api.organizations WHERE id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    let org_data = org.map(|r| serde_json::json!({
        "name": r.get::<_, Option<String>>(0),
        "display_name": r.get::<_, Option<String>>(1),
        "email_domain": r.get::<_, Option<String>>(2),
        "trial_started_at": r.get::<_, Option<String>>(3),
        "trial_expires_at": r.get::<_, Option<String>>(4),
        "trial_status": r.get::<_, Option<String>>(5),
    }));

    // Export users
    let users = client.query(
        "SELECT id::text, email, first_name, last_name, title, created_at::text
         FROM api.users WHERE organization_id = $1 AND (is_deleted = false OR is_deleted IS NULL)",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    let users_data: Vec<serde_json::Value> = users.iter().map(|r| serde_json::json!({
        "id": r.get::<_, String>(0),
        "email": r.get::<_, Option<String>>(1),
        "first_name": r.get::<_, Option<String>>(2),
        "last_name": r.get::<_, Option<String>>(3),
        "title": r.get::<_, Option<String>>(4),
        "created_at": r.get::<_, Option<String>>(5),
    })).collect();

    // Export tasks (no LIMIT — GDPR requires complete data)
    let tasks = client.query(
        "SELECT id::text, title, description, status, priority, created_at::text
         FROM api.tasks WHERE organization_id = $1 ORDER BY created_at DESC",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    let tasks_data: Vec<serde_json::Value> = tasks.iter().map(|r| serde_json::json!({
        "id": r.get::<_, String>(0),
        "title": r.get::<_, Option<String>>(1),
        "description": r.get::<_, Option<String>>(2),
        "status": r.get::<_, Option<String>>(3),
        "priority": r.get::<_, Option<String>>(4),
        "created_at": r.get::<_, Option<String>>(5),
    })).collect();

    // Export goals
    let goals = client.query(
        "SELECT id::text, title, description, status, created_at::text
         FROM api.goals WHERE organization_id = $1 ORDER BY created_at DESC",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    let goals_data: Vec<serde_json::Value> = goals.iter().map(|r| serde_json::json!({
        "id": r.get::<_, String>(0),
        "title": r.get::<_, Option<String>>(1),
        "description": r.get::<_, Option<String>>(2),
        "status": r.get::<_, Option<String>>(3),
        "created_at": r.get::<_, Option<String>>(4),
    })).collect();

    // Export conversations & messages
    let messages = client.query(
        "SELECT m.id::text, m.role, m.content, m.created_at::text, c.title as conversation_title
         FROM api.messages m JOIN api.conversations c ON c.id = m.conversation_id
         WHERE m.organization_id = $1 ORDER BY m.created_at DESC",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    let messages_data: Vec<serde_json::Value> = messages.iter().map(|r| serde_json::json!({
        "id": r.get::<_, String>(0),
        "role": r.get::<_, Option<String>>(1),
        "content": r.get::<_, Option<String>>(2),
        "created_at": r.get::<_, Option<String>>(3),
        "conversation_title": r.get::<_, Option<String>>(4),
    })).collect();

    // Export EULA acceptances (includes IP addresses — personal data)
    let eula_acceptances = client.query(
        "SELECT ea.accepted_at::text, ea.ip_address::text, ea.user_agent, ev.version
         FROM trial.eula_acceptances ea
         JOIN trial.eula_versions ev ON ev.id = ea.eula_version_id
         WHERE ea.organization_id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    let eula_data: Vec<serde_json::Value> = eula_acceptances.iter().map(|r| serde_json::json!({
        "accepted_at": r.get::<_, Option<String>>(0),
        "ip_address": r.get::<_, Option<String>>(1),
        "user_agent": r.get::<_, Option<String>>(2),
        "eula_version": r.get::<_, Option<String>>(3),
    })).collect();

    tracing::info!("Data export for org {} by user {}", org_id, user_id);

    Ok(Json(serde_json::json!({
        "export_date": chrono::Utc::now().to_rfc3339(),
        "organization": org_data,
        "users": users_data,
        "tasks": tasks_data,
        "goals": goals_data,
        "messages": messages_data,
        "eula_acceptances": eula_data,
        "counts": {
            "users": users_data.len(),
            "tasks": tasks_data.len(),
            "goals": goals_data.len(),
            "messages": messages_data.len(),
        },
    })))
}

// ---------------------------------------------------------------------------
// POST /api/trial/delete-account — Schedule org deletion
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn delete_account(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| {
        tracing::error!("DB error: {}", e);
        err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    })?;

    let org_id: Uuid = claims.organization_id.parse()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;
    let user_id: Uuid = claims.user_id.parse()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;

    // Verify admin
    let admin = client.query_opt(
        "SELECT 1 FROM api.user_roles WHERE user_id = $1 AND role = 'admin' AND organization_id = $2",
        &[&user_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;
    if admin.is_none() {
        return Err(err(StatusCode::FORBIDDEN, "Admin access required to delete account"));
    }

    // Schedule deletion by setting status to 'suspended' and trial_expires_at to now
    // The lifecycle task will purge data 30 days after trial_expires_at
    client.execute(
        "UPDATE api.organizations SET trial_status = 'suspended', trial_expires_at = now()
         WHERE id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    tracing::warn!("Account deletion scheduled for org {} by user {}", org_id, user_id);

    Ok(Json(serde_json::json!({
        "status": "deletion_scheduled",
        "message": "Your organization has been scheduled for deletion. All data will be permanently removed within 30 days. You can contact support to cancel this request.",
    })))
}

// ---------------------------------------------------------------------------
// POST /api/trial/convert — Convert trial to paid (stub)
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct ConvertRequest {
    pub plan: String,
}

#[cfg(feature = "postgres")]
pub async fn convert_trial(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
    Json(body): Json<ConvertRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| {
        tracing::error!("DB error: {}", e);
        err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    })?;

    let org_id: Uuid = claims.organization_id.parse()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;

    // Stub: just update the status (real Stripe integration would go here)
    client.execute(
        "UPDATE api.organizations SET trial_status = 'converted' WHERE id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    // Insert subscription record
    client.execute(
        "INSERT INTO trial.subscriptions (organization_id, plan, status)
         VALUES ($1, $2, 'active')",
        &[&org_id, &body.plan],
    ).await.map_err(|e| { tracing::error!("DB: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error") })?;

    tracing::info!("Trial converted to paid plan '{}' for org {}", body.plan, org_id);

    Ok(Json(serde_json::json!({
        "status": "converted",
        "plan": body.plan,
        "message": "Your trial has been converted to a paid plan. All restrictions have been removed.",
    })))
}

// ---------------------------------------------------------------------------
// Fallbacks
// ---------------------------------------------------------------------------

#[cfg(not(feature = "postgres"))]
pub async fn export_org_data() -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn delete_account() -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn convert_trial() -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
