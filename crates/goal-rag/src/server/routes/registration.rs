//! Self-service registration endpoints for trial platform.
//!
//! Public endpoints (no auth required):
//!   POST /api/trial/register    — Create org + first admin user
//!   POST /api/trial/verify-email — Confirm email verification token
//!   POST /api/trial/check-org   — Real-time org name/domain uniqueness
//!   POST /api/trial/check-domain — MX record validation
//!   POST /api/trial/join-request — Request to join existing org
//!   GET  /api/trial/eula        — Get current EULA
//!
//! Protected endpoint:
//!   GET  /api/trial/status      — Trial days remaining + quota usage

#[cfg(feature = "postgres")]
use axum::{extract::State, http::StatusCode, Json};
#[cfg(feature = "postgres")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "postgres")]
use crate::server::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
    pub org_name: String,
    pub org_display_name: Option<String>,
    pub eula_version_id: String,
    #[serde(default)]
    pub recaptcha_token: String,
}

#[cfg(feature = "postgres")]
#[derive(Serialize)]
pub struct RegisterResponse {
    pub token: String,
    pub user: RegisteredUser,
    pub organization_id: String,
    pub organization_name: String,
    pub trial_expires_at: String,
}

#[cfg(feature = "postgres")]
#[derive(Serialize)]
pub struct RegisteredUser {
    pub id: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub is_admin: bool,
}

#[cfg(feature = "postgres")]
#[derive(Serialize)]
pub struct ErrorResp {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[cfg(feature = "postgres")]
impl ErrorResp {
    fn simple(error: impl Into<String>) -> Self {
        Self { error: error.into(), org_name: None, message: None }
    }
}

#[cfg(feature = "postgres")]
type ApiError = (StatusCode, Json<ErrorResp>);

#[cfg(feature = "postgres")]
fn err(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(ErrorResp::simple(msg)))
}



// ---------------------------------------------------------------------------
// JWT helper (replicates auth.rs pattern — sign_jwt is private)
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Serialize)]
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

#[cfg(feature = "postgres")]
fn create_trial_jwt(user_id: &str, email: &str, org_id: &str) -> Result<String, String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use base64::Engine;

    let secret = std::env::var("POSTGREST_JWT_SECRET")
        .map_err(|_| "POSTGREST_JWT_SECRET not set".to_string())?;
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        role: "authenticated".into(),
        email: email.into(),
        user_id: user_id.into(),
        organization_id: org_id.into(),
        is_platform_admin: false,
        aud: "postgrest".into(),
        exp: now + 86400,
        iat: now,
    };
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(b"{\"alg\":\"HS256\",\"typ\":\"JWT\"}");
    let payload_json = serde_json::to_vec(&claims).map_err(|e| e.to_string())?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&payload_json);
    let signing_input = format!("{}.{}", header, payload);
    let mut mac = <Hmac<Sha256>>::new_from_slice(secret.as_bytes())
        .map_err(|e| e.to_string())?;
    mac.update(signing_input.as_bytes());
    let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(mac.finalize().into_bytes());
    Ok(format!("{}.{}.{}", header, payload, sig))
}

// ---------------------------------------------------------------------------
// All features × actions for admin role seeding
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
const ALL_FEATURES: &[&str] = &[
    "timekeeping", "roster", "reports", "audit", "approvals", "admin",
    "knowledge", "marshal", "analytics", "users", "groups", "tasks",
    "goals", "events", "documents",
];

#[cfg(feature = "postgres")]
const ALL_ACTIONS: &[&str] = &["read", "write", "delete", "admin"];

// ---------------------------------------------------------------------------
// reCAPTCHA server-side verification
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
async fn verify_recaptcha(token: &str) -> Result<(), String> {
    let secret = std::env::var("RECAPTCHA_SECRET_KEY").unwrap_or_default();
    if secret.is_empty() || secret == "REPLACE_ME" {
        // reCAPTCHA not configured — skip validation
        return Ok(());
    }
    if token.is_empty() {
        return Err("reCAPTCHA token required".into());
    }
    let client = reqwest::Client::new();
    let resp = client.post("https://www.google.com/recaptcha/api/siteverify")
        .form(&[("secret", secret.as_str()), ("response", token)])
        .send()
        .await
        .map_err(|e| format!("reCAPTCHA verification failed: {}", e))?;
    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("reCAPTCHA response parse failed: {}", e))?;
    let success = body.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let score = body.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if !success || score < 0.5 {
        return Err("reCAPTCHA verification failed".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// POST /api/trial/register
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let mut client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // --- Input validation ---
    let email = req.email.trim().to_lowercase();
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || !parts[1].contains('.') || parts[1].len() < 3 {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid email address"));
    }
    if req.password.len() < 8 || req.password.len() > 72 {
        return Err(err(StatusCode::BAD_REQUEST, "Password must be 8-72 characters"));
    }
    let org_name = req.org_name.trim().to_string();
    if org_name.len() < 3 || org_name.len() > 100 {
        return Err(err(StatusCode::BAD_REQUEST, "Organization name must be 3-100 characters"));
    }
    let first_name = req.first_name.trim().to_string();
    let last_name = req.last_name.trim().to_string();
    if first_name.is_empty() || last_name.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "First and last name are required"));
    }

    // --- reCAPTCHA verification ---
    if let Err(msg) = verify_recaptcha(&req.recaptcha_token).await {
        return Err(err(StatusCode::BAD_REQUEST, msg));
    }

    // --- Extract domain ---
    let domain = email.split('@').nth(1).unwrap_or("").to_string();
    if domain.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid email domain"));
    }

    // --- Rate limiting ---
    if let Some(pool_ref) = state.pg_pool() {
        if let Err(msg) = crate::server::trial_lifecycle::check_signup_rate_limit(
            pool_ref, "0.0.0.0", &domain
        ).await {
            return Err(err(StatusCode::TOO_MANY_REQUESTS, msg));
        }
    }

    // --- Blocked domain check ---
    let blocked = client.query_opt(
        "SELECT 1 FROM trial.blocked_domains WHERE domain = $1", &[&domain]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if blocked.is_some() {
        return Err(err(StatusCode::BAD_REQUEST, "This email domain is not allowed for registration"));
    }

    // --- Verify EULA version ---
    let eula_id: uuid::Uuid = req.eula_version_id.parse()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid EULA version ID"))?;
    let eula_exists = client.query_opt(
        "SELECT 1 FROM trial.eula_versions WHERE id = $1", &[&eula_id]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if eula_exists.is_none() {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid EULA version"));
    }

    // --- Hash password ---
    let password = req.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || {
        bcrypt::hash(&password, 12)
    }).await.map_err(|e| {
        tracing::error!("Password hashing task failed: {}", e);
        err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    })?.map_err(|e| {
        tracing::error!("bcrypt error: {}", e);
        err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    })?;

    // --- Transaction: uniqueness checks + create org + user + roles ---
    let txn = client.transaction().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // Org name uniqueness (inside transaction to prevent TOCTOU)
    let name_taken = txn.query_opt(
        "SELECT 1 FROM api.organizations WHERE LOWER(name) = LOWER($1)", &[&org_name]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if name_taken.is_some() {
        return Err(err(StatusCode::CONFLICT, "Organization name already taken"));
    }

    // Email domain collision (existing org) — inside transaction
    let domain_org = txn.query_opt(
        "SELECT name FROM api.organizations WHERE email_domain = $1", &[&domain]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if let Some(row) = domain_org {
        let existing_name: String = row.get(0);
        return Err((StatusCode::CONFLICT, Json(ErrorResp {
            error: "org_exists".into(),
            org_name: Some(existing_name),
            message: Some("An organization with this email domain already exists. Please request to join instead.".into()),
        })));
    }

    // Email uniqueness — inside transaction
    let email_taken = txn.query_opt(
        "SELECT 1 FROM api.users WHERE email = $1", &[&email]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if email_taken.is_some() {
        return Err(err(StatusCode::CONFLICT, "Email already registered"));
    }

    let display = req.org_display_name.as_deref().unwrap_or(&org_name);
    let org_row = txn.query_one(
        "INSERT INTO api.organizations (name, display_name, trial_started_at, trial_expires_at, trial_status, email_domain)
         VALUES ($1, $2, now(), now() + interval '90 days', 'active', $3)
         RETURNING id::text, trial_expires_at::text",
        &[&org_name, &display, &domain],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let org_id_str: String = org_row.get(0);
    let trial_expires: String = org_row.get(1);
    let org_id: uuid::Uuid = org_id_str.parse()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to parse org ID"))?;

    // Quotas
    txn.execute(
        "INSERT INTO trial.org_quotas (organization_id) VALUES ($1)",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // User
    let user_row = txn.query_one(
        "INSERT INTO api.users (organization_id, first_name, last_name, email, password_hash)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id::text",
        &[&org_id, &first_name, &last_name, &email, &password_hash],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let user_id_str: String = user_row.get(0);
    let user_id: uuid::Uuid = user_id_str.parse()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to parse user ID"))?;

    // Role: admin
    let role_row = txn.query_one(
        "INSERT INTO api.roles (organization_id, name, description)
         VALUES ($1, 'admin', 'Full administrative access')
         RETURNING id",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let role_id: uuid::Uuid = role_row.get(0);

    // Seed all permissions for admin role
    for feature in ALL_FEATURES {
        for action in ALL_ACTIONS {
            txn.execute(
                "INSERT INTO api.role_permissions (role_id, feature, action, organization_id)
                 VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
                &[&role_id, feature, action, &org_id],
            ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
        }
    }

    // Assign admin role to user
    txn.execute(
        "INSERT INTO api.user_roles (user_id, role, organization_id)
         VALUES ($1, 'admin', $2)",
        &[&user_id, &org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // EULA acceptance
    txn.execute(
        "INSERT INTO trial.eula_acceptances (user_id, organization_id, eula_version_id)
         VALUES ($1, $2, $3)",
        &[&user_id, &org_id, &eula_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // Update quota
    txn.execute(
        "UPDATE trial.org_quotas SET current_users = 1 WHERE organization_id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    txn.commit().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // --- Send verification email ---
    let verify_token = uuid::Uuid::new_v4().to_string();
    let _ = client.execute(
        "INSERT INTO trial.email_verifications (email, token, purpose) VALUES ($1, $2, 'signup')",
        &[&email, &verify_token],
    ).await;
    // Send email asynchronously (don't block response)
    if let Some(resend) = crate::server::notifications::ResendClient::from_env() {
        let email_clone = email.clone();
        let token_clone = verify_token.clone();
        tokio::spawn(async move {
            let _ = resend.send_verification_email(&email_clone, &token_clone).await;
        });
    }

    // --- Record successful signup attempt ---
    if let Some(pool_ref) = state.pg_pool() {
        crate::server::trial_lifecycle::record_signup_attempt(
            pool_ref, "0.0.0.0", &email, &domain, "success"
        ).await;
    }

    // --- Generate JWT ---
    let token = create_trial_jwt(&user_id_str, &email, &org_id_str)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    tracing::info!("New trial org registered: {} ({}), user: {}", org_name, org_id_str, email);

    Ok((StatusCode::CREATED, Json(RegisterResponse {
        token,
        user: RegisteredUser {
            id: user_id_str,
            email,
            first_name,
            last_name,
            is_admin: true,
        },
        organization_id: org_id.to_string(),
        organization_name: org_name,
        trial_expires_at: trial_expires,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/trial/verify-email
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[cfg(feature = "postgres")]
pub async fn verify_email(
    State(state): State<AppState>,
    Json(req): Json<VerifyEmailRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let row = client.query_opt(
        "UPDATE trial.email_verifications SET verified_at = now()
         WHERE token = $1 AND verified_at IS NULL AND expires_at > now()
         RETURNING email",
        &[&req.token],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    match row {
        Some(r) => {
            let email: String = r.get(0);
            let domain = email.split('@').nth(1).unwrap_or("");
            if !domain.is_empty() {
                client.execute(
                    "UPDATE api.organizations SET domain_verified = true WHERE email_domain = $1",
                    &[&domain.to_string()],
                ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
            }
            Ok(Json(serde_json::json!({"verified": true, "email": email})))
        }
        None => Err(err(StatusCode::NOT_FOUND, "Invalid or expired verification token")),
    }
}

// ---------------------------------------------------------------------------
// POST /api/trial/check-org
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct CheckOrgRequest {
    pub name: Option<String>,
    pub email_domain: Option<String>,
}

#[cfg(feature = "postgres")]
pub async fn check_org(
    State(state): State<AppState>,
    Json(req): Json<CheckOrgRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let mut name_available = true;
    let mut domain_available = true;
    let mut existing_org_name: Option<String> = None;

    if let Some(name) = &req.name {
        let row = client.query_opt(
            "SELECT 1 FROM api.organizations WHERE LOWER(name) = LOWER($1)", &[name]
        ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
        name_available = row.is_none();
    }

    if let Some(domain) = &req.email_domain {
        let row = client.query_opt(
            "SELECT name FROM api.organizations WHERE email_domain = $1", &[domain]
        ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
        if let Some(r) = row {
            domain_available = false;
            existing_org_name = Some(r.get(0));
        }
    }

    Ok(Json(serde_json::json!({
        "name_available": name_available,
        "domain_available": domain_available,
        "existing_org_name": existing_org_name,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/trial/check-domain
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct CheckDomainRequest {
    pub domain: String,
}

#[cfg(feature = "postgres")]
pub async fn check_domain(
    State(state): State<AppState>,
    Json(req): Json<CheckDomainRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let domain = req.domain.trim().to_lowercase();

    // Blocked check
    let blocked = client.query_opt(
        "SELECT 1 FROM trial.blocked_domains WHERE domain = $1", &[&domain]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // MX record check via host command
    let domain_clone = domain.clone();
    let has_mx = tokio::task::spawn_blocking(move || {
        std::process::Command::new("host")
            .arg("-t").arg("MX").arg(&domain_clone)
            .output()
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                o.status.success() && stdout.contains("mail is handled by")
            })
            .unwrap_or(false)
    }).await.unwrap_or(false);

    Ok(Json(serde_json::json!({
        "valid": !blocked.is_some() && has_mx,
        "blocked": blocked.is_some(),
        "has_mx": has_mx,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/trial/join-request
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
#[derive(Deserialize)]
pub struct JoinRequestBody {
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub password: String,
    #[serde(default)]
    pub recaptcha_token: String,
}

#[cfg(feature = "postgres")]
pub async fn join_request(
    State(state): State<AppState>,
    Json(req): Json<JoinRequestBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let email = req.email.trim().to_lowercase();
    let domain = email.split('@').nth(1).unwrap_or("");
    if domain.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid email"));
    }
    let domain = domain.to_string();

    // --- reCAPTCHA verification ---
    if let Err(msg) = verify_recaptcha(&req.recaptcha_token).await {
        return Err(err(StatusCode::BAD_REQUEST, msg));
    }

    // --- Rate limiting ---
    if let Some(pool_ref) = state.pg_pool() {
        if let Err(msg) = crate::server::trial_lifecycle::check_signup_rate_limit(
            pool_ref, "0.0.0.0", &domain
        ).await {
            return Err(err(StatusCode::TOO_MANY_REQUESTS, msg));
        }
    }

    // Find org by domain
    let org_row = client.query_opt(
        "SELECT id, name FROM api.organizations WHERE email_domain = $1",
        &[&domain],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    let org_row = org_row.ok_or_else(|| err(StatusCode::NOT_FOUND, "No organization found for this email domain"))?;
    let org_id: uuid::Uuid = org_row.get(0);
    let org_name: String = org_row.get(1);

    // Check email not already registered
    let existing = client.query_opt(
        "SELECT 1 FROM api.users WHERE email = $1", &[&email]
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if existing.is_some() {
        return Err(err(StatusCode::CONFLICT, "Email already registered"));
    }

    // Check no pending request
    let pending = client.query_opt(
        "SELECT 1 FROM trial.join_requests WHERE requester_email = $1 AND status = 'pending'",
        &[&email],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;
    if pending.is_some() {
        return Err(err(StatusCode::CONFLICT, "You already have a pending join request"));
    }

    // Hash password
    let password = req.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || {
        bcrypt::hash(&password, 12)
    }).await.map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))?
      .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))?;

    let first_name = req.first_name.trim().to_string();
    let last_name = req.last_name.trim().to_string();

    // Insert join request
    client.execute(
        "INSERT INTO trial.join_requests (organization_id, requester_email, requester_first_name, requester_last_name, requester_password_hash)
         VALUES ($1, $2, $3, $4, $5)",
        &[&org_id, &email, &first_name, &last_name, &password_hash],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    // Record successful join request attempt
    if let Some(pool_ref) = state.pg_pool() {
        crate::server::trial_lifecycle::record_signup_attempt(
            pool_ref, "0.0.0.0", &email, &domain, "success"
        ).await;
    }

    tracing::info!("Join request from {} for org {} ({})", email, org_name, org_id);

    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "status": "pending",
        "organization_name": org_name,
        "message": "Your request has been sent to the organization administrator. You'll be notified when it's reviewed.",
    }))))
}

// ---------------------------------------------------------------------------
// GET /api/trial/eula
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn get_eula(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let row = client.query_opt(
        "SELECT id::text, version, content FROM trial.eula_versions ORDER BY effective_at DESC LIMIT 1",
        &[],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    match row {
        Some(r) => {
            let id: String = r.get(0);
            let version: String = r.get(1);
            let content: String = r.get(2);
            Ok(Json(serde_json::json!({ "id": id, "version": version, "content": content })))
        }
        None => Err(err(StatusCode::NOT_FOUND, "No EULA version found")),
    }
}

// ---------------------------------------------------------------------------
// GET /api/trial/status (requires auth)
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub async fn get_trial_status(
    State(state): State<AppState>,
    axum::Extension(claims): axum::Extension<super::auth::AuthClaims>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pg_pool().ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "Database not available"))?;
    let client = pool.get().await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let org_id: uuid::Uuid = claims.organization_id.parse()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "Invalid organization ID in token"))?;

    let org_row = client.query_opt(
        "SELECT trial_started_at::text, trial_expires_at::text, trial_status,
                EXTRACT(DAY FROM trial_expires_at - now())::int as days_remaining
         FROM api.organizations WHERE id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let org = org_row.ok_or_else(|| err(StatusCode::NOT_FOUND, "Organization not found"))?;

    let quota_row = client.query_opt(
        "SELECT max_users, max_storage_bytes, max_documents, current_users, current_storage_bytes, current_documents
         FROM trial.org_quotas WHERE organization_id = $1",
        &[&org_id],
    ).await.map_err(|e| { tracing::error!("DB error: {}", e); err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error") })?;

    let quotas = quota_row.map(|q| serde_json::json!({
        "max_users": q.get::<_, i32>(0),
        "max_storage_bytes": q.get::<_, i64>(1),
        "max_documents": q.get::<_, i32>(2),
        "current_users": q.get::<_, i32>(3),
        "current_storage_bytes": q.get::<_, i64>(4),
        "current_documents": q.get::<_, i32>(5),
    }));

    Ok(Json(serde_json::json!({
        "trial_started_at": org.get::<_, Option<String>>(0),
        "trial_expires_at": org.get::<_, Option<String>>(1),
        "trial_status": org.get::<_, Option<String>>(2),
        "days_remaining": org.get::<_, Option<i32>>(3).unwrap_or(0),
        "quotas": quotas,
    })))
}

// ---------------------------------------------------------------------------
// Fallbacks when postgres feature is not enabled
// ---------------------------------------------------------------------------

#[cfg(not(feature = "postgres"))]
pub async fn register() -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Registration requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn verify_email() -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn check_org() -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn check_domain() -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn join_request() -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn get_eula() -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
#[cfg(not(feature = "postgres"))]
pub async fn get_trial_status() -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({"error": "Requires PostgreSQL"})))
}
