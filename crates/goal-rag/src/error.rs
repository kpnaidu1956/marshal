//! Error types for the RAG system

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Result type alias for RAG operations
pub type Result<T> = std::result::Result<T, Error>;

/// RAG system errors
#[derive(Debug, Error)]
pub enum Error {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Validation error (input validation failures)
    #[error("Validation error: {0}")]
    Validation(String),

    /// Rate limited (too many requests)
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// Service unavailable (circuit breaker open or backpressure)
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// File parsing error
    #[error("Failed to parse file '{filename}': {message}")]
    FileParse { filename: String, message: String },

    /// Unsupported file type
    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),

    /// Embedding error
    #[error("Embedding generation failed: {0}")]
    Embedding(String),

    /// Vector database error
    #[error("Vector database error: {0}")]
    VectorDb(String),

    /// Ollama/LLM error
    #[error("LLM error: {0}")]
    Llm(String),

    /// Document not found
    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP request error
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    /// RuVector core error
    #[error("RuVector error: {0}")]
    RuVector(String),

    /// Forbidden (access denied by ACL)
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Create a file parse error
    pub fn file_parse(filename: impl Into<String>, message: impl Into<String>) -> Self {
        Self::FileParse {
            filename: filename.into(),
            message: message.into(),
        }
    }

    /// Create an embedding error
    pub fn embedding(message: impl Into<String>) -> Self {
        Self::Embedding(message.into())
    }

    /// Create a vector db error
    pub fn vector_db(message: impl Into<String>) -> Self {
        Self::VectorDb(message.into())
    }

    /// Create an LLM error
    pub fn llm(message: impl Into<String>) -> Self {
        Self::Llm(message.into())
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl From<ruvector_core::RuvectorError> for Error {
    fn from(err: ruvector_core::RuvectorError) -> Self {
        Error::RuVector(err.to_string())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            Error::Config(msg) => (StatusCode::BAD_REQUEST, "config_error", msg.clone()),
            Error::Validation(msg) => (StatusCode::BAD_REQUEST, "validation_error", msg.clone()),
            Error::RateLimited(msg) => (StatusCode::TOO_MANY_REQUESTS, "rate_limited", msg.clone()),
            Error::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", msg.clone()),
            Error::FileParse { filename, message } => (
                StatusCode::BAD_REQUEST,
                "parse_error",
                format!("Failed to parse '{}': {}", filename, message),
            ),
            Error::UnsupportedFileType(ext) => (
                StatusCode::BAD_REQUEST,
                "unsupported_type",
                format!("Unsupported file type: {}", ext),
            ),
            Error::Embedding(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "embedding_error", msg.clone())
            }
            Error::VectorDb(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "vector_db_error", msg.clone())
            }
            Error::Llm(msg) => (StatusCode::SERVICE_UNAVAILABLE, "llm_error", msg.clone()),
            Error::DocumentNotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("Document not found: {}", id),
            ),
            Error::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                "forbidden",
                msg.clone(),
            ),
            Error::Io(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "io_error",
                err.to_string(),
            ),
            Error::Json(err) => (StatusCode::BAD_REQUEST, "json_error", err.to_string()),
            Error::Http(err) => (
                StatusCode::BAD_GATEWAY,
                "http_error",
                err.to_string(),
            ),
            Error::RuVector(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "vector_error", msg.clone())
            }
            Error::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg.clone())
            }
        };

        let body = Json(json!({
            "error": {
                "type": error_type,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}
