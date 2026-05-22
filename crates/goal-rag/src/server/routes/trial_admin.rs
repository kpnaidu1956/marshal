//! Admin-only endpoints for Marshal Trial management.
//!
//! All endpoints require JWT authentication + admin role.
//!
//!   GET  /api/admin/join-requests           — List pending join requests
//!   POST /api/admin/join-requests/:id/approve — Approve a join request
//!   POST /api/admin/join-requests/:id/reject  — Reject a join request
//!   POST /api/admin/users/:id/promote       — Promote user to admin
//!   POST /api/admin/users/:id/demote        — Remove admin role
//!   POST /api/trial/eula/accept             — Accept EULA

#[cfg(feature = "postgres")]
use axum::{extract::{State, Path}, http::StatusCode, Json};
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
pub struct ErrorResp {
    pub error: String,
}

#[cfg(feature = "postgres")]
type ApiError = (StatusCode, Json<ErrorResp>);

#[cfg(feature = "postgres")]
fn err(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(ErrorResp { error: msg.into() }))
}



/// Verify that the user is an admin of the given org.
#[cfg(feature = "postgres")]
async fn verify_admin(
    client: &deadpool_postgres::Client,
    user_id: &Uuid,
    org_id: &Uuid,
) -> Result<(), ApiError> {
    let row = client.query_opt(
        "SELECT 1 FROM api.user_roles WHERE user_id = $1 AND role = 'admin' AND organization_id = $2",
        &[user_id, org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if row.is_none() {
        return Err(err(StatusCode::FORBIDDEN, "Admin access required"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// GET /api/admin/join-requests
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Serialize)]
pub struct JoinRequestRow {
    id: String,
    requester_email: String,
    requester_first_name: String,
    requester_last_name: String,
    status: String,
    created_at: String,
    expires_at: String,
}

#[cfg(feature = "postgres")]
pub async fn list_join_requests(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
) -> Result<Json<Vec<JoinRequestRow>>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let user_id: Uuid = claims.user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;
    let org_id: Uuid = claims.organization_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;

    verify_admin(&client, &user_id, &org_id).await?;

    let rows = client.query(
        "SELECT id::text, requester_email, requester_first_name, requester_last_name,
                status, created_at::text, expires_at::text
         FROM trial.join_requests
         WHERE organization_id = $1 AND status = 'pending'
         ORDER BY created_at DESC",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let results: Vec<JoinRequestRow> = rows.iter().map(|r| JoinRequestRow {
        id: r.get(0),
        requester_email: r.get(1),
        requester_first_name: r.get(2),
        requester_last_name: r.get(3),
        status: r.get(4),
        created_at: r.get(5),
        expires_at: r.get(6),
    }).collect();

    Ok(Json(results))
}

// ---------------------------------------------------------------------------
// POST /api/admin/join-requests/:id/approve
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn approve_join_request(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
    Path(request_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let mut client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let user_id: Uuid = claims.user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;
    let org_id: Uuid = claims.organization_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;
    let req_id: Uuid = request_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid request ID"))?;

    verify_admin(&client, &user_id, &org_id).await?;

    // Verify org is still active
    let org_status = client.query_opt(
        "SELECT trial_status FROM api.organizations WHERE id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if let Some(row) = &org_status {
        let status: Option<String> = row.get(0);
        if status.as_deref() != Some("active") && status.as_deref() != Some("converted") {
            return Err(err(StatusCode::CONFLICT, "Organization trial has expired"));
        }
    }

    // Get the join request (also check expires_at)
    let jr = client.query_opt(
        "SELECT requester_email, requester_first_name, requester_last_name, requester_password_hash, organization_id
         FROM trial.join_requests
         WHERE id = $1 AND status = 'pending' AND expires_at > now()",
        &[&req_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let jr = jr.ok_or_else(|| err(StatusCode::NOT_FOUND, "Join request not found, already processed, or expired"))?;

    let req_email: String = jr.get(0);
    let req_first: String = jr.get(1);
    let req_last: String = jr.get(2);
    let req_hash: Option<String> = jr.get(3);
    let req_org: Uuid = jr.get(4);

    if req_org != org_id {
        return Err(err(StatusCode::FORBIDDEN, "Join request belongs to a different organization"));
    }

    // Validate password hash is present
    let password_hash = req_hash.ok_or_else(|| err(StatusCode::CONFLICT, "Join request has no password — cannot create account"))?;

    // Transaction: create user + update request + update quota (with atomic quota check)
    let txn = client.transaction().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // Atomic quota check + increment inside transaction (prevents TOCTOU)
    let quota_updated = txn.execute(
        "UPDATE trial.org_quotas SET current_users = current_users + 1, updated_at = now()
         WHERE organization_id = $1 AND current_users < max_users",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if quota_updated == 0 {
        let _ = txn.rollback().await;
        return Err(err(StatusCode::CONFLICT, "User quota exceeded for this organization"));
    }

    let new_user = txn.query_one(
        "INSERT INTO api.users (organization_id, first_name, last_name, email, password_hash)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id::text",
        &[&org_id, &req_first, &req_last, &req_email, &password_hash.as_str()],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let new_user_id: String = new_user.get(0);

    let new_uid: Uuid = new_user_id.parse().map_err(|_| {
        tracing::error!("Failed to parse new user ID as UUID: {}", new_user_id);
        err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    })?;
    txn.execute(
        "INSERT INTO api.user_roles (user_id, role, organization_id) VALUES ($1, 'member', $2)",
        &[&new_uid, &org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    txn.execute(
        "UPDATE trial.join_requests SET status = 'approved', reviewed_by = $1, reviewed_at = now() WHERE id = $2",
        &[&user_id, &req_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    txn.commit().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    tracing::info!("Join request {} approved — user {} created in org {}", req_id, req_email, org_id);

    Ok(Json(serde_json::json!({
        "status": "approved",
        "user_id": new_user_id,
        "email": req_email,
        "first_name": req_first,
        "last_name": req_last,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/admin/join-requests/:id/reject
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct RejectBody {
    pub reason: Option<String>,
}

#[cfg(feature = "postgres")]
pub async fn reject_join_request(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
    Path(request_id): Path<String>,
    Json(_body): Json<RejectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let user_id: Uuid = claims.user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;
    let org_id: Uuid = claims.organization_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;
    let req_id: Uuid = request_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid request ID"))?;

    verify_admin(&client, &user_id, &org_id).await?;

    let affected = client.execute(
        "UPDATE trial.join_requests SET status = 'rejected', reviewed_by = $1, reviewed_at = now()
         WHERE id = $2 AND status = 'pending' AND organization_id = $3",
        &[&user_id, &req_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    if affected == 0 {
        return Err(err(StatusCode::NOT_FOUND, "Join request not found or already processed"));
    }

    tracing::info!("Join request {} rejected by {}", req_id, user_id);
    Ok(Json(serde_json::json!({"status": "rejected"})))
}

// ---------------------------------------------------------------------------
// POST /api/admin/users/:id/promote
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn promote_to_admin(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
    Path(target_user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let admin_id: Uuid = claims.user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;
    let org_id: Uuid = claims.organization_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;
    let target_id: Uuid = target_user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid target user ID"))?;

    verify_admin(&client, &admin_id, &org_id).await?;

    // Verify target user belongs to same org
    let target = client.query_opt(
        "SELECT 1 FROM api.users WHERE id = $1 AND organization_id = $2 AND (is_deleted = false OR is_deleted IS NULL)",
        &[&target_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if target.is_none() {
        return Err(err(StatusCode::NOT_FOUND, "User not found in your organization"));
    }

    client.execute(
        "INSERT INTO api.user_roles (user_id, role, organization_id) VALUES ($1, 'admin', $2) ON CONFLICT DO NOTHING",
        &[&target_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    tracing::info!("User {} promoted to admin in org {} by {}", target_id, org_id, admin_id);
    Ok(Json(serde_json::json!({"status": "promoted", "user_id": target_user_id})))
}

// ---------------------------------------------------------------------------
// POST /api/admin/users/:id/demote
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn demote_from_admin(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
    Path(target_user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let admin_id: Uuid = claims.user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;
    let org_id: Uuid = claims.organization_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;
    let target_id: Uuid = target_user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid target user ID"))?;

    if admin_id == target_id {
        return Err(err(StatusCode::BAD_REQUEST, "Cannot demote yourself"));
    }

    verify_admin(&client, &admin_id, &org_id).await?;

    // Ensure we don't remove the last admin
    let admin_count = client.query_one(
        "SELECT COUNT(*) FROM api.user_roles WHERE role = 'admin' AND organization_id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let count: i64 = admin_count.get(0);
    if count <= 1 {
        return Err(err(StatusCode::CONFLICT, "Cannot demote the last admin"));
    }

    client.execute(
        "DELETE FROM api.user_roles WHERE user_id = $1 AND role = 'admin' AND organization_id = $2",
        &[&target_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    tracing::info!("User {} demoted from admin in org {} by {}", target_id, org_id, admin_id);
    Ok(Json(serde_json::json!({"status": "demoted", "user_id": target_user_id})))
}

// ---------------------------------------------------------------------------
// POST /api/trial/eula/accept
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct EulaAcceptBody {
    pub eula_version_id: String,
}

#[cfg(feature = "postgres")]
pub async fn accept_eula(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<AuthClaims>,
    Json(body): Json<EulaAcceptBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let user_id: Uuid = claims.user_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid user ID"))?;
    let org_id: Uuid = claims.organization_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid org ID"))?;
    let eula_id: Uuid = body.eula_version_id.parse().map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid EULA version ID"))?;

    // Verify EULA version exists
    let exists = client.query_opt(
        "SELECT 1 FROM trial.eula_versions WHERE id = $1", &[&eula_id]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if exists.is_none() {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid EULA version"));
    }

    client.execute(
        "INSERT INTO trial.eula_acceptances (user_id, organization_id, eula_version_id)
         VALUES ($1, $2, $3)",
        &[&user_id, &org_id, &eula_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    client.execute(
        "UPDATE api.organizations SET eula_accepted_at = now(), eula_accepted_by = $1 WHERE id = $2",
        &[&user_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    tracing::info!("EULA {} accepted by user {} for org {}", eula_id, user_id, org_id);
    Ok(Json(serde_json::json!({"status": "accepted"})))
}

// ---------------------------------------------------------------------------
// Fallbacks when postgres is not enabled
// ---------------------------------------------------------------------------

#[cfg(not(feature = "postgres"))]
pub async fn list_join_requests() -> (StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn approve_join_request() -> (StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn reject_join_request() -> (StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn promote_to_admin() -> (StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn demote_from_admin() -> (StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn accept_eula() -> (StatusCode, axum::Json<serde_json::Value>) {
    (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
