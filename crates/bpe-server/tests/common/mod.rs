use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use axum::Router;
use base64::Engine;
use bpe_core::{auth::JwtSecret, config::BpeConfig, db::PgPool, metrics::Metrics};
use bpe_server::{build_router, AppState};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const TEST_JWT_SECRET: &str = "test-secret-for-bpe-integration-tests-32bytes!";
pub const TEST_ORG_SLUG: &str = "test-org";
pub const TEST_ORG_UUID: &str = "00000000-0000-0000-0000-000000000001";
pub const TEST_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
pub const TEST_EMAIL: &str = "test@example.com";

/// Generate a valid JWT token for tests.
pub fn make_jwt(
    user_id: &str,
    email: &str,
    organization_id: &str,
    is_admin: bool,
    secret: &str,
) -> String {
    let header = base64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let exp = chrono::Utc::now().timestamp() + 3600; // 1 hour from now
    let claims = serde_json::json!({
        "user_id": user_id,
        "email": email,
        "organization_id": organization_id,
        "is_platform_admin": is_admin,
        "exp": exp
    });
    let payload = base64url_encode(claims.to_string().as_bytes());
    let signing_input = format!("{header}.{payload}");

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signing_input.as_bytes());
    let sig = base64url_encode(&mac.finalize().into_bytes());

    format!("{signing_input}.{sig}")
}

/// Generate an expired JWT token for auth tests.
pub fn make_expired_jwt(secret: &str) -> String {
    let header = base64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let exp = chrono::Utc::now().timestamp() - 3600; // 1 hour ago
    let claims = serde_json::json!({
        "user_id": TEST_USER_ID,
        "email": TEST_EMAIL,
        "organization_id": TEST_ORG_UUID,
        "is_platform_admin": false,
        "exp": exp
    });
    let payload = base64url_encode(claims.to_string().as_bytes());
    let signing_input = format!("{header}.{payload}");

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signing_input.as_bytes());
    let sig = base64url_encode(&mac.finalize().into_bytes());

    format!("{signing_input}.{sig}")
}

/// Default valid test token.
pub fn test_token() -> String {
    make_jwt(TEST_USER_ID, TEST_EMAIL, TEST_ORG_UUID, false, TEST_JWT_SECRET)
}

/// Admin test token (bypasses org checks).
pub fn admin_token() -> String {
    make_jwt(TEST_USER_ID, TEST_EMAIL, TEST_ORG_UUID, true, TEST_JWT_SECRET)
}

fn base64url_encode(input: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input)
}

/// Build a test BpeConfig pointing to the local test database.
/// Uses BPE_TEST_PG_* env vars, falling back to defaults suitable for local dev.
fn test_config() -> BpeConfig {
    BpeConfig {
        pg_host: std::env::var("BPE_TEST_PG_HOST").unwrap_or_else(|_| "localhost".into()),
        pg_port: std::env::var("BPE_TEST_PG_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5432),
        pg_dbname: std::env::var("BPE_TEST_PG_DBNAME").unwrap_or_else(|_| "goalrag".into()),
        pg_user: std::env::var("BPE_TEST_PG_USER").unwrap_or_else(|_| "postgres".into()),
        pg_password: std::env::var("BPE_TEST_PG_PASSWORD").unwrap_or_default(),
        pg_pool_max_size: 2,
        listen_host: "127.0.0.1".into(),
        listen_port: 0,
        jwt_secret: TEST_JWT_SECRET.into(),
        rag_base_url: "http://localhost:8080".into(),
        credential_encryption_key: None,
        cors_origins: vec![],
    }
}

/// Build a test app router connected to the test database.
/// Returns None if the database is not available.
pub async fn test_app() -> Option<Router> {
    let config = test_config();
    let pool = match PgPool::new(&config).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping integration test — DB not available: {e}");
            return None;
        }
    };

    // Run migrations
    if let Ok(client) = pool.get().await {
        let _ = bpe_core::db::migrations::run_migrations(&client).await;
    }

    let metrics = Metrics::new();
    let state = AppState::new(pool, config, metrics);
    let jwt_secret = JwtSecret(TEST_JWT_SECRET.into());

    Some(build_router(state, jwt_secret))
}

/// Send a request and return (status, json body).
pub async fn send(app: &Router, req: Request<Body>) -> (u16, serde_json::Value) {
    let response: Response = tower::ServiceExt::oneshot(app.clone(), req)
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::String(
            String::from_utf8_lossy(&body).to_string(),
        ))
    };
    (status, json)
}

/// Build a GET request with auth header.
pub fn get_req(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Build a POST request with JSON body and auth header.
pub fn post_json(uri: &str, token: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Build a PUT request with JSON body and auth header.
pub fn put_json(uri: &str, token: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Build a DELETE request with auth header.
pub fn delete_req(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Macro to skip a test if the database is not available.
#[macro_export]
macro_rules! require_db {
    ($app:expr) => {
        match $app {
            Some(a) => a,
            None => {
                eprintln!("SKIPPED: database not available");
                return;
            }
        }
    };
}
