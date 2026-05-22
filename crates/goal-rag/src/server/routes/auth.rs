//! Authentication endpoints for Marshal UI
//!
//! Provides JWT token generation compatible with PostgREST.
//! Supports `platform_admin` role for cross-organization data access.

use axum::{extract::State, http::StatusCode, Json};
use axum::http::header;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::server::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// PostgREST JWT secret (HMAC-SHA256).
/// In production, load from config or env var.
const JWT_SECRET_ENV: &str = "POSTGREST_JWT_SECRET";

/// JWT audience expected by PostgREST.
const JWT_AUD: &str = "postgrest";

/// Token validity duration (24 hours).
const TOKEN_EXPIRY_SECS: i64 = 86400;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
    pub organizations: Vec<OrgInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<std::collections::HashMap<String, Vec<String>>>,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub organization_id: String,
    pub avatar_url: Option<String>,
    pub title: Option<String>,
    pub is_platform_admin: bool,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Lightweight org info returned in the organizations endpoint.
#[derive(Debug, Serialize)]
pub struct OrgInfo {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
}

// ---------------------------------------------------------------------------
// JWT Claims
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct Claims {
    role: String,
    email: String,
    user_id: String,
    organization_id: String,
    is_platform_admin: bool,
    aud: String,
    exp: i64,
    iat: i64,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `POST /api/auth/login`
///
/// Validates email + password against `api.users`, returns a signed JWT
/// that PostgREST will accept for the `authenticated` role.
#[cfg(feature = "postgres")]
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pool = state.pg_pool().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "Database unavailable".into() }),
        )
    })?;

    let client = pool.get().await.map_err(|e| {
        tracing::error!("Failed to get DB connection for login: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "Database connection failed".into() }),
        )
    })?;

    // Look up user by email
    let row = client
        .query_opt(
            "SELECT id::text, email, first_name, last_name, \
                    organization_id::text, avatar_url, title, password_hash \
             FROM api.users \
             WHERE email = $1 AND (is_deleted IS NULL OR is_deleted = false) \
             LIMIT 1",
            &[&payload.email],
        )
        .await
        .map_err(|e| {
            tracing::error!("Login query failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Authentication service error".into() }),
            )
        })?;

    let row = row.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse { error: "Invalid email or password".into() }),
        )
    })?;

    // Extract user fields
    let user_id: String = row.get("id");
    let email: String = row.get("email");
    let first_name: String = row.get::<_, Option<String>>("first_name").unwrap_or_default();
    let last_name: String = row.get::<_, Option<String>>("last_name").unwrap_or_default();
    let organization_id: String = row.get("organization_id");
    let avatar_url: Option<String> = row.get("avatar_url");
    let title: Option<String> = row.get("title");

    // Password verification: bcrypt hash if available, legacy fallback otherwise
    let password_hash: Option<String> = row.get("password_hash");
    if let Some(ref hash) = password_hash {
        let valid = bcrypt::verify(&payload.password, hash).map_err(|e| {
            tracing::error!("bcrypt verification error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "Authentication service error".into() }),
            )
        })?;
        if !valid {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse { error: "Invalid email or password".into() }),
            ));
        }
    } else {
        // TRIAL: No legacy fallback — all users must have a password hash
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse { error: "Account requires password reset".into() }),
        ));
    }

    // Check if user has platform_admin role
    let user_uuid = uuid::Uuid::parse_str(&user_id).ok();
    let is_platform_admin = if let Some(ref uid) = user_uuid {
        client
            .query_opt(
                "SELECT 1 FROM api.user_roles \
                 WHERE user_id = $1 AND role = 'platform_admin' \
                 LIMIT 1",
                &[uid],
            )
            .await
            .map(|r| r.is_some())
            .unwrap_or(false)
    } else {
        false
    };

    // Build JWT
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        role: "authenticated".into(),
        email: email.clone(),
        user_id: user_id.clone(),
        organization_id: organization_id.clone(),
        is_platform_admin,
        aud: JWT_AUD.into(),
        exp: now + TOKEN_EXPIRY_SECS,
        iat: now,
    };

    let token = sign_jwt(&claims).map_err(|e| {
        tracing::error!("JWT signing failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "Token generation failed".into() }),
        )
    })?;

    tracing::info!(
        user_id = %user_id,
        email = %email,
        org_id = %organization_id,
        is_platform_admin = is_platform_admin,
        "User authenticated successfully"
    );

    // Fetch organizations for the login response
    let organizations = if is_platform_admin {
        client
            .query(
                "SELECT id::text, name, \
                 COALESCE(display_name, name) AS display_name \
                 FROM api.organizations \
                 ORDER BY name",
                &[],
            )
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| OrgInfo {
                id: r.get("id"),
                name: r.get("name"),
                display_name: r.get("display_name"),
            })
            .collect()
    } else {
        let org_uuid = uuid::Uuid::parse_str(&organization_id).ok();
        if let Some(ref oid) = org_uuid {
            client
                .query(
                    "SELECT id::text, name, \
                     COALESCE(display_name, name) AS display_name \
                     FROM api.organizations WHERE id = $1",
                    &[oid],
                )
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| OrgInfo {
                    id: r.get("id"),
                    name: r.get("name"),
                    display_name: r.get("display_name"),
                })
                .collect()
        } else {
            vec![]
        }
    };

    // Load user permissions from roles + groups
    let permissions = if !is_platform_admin {
        let org_uuid = uuid::Uuid::parse_str(&organization_id).ok();
        let perm_rows = if let (Some(ref uid), Some(ref oid)) = (&user_uuid, &org_uuid) {
            match client.query(
                "SELECT DISTINCT rp.feature::text AS feature, rp.action::text AS action
                 FROM api.user_roles ur
                 JOIN api.roles r ON r.name = ur.role::text AND r.organization_id = $2
                 JOIN api.role_permissions rp ON rp.role_id = r.id
                 WHERE ur.user_id = $1 AND ur.organization_id = $2
                 UNION
                 SELECT DISTINCT gp.feature::text AS feature, gp.action::text AS action
                 FROM api.user_groups ug
                 JOIN api.group_permissions gp ON gp.group_id = ug.group_id
                 WHERE ug.user_id = $1 AND ug.organization_id = $2",
                &[uid, oid],
            ).await {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("Failed to load permissions for user {}: {}", user_id, e);
                    vec![]
                }
            }
        } else {
            tracing::warn!("Could not parse user_id or organization_id as UUID for permission loading");
            vec![]
        };

        let mut perm_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for row in &perm_rows {
            let feature: String = row.get("feature");
            let action: String = row.get("action");
            perm_map.entry(feature).or_default().push(action);
        }
        if perm_map.is_empty() { None } else { Some(perm_map) }
    } else {
        None // Admins have full access, no need to enumerate
    };

    Ok(Json(LoginResponse {
        token,
        user: UserInfo {
            id: user_id,
            email,
            first_name,
            last_name,
            organization_id,
            avatar_url,
            title,
            is_platform_admin,
        },
        organizations,
        permissions,
    }))
}

/// `GET /api/auth/organizations`
///
/// Returns all organizations for `platform_admin` users, or just the
/// user's own organization for regular users. Reads the JWT from the
/// Authorization header to determine the caller's identity.
#[cfg(feature = "postgres")]
pub async fn list_organizations(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<OrgInfo>>, (StatusCode, Json<ErrorResponse>)> {
    let pool = state.pg_pool().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "Database unavailable".into() }),
        )
    })?;

    // Decode JWT from Authorization header (no signature verification —
    // the server trusts its own tokens and PostgREST re-validates).
    let token_str = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Missing authorization".into() }))
        })?;

    let claims = decode_jwt_claims(token_str).map_err(|e| {
        (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: e }))
    })?;

    let client = pool.get().await.map_err(|e| {
        tracing::error!("DB connection failed: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".into() }))
    })?;

    let rows = if claims.is_platform_admin {
        // Platform Admin: return ALL organizations
        client
            .query(
                "SELECT id::text, name, display_name FROM api.organizations ORDER BY name",
                &[],
            )
            .await
    } else {
        // Regular user: return only their organization
        let org_uuid = uuid::Uuid::parse_str(&claims.organization_id).map_err(|_| {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid organization_id in token".into() }))
        })?;
        client
            .query(
                "SELECT id::text, name, display_name FROM api.organizations \
                 WHERE id = $1",
                &[&org_uuid],
            )
            .await
    };

    let rows = rows.map_err(|e| {
        tracing::error!("Org query failed: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Query failed".into() }))
    })?;

    let orgs: Vec<OrgInfo> = rows.iter().map(|r| OrgInfo {
        id: r.get("id"),
        name: r.get("name"),
        display_name: r.get("display_name"),
    }).collect();

    Ok(Json(orgs))
}

// ---------------------------------------------------------------------------
// Set password endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SetPasswordRequest {
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct SetPasswordResponse {
    pub success: bool,
    pub message: String,
}

/// `POST /api/auth/set-password`
///
/// Sets or updates the user's bcrypt password hash.
/// Requires a valid JWT in the Authorization header.
#[cfg(feature = "postgres")]
pub async fn set_password(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<SetPasswordRequest>,
) -> Result<Json<SetPasswordResponse>, (StatusCode, Json<ErrorResponse>)> {
    if payload.new_password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "Password must be at least 8 characters".into() }),
        ));
    }
    if payload.new_password.len() > 72 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "Password must be at most 72 characters".into() }),
        ));
    }

    let token_str = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "Missing authorization".into() }))
        })?;

    let claims = decode_jwt_claims(token_str).map_err(|e| {
        (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: e }))
    })?;

    let user_uuid = uuid::Uuid::parse_str(&claims.user_id).map_err(|_| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid user_id in token".into() }))
    })?;

    let hash = bcrypt::hash(&payload.new_password, 12).map_err(|e| {
        tracing::error!("bcrypt hash error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "Password hashing failed".into() }),
        )
    })?;

    let pool = state.pg_pool().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "Database unavailable".into() }),
        )
    })?;
    let client = pool.get().await.map_err(|e| {
        tracing::error!("DB connection failed: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".into() }))
    })?;

    client
        .execute(
            "UPDATE api.users SET password_hash = $1 WHERE id = $2",
            &[&hash, &user_uuid],
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to set password: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Failed to update password".into() }))
        })?;

    tracing::info!(user_id = %user_uuid, "Password hash set successfully");

    Ok(Json(SetPasswordResponse {
        success: true,
        message: "Password set successfully".into(),
    }))
}

/// Fallback when the `postgres` feature is not enabled.
#[cfg(not(feature = "postgres"))]
pub async fn login() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse { error: "Authentication requires PostgreSQL".into() }),
    )
}

/// Fallback when the `postgres` feature is not enabled.
#[cfg(not(feature = "postgres"))]
pub async fn list_organizations() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse { error: "Authentication requires PostgreSQL".into() }),
    )
}

/// Fallback when the `postgres` feature is not enabled.
#[cfg(not(feature = "postgres"))]
pub async fn set_password() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse { error: "Authentication requires PostgreSQL".into() }),
    )
}

// ---------------------------------------------------------------------------
// JWT helpers
// ---------------------------------------------------------------------------

/// Sign a JWT using HMAC-SHA256 with the PostgREST-compatible secret.
///
/// PostgREST uses the `jwt-secret` value directly as the HMAC key
/// (raw string bytes) unless `jwt-secret-is-base64 = true` is set.
fn sign_jwt(claims: &Claims) -> Result<String, String> {
    let secret = std::env::var(JWT_SECRET_ENV)
        .expect("POSTGREST_JWT_SECRET env var must be set — refusing to start without a JWT secret");

    // Use the secret string directly as the HMAC key (PostgREST default behavior)
    let secret_bytes = secret.as_bytes();

    // Header: {"alg":"HS256","typ":"JWT"}
    let header = base64url_encode(b"{\"alg\":\"HS256\",\"typ\":\"JWT\"}");

    // Payload
    let payload_json = serde_json::to_vec(claims)
        .map_err(|e| format!("Failed to serialize claims: {}", e))?;
    let payload = base64url_encode(&payload_json);

    // Signature
    let signing_input = format!("{}.{}", header, payload);
    let mut mac = HmacSha256::new_from_slice(&secret_bytes)
        .map_err(|e| format!("HMAC key error: {}", e))?;
    mac.update(signing_input.as_bytes());
    let signature = base64url_encode(&mac.finalize().into_bytes());

    Ok(format!("{}.{}.{}", header, payload, signature))
}

/// Base64url encoding (no padding, URL-safe alphabet).
fn base64url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Minimal JWT claims struct for server-side decoding (no signature check).
#[derive(Debug, Deserialize)]
struct DecodedClaims {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    organization_id: String,
    #[serde(default)]
    is_platform_admin: bool,
}

/// Decode JWT payload without verifying signature.
/// Suitable only for server-side use where the server trusts its own tokens.
fn decode_jwt_claims(token: &str) -> Result<DecodedClaims, String> {
    use base64::Engine;
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format".into());
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| {
            // Try with padding
            let padded = match parts[1].len() % 4 {
                2 => format!("{}==", parts[1]),
                3 => format!("{}=", parts[1]),
                _ => parts[1].to_string(),
            };
            base64::engine::general_purpose::URL_SAFE.decode(&padded)
        })
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    serde_json::from_slice(&payload)
        .map_err(|e| format!("JSON parse failed: {}", e))
}

// ---------------------------------------------------------------------------
// JWT Auth Middleware
// ---------------------------------------------------------------------------

/// Verified JWT claims available to downstream handlers via request extensions.
#[derive(Debug, Clone)]
pub struct AuthClaims {
    pub user_id: String,
    pub email: String,
    pub organization_id: String,
    pub is_platform_admin: bool,
    pub exp: i64,
}

/// Errors from the JWT auth middleware.
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
    Expired,
}

impl axum::response::IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authorization token"),
            AuthError::InvalidToken(_) => (StatusCode::UNAUTHORIZED, "Invalid authorization token"),
            AuthError::Expired => (StatusCode::UNAUTHORIZED, "Token expired"),
        };
        (status, Json(ErrorResponse { error: msg.into() })).into_response()
    }
}

/// Verify JWT HMAC-SHA256 signature.
fn verify_jwt_signature(token: &str) -> Result<(), String> {
    use base64::Engine;

    let secret = std::env::var(JWT_SECRET_ENV)
        .expect("POSTGREST_JWT_SECRET env var must be set — refusing to verify JWT without a secret");
    let secret_bytes = secret.as_bytes();

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format".into());
    }

    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let mut mac = HmacSha256::new_from_slice(secret_bytes)
        .map_err(|e| format!("HMAC key error: {}", e))?;
    mac.update(signing_input.as_bytes());

    let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[2])
        .or_else(|_| {
            let padded = match parts[2].len() % 4 {
                2 => format!("{}==", parts[2]),
                3 => format!("{}=", parts[2]),
                _ => parts[2].to_string(),
            };
            base64::engine::general_purpose::URL_SAFE.decode(&padded)
        })
        .map_err(|e| format!("Signature decode failed: {}", e))?;

    mac.verify_slice(&signature_bytes)
        .map_err(|_| "Invalid JWT signature".to_string())
}

/// Decode JWT payload into AuthClaims with full fields.
fn decode_jwt_auth_claims(token: &str) -> Result<AuthClaims, String> {
    use base64::Engine;

    #[derive(Debug, Deserialize)]
    struct FullClaims {
        #[serde(default)]
        user_id: String,
        #[serde(default)]
        email: String,
        #[serde(default)]
        organization_id: String,
        #[serde(default)]
        is_platform_admin: bool,
        #[serde(default)]
        exp: i64,
    }

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format".into());
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| {
            let padded = match parts[1].len() % 4 {
                2 => format!("{}==", parts[1]),
                3 => format!("{}=", parts[1]),
                _ => parts[1].to_string(),
            };
            base64::engine::general_purpose::URL_SAFE.decode(&padded)
        })
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    let claims: FullClaims = serde_json::from_slice(&payload)
        .map_err(|e| format!("JSON parse failed: {}", e))?;

    Ok(AuthClaims {
        user_id: claims.user_id,
        email: claims.email,
        organization_id: claims.organization_id,
        is_platform_admin: claims.is_platform_admin,
        exp: claims.exp,
    })
}

/// Axum middleware that rejects requests without a valid JWT.
/// Inserts `AuthClaims` into request extensions for downstream handlers.
pub async fn require_auth(
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, AuthError> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AuthError::MissingToken)?;

    verify_jwt_signature(token).map_err(|e| AuthError::InvalidToken(e))?;

    let claims = decode_jwt_auth_claims(token)
        .map_err(|e| AuthError::InvalidToken(e))?;

    let now = chrono::Utc::now().timestamp();
    if claims.exp <= now {
        return Err(AuthError::Expired);
    }

    // TRIAL: Restrict platform_admin to super-admin allowlist
    #[cfg(feature = "postgres")]
    if claims.is_platform_admin && !crate::server::trial_lifecycle::is_trial_super_admin(&claims.email) {
        tracing::warn!("Non-super-admin {} attempted platform_admin access", claims.email);
        // Strip admin privilege for this request
        let mut restricted = claims.clone();
        restricted.is_platform_admin = false;
        req.extensions_mut().insert(restricted);
        return Ok(next.run(req).await);
    }

    // TRIAL: Check trial status — block expired/suspended orgs
    #[cfg(feature = "postgres")]
    {
        use crate::server::state::AppState;
        if let Some(state) = req.extensions().get::<AppState>().cloned() {
            if let Some(pool) = state.pg_pool() {
                match crate::server::trial_lifecycle::check_trial_status(pool, &claims.organization_id).await {
                    Ok(()) => {} // active or converted — proceed
                    Err(reason) => {
                        // Grace period: allow GET requests (reads), block writes
                        let method = req.method().clone();
                        if reason.is_fully_blocked() {
                            return Err(AuthError::InvalidToken(reason.message().to_string()));
                        }
                        if reason.is_write_blocked() && method != axum::http::Method::GET {
                            return Err(AuthError::InvalidToken(reason.message().to_string()));
                        }
                    }
                }
            }
        }
    }

    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}
