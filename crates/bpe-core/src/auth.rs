use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Claims extracted from a validated JWT token.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthClaims {
    pub user_id: String,
    pub email: String,
    pub organization_id: String,
    #[serde(default)]
    pub is_platform_admin: bool,
    pub exp: i64,
}

/// Auth error type with JSON response.
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
    Expired,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authorization token"),
            AuthError::InvalidToken(_) => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::Expired => (StatusCode::UNAUTHORIZED, "Token expired"),
        };
        let body = axum::Json(serde_json::json!({ "error": msg }));
        (status, body).into_response()
    }
}

/// Verify JWT HMAC-SHA256 signature and decode claims in a single pass.
/// Splits the token once, verifies the signature, then decodes the payload.
fn verify_and_decode(token: &str, secret: &str) -> Result<AuthClaims, AuthError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(AuthError::InvalidToken("Malformed JWT".into()));
    }

    // Verify signature
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let signature_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        parts[2],
    )
    .map_err(|_| AuthError::InvalidToken("Bad signature encoding".into()))?;

    let secret_bytes = secret.as_bytes();
    let mut mac = HmacSha256::new_from_slice(secret_bytes)
        .map_err(|_| AuthError::InvalidToken("Bad secret".into()))?;
    mac.update(signing_input.as_bytes());
    mac.verify_slice(&signature_bytes)
        .map_err(|_| AuthError::InvalidToken("Signature mismatch".into()))?;

    // Decode claims from the already-split payload
    let payload_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        parts[1],
    )
    .map_err(|_| AuthError::InvalidToken("Bad payload encoding".into()))?;

    serde_json::from_slice::<AuthClaims>(&payload_bytes)
        .map_err(|e| AuthError::InvalidToken(format!("Bad claims: {e}")))
}

/// Axum middleware that validates JWT Bearer tokens.
///
/// Extracts the Bearer token from the Authorization header,
/// verifies the HMAC-SHA256 signature using the shared secret,
/// checks expiration, and inserts `AuthClaims` into request extensions.
pub async fn require_auth(mut request: Request, next: Next) -> Result<Response, AuthError> {
    let jwt_secret = request
        .extensions()
        .get::<JwtSecret>()
        .ok_or_else(|| AuthError::InvalidToken("JWT secret not configured".into()))?;

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::MissingToken)?;

    let claims = verify_and_decode(token, &jwt_secret.0)?;

    let now = chrono::Utc::now().timestamp();
    if claims.exp <= now {
        return Err(AuthError::Expired);
    }

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

/// Wrapper to inject JWT secret into request extensions via layer.
#[derive(Clone)]
pub struct JwtSecret(pub String);
