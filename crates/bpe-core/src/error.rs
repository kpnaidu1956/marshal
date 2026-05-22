use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum BpeError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for BpeError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            BpeError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            BpeError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            BpeError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            BpeError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            BpeError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            BpeError::Database(msg) => {
                tracing::error!("Database error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into())
            }
            BpeError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into())
            }
        };

        let body = axum::Json(json!({ "error": message }));
        (status, body).into_response()
    }
}

impl From<deadpool_postgres::PoolError> for BpeError {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        BpeError::Database(format!("Pool error: {e}"))
    }
}

impl From<tokio_postgres::Error> for BpeError {
    fn from(e: tokio_postgres::Error) -> Self {
        if let Some(db_err) = e.as_db_error() {
            let code = db_err.code().code();
            let msg = db_err.message().to_string();
            let detail = db_err.detail().unwrap_or("").to_string();

            // Class 23: integrity constraint violations
            match code {
                // 23505 = unique_violation
                "23505" => return BpeError::Conflict(format!("Duplicate entry: {msg}. {detail}")),
                // 23503 = foreign_key_violation
                "23503" => return BpeError::BadRequest(format!("Referenced record not found: {msg}. {detail}")),
                // 23502 = not_null_violation
                "23502" => return BpeError::BadRequest(format!("Required field missing: {msg}")),
                // 23514 = check_violation
                "23514" => return BpeError::BadRequest(format!("Constraint check failed: {msg}. {detail}")),
                _ => {}
            }

            BpeError::Database(format!("PostgreSQL error: {}: {} ({})", db_err.severity(), msg, detail))
        } else {
            BpeError::Database(format!("PostgreSQL error: {e}"))
        }
    }
}
