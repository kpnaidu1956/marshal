//! Bucket-based file storage API endpoints
//!
//! Provides generic file storage for frontend attachments (task files, etc.)
//! Separate from document ingestion - these files are stored but not processed for RAG.
//!
//! Security features:
//! - Organization-based multi-tenancy (files isolated by org_id)
//! - Bucket whitelist validation
//! - Comprehensive path sanitization (traversal, injection protection)
//! - Content-Length validation before reading into memory
//! - Rate limiting via production controls

use axum::{
    extract::{Multipart, Path, Query, State},
    http::HeaderMap,
    Json,
    response::Response,
};
#[cfg(feature = "gcp")]
use axum::{
    body::Body,
    http::{header, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::server::state::AppState;
#[cfg(feature = "gcp")]
use crate::validation::validate_organization_id;

/// Allowed storage buckets for frontend files
#[cfg(feature = "gcp")]
const ALLOWED_BUCKETS: &[&str] = &[
    "task-attachments",
    "user-avatars",
    "organization-logos",
    "message-attachments",
    "goal-attachments",
];

/// Maximum file size for storage uploads (50 MB)
#[cfg(feature = "gcp")]
const MAX_STORAGE_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Maximum message/path length
#[cfg(feature = "gcp")]
const MAX_PATH_LENGTH: usize = 500;

/// Response for file upload
#[derive(Debug, Serialize)]
pub struct StorageUploadResponse {
    pub success: bool,
    pub url: String,
    pub path: String,
    pub size: u64,
    pub content_type: String,
    pub organization_id: String,
}

/// Response for file deletion
#[derive(Debug, Serialize)]
pub struct StorageDeleteResponse {
    pub success: bool,
    pub message: String,
}

/// Response for file listing
#[derive(Debug, Serialize)]
pub struct StorageListResponse {
    pub success: bool,
    pub bucket: String,
    pub organization_id: String,
    pub files: Vec<StorageFileInfo>,
    pub count: usize,
}

/// File info for listing
#[derive(Debug, Serialize)]
pub struct StorageFileInfo {
    pub path: String,
    pub size: u64,
    pub content_type: String,
    pub updated_at: Option<String>,
}

/// Query parameters for listing
#[derive(Debug, Deserialize)]
pub struct ListStorageQuery {
    pub organization_id: String,
    #[serde(default)]
    pub prefix: Option<String>,
}

/// Validate bucket name
#[cfg(feature = "gcp")]
fn validate_bucket(bucket: &str) -> Result<()> {
    if !ALLOWED_BUCKETS.contains(&bucket) {
        return Err(Error::Validation(format!(
            "Invalid bucket '{}'. Allowed buckets: {}",
            bucket,
            ALLOWED_BUCKETS.join(", ")
        )));
    }
    Ok(())
}

/// Comprehensive path validation and sanitization
/// Protects against:
/// - Path traversal (../, encoded variants)
/// - Null byte injection
/// - Backslash injection
/// - Control characters
/// - Empty path segments
/// - Excessive depth/length
#[cfg(feature = "gcp")]
fn validate_storage_path(path: &str) -> Result<String> {
    // Reject empty paths
    if path.is_empty() {
        return Err(Error::Validation("Path cannot be empty".to_string()));
    }

    // Reject null bytes
    if path.contains('\0') {
        return Err(Error::Validation("Invalid path: contains null byte".to_string()));
    }

    // Reject backslashes (Windows-style paths)
    if path.contains('\\') {
        return Err(Error::Validation("Invalid path: contains backslash".to_string()));
    }

    // Reject control characters
    if path.chars().any(|c| c.is_control()) {
        return Err(Error::Validation("Invalid path: contains control characters".to_string()));
    }

    // Reject path traversal (including URL-decoded variants)
    if path.contains("..") {
        return Err(Error::Validation("Invalid path: contains path traversal".to_string()));
    }

    // Reject leading slash
    if path.starts_with('/') {
        return Err(Error::Validation("Invalid path: cannot start with /".to_string()));
    }

    // Reject empty segments (consecutive slashes)
    if path.contains("//") {
        return Err(Error::Validation("Invalid path: contains empty segment".to_string()));
    }

    // Limit path depth
    let depth = path.matches('/').count();
    if depth > 5 {
        return Err(Error::Validation("Path too deep (max 5 levels)".to_string()));
    }

    // Limit total path length
    if path.len() > MAX_PATH_LENGTH {
        return Err(Error::Validation(format!(
            "Path too long (max {} characters)",
            MAX_PATH_LENGTH
        )));
    }

    // Validate each segment doesn't start with a dot (hidden files)
    for segment in path.split('/') {
        if segment.starts_with('.') && segment != "." {
            return Err(Error::Validation("Invalid path: hidden files not allowed".to_string()));
        }
    }

    Ok(path.to_string())
}

/// Sanitize filename for Content-Disposition header
/// Removes characters that could cause header injection
#[cfg(feature = "gcp")]
fn sanitize_filename_for_header(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// Build public URL from request headers or fallback
#[cfg(feature = "gcp")]
fn build_public_url(headers: &HeaderMap, bucket: &str, org_id: &str, path: &str) -> String {
    // Try to get host from X-Forwarded-Host or Host header
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("rags.marshal.ai");

    // Determine protocol (assume HTTPS in production)
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("https");

    format!("{}://{}/api/storage/{}/{}/{}", proto, host, bucket, org_id, path)
}

/// POST /api/storage/upload - Upload a file to storage bucket
///
/// Request: multipart/form-data with fields:
/// - organization_id: Organization ID (required for multi-tenancy)
/// - bucket: Storage bucket name (e.g., "task-attachments")
/// - path: Path within bucket (e.g., "tasks/{task_id}/{filename}")
/// - file: The file binary
///
/// Response:
/// ```json
/// {
///   "success": true,
///   "url": "https://rags.marshal.ai/api/storage/task-attachments/org123/tasks/abc123/document.pdf",
///   "path": "tasks/abc123/document.pdf",
///   "size": 1024000,
///   "content_type": "application/pdf",
///   "organization_id": "org123"
/// }
/// ```
#[cfg(feature = "gcp")]
pub async fn upload_storage_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<StorageUploadResponse>> {
    // Check rate limiting
    if !state.production_controls().allow_upload() {
        return Err(Error::RateLimited(
            "Too many upload requests. Please try again later.".to_string(),
        ));
    }

    let mut file_data: Option<Vec<u8>> = None;
    let mut bucket: Option<String> = None;
    let mut path: Option<String> = None;
    let mut organization_id: Option<String> = None;
    let mut content_type: Option<String> = None;

    // Parse multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        Error::Internal(format!("Failed to read multipart field: {}", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                // Get content type from field
                if content_type.is_none() {
                    content_type = field.content_type().map(|s| s.to_string());
                }

                // Check Content-Length header before reading (if available)
                // This provides early rejection for obviously too-large files
                if let Some(size) = field.headers().get("content-length")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    if size > MAX_STORAGE_FILE_SIZE {
                        return Err(Error::Validation(format!(
                            "File size ({} MB) exceeds maximum allowed ({} MB)",
                            size / (1024 * 1024),
                            MAX_STORAGE_FILE_SIZE / (1024 * 1024)
                        )));
                    }
                }

                file_data = Some(field.bytes().await.map_err(|e| {
                    Error::Internal(format!("Failed to read file data: {}", e))
                })?.to_vec());
            }
            "bucket" => {
                bucket = Some(field.text().await.map_err(|e| {
                    Error::Internal(format!("Failed to read bucket: {}", e))
                })?);
            }
            "path" => {
                path = Some(field.text().await.map_err(|e| {
                    Error::Internal(format!("Failed to read path: {}", e))
                })?);
            }
            "organization_id" => {
                organization_id = Some(field.text().await.map_err(|e| {
                    Error::Internal(format!("Failed to read organization_id: {}", e))
                })?);
            }
            _ => {}
        }
    }

    // Validate required fields
    let file_data = file_data.ok_or_else(|| {
        Error::Validation("Missing 'file' in multipart form".to_string())
    })?;
    let bucket = bucket.ok_or_else(|| {
        Error::Validation("Missing 'bucket' in multipart form".to_string())
    })?;
    let path = path.ok_or_else(|| {
        Error::Validation("Missing 'path' in multipart form".to_string())
    })?;
    let organization_id = organization_id.ok_or_else(|| {
        Error::Validation("Missing 'organization_id' in multipart form".to_string())
    })?;
    validate_organization_id(&organization_id)?;

    // Validate inputs
    validate_bucket(&bucket)?;
    let validated_path = validate_storage_path(&path)?;

    let file_size = file_data.len() as u64;

    // Final size check (in case Content-Length was missing or inaccurate)
    if file_size > MAX_STORAGE_FILE_SIZE {
        return Err(Error::Validation(format!(
            "File size ({} MB) exceeds maximum allowed ({} MB)",
            file_size / (1024 * 1024),
            MAX_STORAGE_FILE_SIZE / (1024 * 1024)
        )));
    }

    if file_size == 0 {
        return Err(Error::Validation("File is empty".to_string()));
    }

    // Determine content type
    let content_type = content_type.unwrap_or_else(|| {
        mime_guess::from_path(&path)
            .first_or_octet_stream()
            .to_string()
    });

    // Get GCS document store
    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

    // Build GCS path: storage/{org_id}/{bucket}/{path}
    let gcs_path = format!("storage/{}/{}/{}", organization_id, bucket, validated_path);

    // Upload to GCS
    document_store.store_storage_file(&gcs_path, file_data, &content_type).await?;

    // Build public URL from request headers
    let public_url = build_public_url(&headers, &bucket, &organization_id, &validated_path);

    tracing::info!(
        org_id = %organization_id,
        bucket = %bucket,
        path = %validated_path,
        size = file_size,
        "Stored file in storage bucket"
    );

    Ok(Json(StorageUploadResponse {
        success: true,
        url: public_url,
        path: validated_path,
        size: file_size,
        content_type,
        organization_id,
    }))
}

/// GET /api/storage/:bucket/:org_id/*path - Download a file from storage
#[cfg(feature = "gcp")]
pub async fn download_storage_file(
    State(state): State<AppState>,
    Path((bucket, org_id, path)): Path<(String, String, String)>,
) -> Result<Response> {
    // Validate inputs
    validate_bucket(&bucket)?;
    validate_organization_id(&org_id)?;
    let validated_path = validate_storage_path(&path)?;

    // Get GCS document store
    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

    // Build GCS path
    let gcs_path = format!("storage/{}/{}/{}", org_id, bucket, validated_path);

    // Download from GCS
    let (data, content_type) = document_store.get_storage_file(&gcs_path).await?;

    // Get filename from path for content-disposition (sanitized)
    let filename = validated_path.rsplit('/').next().unwrap_or("file");
    let safe_filename = sanitize_filename_for_header(filename);

    tracing::info!(
        org_id = %org_id,
        bucket = %bucket,
        path = %validated_path,
        size = data.len(),
        "Downloaded file from storage bucket"
    );

    // Build response with appropriate headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", safe_filename)
        )
        .header(header::CACHE_CONTROL, "public, max-age=31536000")
        .header(header::CONTENT_LENGTH, data.len())
        .body(Body::from(data))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// DELETE /api/storage/:bucket/:org_id/*path - Delete a file from storage
#[cfg(feature = "gcp")]
pub async fn delete_storage_file(
    State(state): State<AppState>,
    Path((bucket, org_id, path)): Path<(String, String, String)>,
) -> Result<Json<StorageDeleteResponse>> {
    // Validate inputs
    validate_bucket(&bucket)?;
    validate_organization_id(&org_id)?;
    let validated_path = validate_storage_path(&path)?;

    // Get GCS document store
    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

    // Build GCS path
    let gcs_path = format!("storage/{}/{}/{}", org_id, bucket, validated_path);

    // Delete from GCS
    document_store.delete_storage_file(&gcs_path).await?;

    tracing::info!(
        org_id = %org_id,
        bucket = %bucket,
        path = %validated_path,
        "Deleted file from storage bucket"
    );

    Ok(Json(StorageDeleteResponse {
        success: true,
        message: format!("File deleted: {}/{}/{}", bucket, org_id, validated_path),
    }))
}

/// GET /api/storage/:bucket/list - List files in a storage bucket
#[cfg(feature = "gcp")]
pub async fn list_storage_files(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<ListStorageQuery>,
) -> Result<Json<StorageListResponse>> {
    // Validate inputs
    validate_bucket(&bucket)?;
    validate_organization_id(&params.organization_id)?;

    // Validate optional prefix if provided
    if let Some(ref prefix) = params.prefix {
        validate_storage_path(prefix)?;
    }

    // Get GCS document store
    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

    // List files from GCS using bucket and org_id
    let files = document_store.list_storage_files(&bucket, &params.organization_id).await?;

    // Convert to response format, filtering by prefix if specified
    let file_infos: Vec<StorageFileInfo> = files
        .into_iter()
        .filter(|f| {
            // If prefix is specified, filter by it
            params.prefix.as_ref().map_or(true, |p| f.path.starts_with(p))
        })
        .map(|f| StorageFileInfo {
            path: f.path,
            size: f.size,
            content_type: f.content_type,
            updated_at: f.updated_at,
        })
        .collect();

    let count = file_infos.len();

    tracing::info!(
        org_id = %params.organization_id,
        bucket = %bucket,
        count = count,
        "Listed storage files"
    );

    Ok(Json(StorageListResponse {
        success: true,
        bucket,
        organization_id: params.organization_id,
        files: file_infos,
        count,
    }))
}

// ===== Non-GCP stubs =====

#[cfg(not(feature = "gcp"))]
pub async fn upload_storage_file(
    _state: State<AppState>,
    _headers: HeaderMap,
    _multipart: Multipart,
) -> Result<Json<StorageUploadResponse>> {
    Err(Error::ServiceUnavailable("Storage API requires GCP feature to be enabled".to_string()))
}

#[cfg(not(feature = "gcp"))]
pub async fn download_storage_file(
    _state: State<AppState>,
    _path: Path<(String, String, String)>,
) -> Result<Response> {
    Err(Error::ServiceUnavailable("Storage API requires GCP feature to be enabled".to_string()))
}

#[cfg(not(feature = "gcp"))]
pub async fn delete_storage_file(
    _state: State<AppState>,
    _path: Path<(String, String, String)>,
) -> Result<Json<StorageDeleteResponse>> {
    Err(Error::ServiceUnavailable("Storage API requires GCP feature to be enabled".to_string()))
}

#[cfg(not(feature = "gcp"))]
pub async fn list_storage_files(
    _state: State<AppState>,
    _path: Path<String>,
    _query: Query<ListStorageQuery>,
) -> Result<Json<StorageListResponse>> {
    Err(Error::ServiceUnavailable("Storage API requires GCP feature to be enabled".to_string()))
}
