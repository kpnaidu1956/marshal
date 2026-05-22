//! File status and tracking API endpoints

use axum::{
    extract::{Path, Query, State},
    Json,
};
#[cfg(feature = "gcp")]
use axum::extract::Multipart;
use serde::{Deserialize, Serialize};
#[cfg(feature = "gcp")]
use sha2::{Sha256, Digest};

use crate::error::{Error, Result};
use crate::server::state::{AppState, FileRegistryStats};
use crate::storage::SyncStatus;
use crate::types::{
    FileCheckItem, FileCheckRequest, FileCheckResponse, FileCheckResult, FileCheckSummary,
    FileRecord, FileRecordStatus, FileRecordSummary, FileUploadAdvice,
};
#[cfg(feature = "gcp")]
use crate::types::{FileUploadResponse, FileUploadInfo, UploadAction};
#[cfg(feature = "gcp")]
use crate::processing::{GcsFileRef, GcsProcessingOptions, GcsJob};
use crate::validation::{validate_organization_id, sanitize_filename, validate_batch_size};

/// Query parameters for listing files
#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
    /// Filter by status: success, failed, skipped, all
    #[serde(default = "default_status")]
    pub status: String,
    /// Limit results
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
    /// Sort by: filename, date, size
    #[serde(default = "default_sort")]
    pub sort: String,
    /// Sort order: asc, desc
    #[serde(default = "default_order")]
    pub order: String,
}

/// Query parameters for file operations requiring org context
#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
}

fn default_status() -> String {
    "all".to_string()
}

fn default_limit() -> usize {
    100
}

fn default_sort() -> String {
    "date".to_string()
}

fn default_order() -> String {
    "desc".to_string()
}

/// Response for file list
#[derive(Debug, Serialize)]
pub struct FileListResponse {
    /// Files in the list
    pub files: Vec<FileRecordSummary>,
    /// Total count (before pagination)
    pub total: usize,
    /// Current offset
    pub offset: usize,
    /// Current limit
    pub limit: usize,
    /// Statistics
    pub stats: FileRegistryStats,
}

/// Response for failed files list
#[derive(Debug, Serialize)]
pub struct FailedFilesResponse {
    /// Failed files with details
    pub files: Vec<FailedFileDetail>,
    /// Total count
    pub total: usize,
    /// Suggestions for fixing
    pub suggestions: Vec<String>,
}

/// Detail for a failed file
#[derive(Debug, Serialize)]
pub struct FailedFileDetail {
    pub filename: String,
    pub file_type: String,
    pub file_size: u64,
    pub error_message: String,
    pub failed_at_stage: String,
    pub last_attempt: String,
    pub upload_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
}

/// GET /api/files - List all tracked files for an organization
pub async fn list_files(
    State(state): State<AppState>,
    Query(params): Query<ListFilesQuery>,
) -> Result<Json<FileListResponse>> {
    // Validate organization_id to prevent path traversal
    validate_organization_id(&params.organization_id)?;

    let mut records: Vec<FileRecord> = match params.status.as_str() {
        "success" => state.list_successful_files(&params.organization_id),
        "failed" => state.list_failed_files(&params.organization_id),
        "skipped" => state.list_skipped_files(&params.organization_id),
        _ => state.list_file_records(&params.organization_id),
    };

    let total = records.len();

    // Sort
    match params.sort.as_str() {
        "filename" => records.sort_by(|a, b| a.filename.cmp(&b.filename)),
        "size" => records.sort_by(|a, b| a.file_size.cmp(&b.file_size)),
        _ => records.sort_by(|a, b| b.last_processed_at.cmp(&a.last_processed_at)),
    }

    if params.order == "asc" {
        records.reverse();
    }

    // Paginate
    let records: Vec<FileRecordSummary> = records
        .into_iter()
        .skip(params.offset)
        .take(params.limit)
        .map(|r| FileRecordSummary::from(&r))
        .collect();

    let stats = state.file_registry_stats(&params.organization_id);

    Ok(Json(FileListResponse {
        files: records,
        total,
        offset: params.offset,
        limit: params.limit,
        stats,
    }))
}

/// GET /api/files/:filename - Get specific file status
pub async fn get_file_status(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<FileRecord>> {
    // Validate inputs to prevent path traversal
    validate_organization_id(&query.organization_id)?;
    let sanitized_filename = sanitize_filename(&filename)?;

    let record = state
        .get_file_record(&sanitized_filename)
        .ok_or_else(|| Error::DocumentNotFound(format!("File '{}' not found in registry", sanitized_filename)))?;

    // Verify file belongs to the requested organization
    if record.organization_id != query.organization_id {
        return Err(Error::DocumentNotFound(format!(
            "File '{}' not found in organization {}",
            filename, query.organization_id
        )));
    }

    Ok(Json(record))
}

/// POST /api/files/check - Check status of files before upload
///
/// Requires organization_id in request body for multi-tenancy support.
pub async fn check_files(
    State(state): State<AppState>,
    Json(request): Json<FileCheckRequest>,
) -> Result<Json<FileCheckResponse>> {
    // Validate organization_id
    validate_organization_id(&request.organization_id)?;

    // Validate batch size (max 1000 files per request)
    validate_batch_size(request.files.len(), 1000)?;

    let mut results = Vec::new();
    let mut needs_upload = 0;
    let mut can_skip = 0;
    let mut should_retry = 0;
    let total_checked = request.files.len();

    for item in request.files {
        // Sanitize each filename before checking
        let sanitized_filename = match sanitize_filename(&item.filename) {
            Ok(name) => name,
            Err(_) => {
                // Skip invalid filenames
                results.push(FileCheckResult {
                    filename: item.filename.clone(),
                    advice: FileUploadAdvice::Upload, // Will fail at upload with proper error
                    existing_record: None,
                });
                needs_upload += 1;
                continue;
            }
        };

        let sanitized_item = FileCheckItem {
            filename: sanitized_filename,
            file_size: item.file_size,
            content_hash: item.content_hash.clone(),
        };

        let (advice, existing_record) = check_single_file(&state, &sanitized_item, &request.organization_id);

        match &advice {
            FileUploadAdvice::Upload => needs_upload += 1,
            FileUploadAdvice::Skip { .. } => can_skip += 1,
            FileUploadAdvice::Retry { .. } => should_retry += 1,
        }

        results.push(FileCheckResult {
            filename: item.filename,
            advice,
            existing_record,
        });
    }

    Ok(Json(FileCheckResponse {
        files: results,
        summary: FileCheckSummary {
            total_checked,
            needs_upload,
            can_skip,
            should_retry,
        },
    }))
}

/// Check a single file for upload status
fn check_single_file(state: &AppState, item: &FileCheckItem, organization_id: &str) -> (FileUploadAdvice, Option<FileRecordSummary>) {
    // First check if we have a record by filename
    if let Some(record) = state.get_file_record(&item.filename) {
        // Verify the record belongs to this organization
        if record.organization_id != organization_id {
            // File exists but belongs to different org - treat as new
            return (FileUploadAdvice::Upload, None);
        }
        let summary = FileRecordSummary::from(&record);

        match record.status {
            FileRecordStatus::Success => {
                // Check if content hash matches (if provided)
                if let Some(ref hash) = item.content_hash {
                    if &record.content_hash == hash {
                        // Same content, skip
                        return (
                            FileUploadAdvice::Skip {
                                reason: "File unchanged (same content hash)".to_string(),
                                existing_document_id: record.document_id,
                            },
                            Some(summary),
                        );
                    } else {
                        // Content changed, upload for update
                        return (FileUploadAdvice::Upload, Some(summary));
                    }
                }

                // No hash provided, check file size
                if record.file_size == item.file_size {
                    return (
                        FileUploadAdvice::Skip {
                            reason: "File unchanged (same size)".to_string(),
                            existing_document_id: record.document_id,
                        },
                        Some(summary),
                    );
                }

                // Different size, likely modified
                (FileUploadAdvice::Upload, Some(summary))
            }
            FileRecordStatus::Failed => {
                // Previously failed, recommend retry
                (
                    FileUploadAdvice::Retry {
                        previous_error: record.error_message.unwrap_or_else(|| "Unknown error".to_string()),
                    },
                    Some(summary),
                )
            }
            FileRecordStatus::Skipped => {
                // Was skipped (duplicate), skip again unless content changed
                if let Some(ref hash) = item.content_hash {
                    if &record.content_hash != hash {
                        return (FileUploadAdvice::Upload, Some(summary));
                    }
                }
                (
                    FileUploadAdvice::Skip {
                        reason: "Previously skipped (duplicate)".to_string(),
                        existing_document_id: record.document_id,
                    },
                    Some(summary),
                )
            }
            FileRecordStatus::Processing => {
                // Currently being processed
                (
                    FileUploadAdvice::Skip {
                        reason: "Currently being processed".to_string(),
                        existing_document_id: None,
                    },
                    Some(summary),
                )
            }
        }
    } else if let Some(ref hash) = item.content_hash {
        // No record by filename, check by content hash
        if let Some(record) = state.get_file_record_by_hash(hash) {
            let summary = FileRecordSummary::from(&record);
            return (
                FileUploadAdvice::Skip {
                    reason: format!("Same content exists as '{}'", record.filename),
                    existing_document_id: record.document_id,
                },
                Some(summary),
            );
        }
        // New file
        (FileUploadAdvice::Upload, None)
    } else {
        // New file (no record found)
        (FileUploadAdvice::Upload, None)
    }
}

/// GET /api/files/failed - List failed files with details for an organization
pub async fn list_failed_files(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<FailedFilesResponse>> {
    // Validate organization_id
    validate_organization_id(&query.organization_id)?;

    let failed = state.list_failed_files(&query.organization_id);
    let total = failed.len();

    let files: Vec<FailedFileDetail> = failed
        .iter()
        .map(|record| {
            let error_msg = record.error_message.as_deref().unwrap_or("Unknown error");
            let stage = record.failed_at_stage.as_deref().unwrap_or("unknown");
            let suggested_action = suggest_action_for_failure(error_msg, stage);

            FailedFileDetail {
                filename: record.filename.clone(),
                file_type: record.file_type.display_name().to_string(),
                file_size: record.file_size,
                error_message: error_msg.to_string(),
                failed_at_stage: stage.to_string(),
                last_attempt: record.last_processed_at.to_rfc3339(),
                upload_count: record.upload_count,
                suggested_action,
            }
        })
        .collect();

    // Generate suggestions based on error patterns
    let mut suggestions = Vec::new();
    let error_messages: Vec<&str> = files.iter()
        .map(|f| f.error_message.as_str())
        .collect();

    if error_messages.iter().any(|e| e.contains("OCR") || e.contains("tesseract")) {
        suggestions.push("Install tesseract-ocr for better scanned document support".to_string());
    }
    if error_messages.iter().any(|e| e.contains("LibreOffice") || e.contains("libreoffice")) {
        suggestions.push("Install LibreOffice for legacy .doc/.ppt/.xls support".to_string());
    }
    if error_messages.iter().any(|e| e.contains("timeout")) {
        suggestions.push("Some files may be too large or complex. Try splitting large documents.".to_string());
    }
    if error_messages.iter().any(|e| e.contains("No text content")) {
        suggestions.push("Some documents may be image-only. Ensure OCR tools are installed.".to_string());
    }
    if error_messages.iter().any(|e| e.contains("rate limit") || e.contains("429")) {
        suggestions.push("Rate limiting detected. Processing will resume automatically.".to_string());
    }

    Ok(Json(FailedFilesResponse {
        files,
        total,
        suggestions,
    }))
}

/// Suggest action based on error message
fn suggest_action_for_failure(error: &str, stage: &str) -> Option<String> {
    let error_lower = error.to_lowercase();

    if error_lower.contains("no text content") || error_lower.contains("empty") {
        Some("Document may be image-based. Ensure OCR is installed and retry.".to_string())
    } else if error_lower.contains("timeout") {
        Some("Document took too long to process. Try splitting into smaller files.".to_string())
    } else if error_lower.contains("unsupported") || error_lower.contains("unknown file type") {
        Some("Convert to a supported format (PDF, DOCX, XLSX, TXT).".to_string())
    } else if error_lower.contains("libreoffice") {
        Some("Install LibreOffice or convert to modern Office format.".to_string())
    } else if error_lower.contains("ocr") || error_lower.contains("tesseract") {
        Some("Install tesseract-ocr or use a non-scanned version.".to_string())
    } else if error_lower.contains("corrupt") || error_lower.contains("invalid") {
        Some("File may be corrupted. Try re-saving or exporting to a new file.".to_string())
    } else if error_lower.contains("password") || error_lower.contains("encrypted") {
        Some("Remove password protection from the document.".to_string())
    } else if stage == "embedding" {
        Some("Embedding service error. Retry upload - transient issue.".to_string())
    } else {
        None
    }
}

/// DELETE /api/files/failed - Clear all failed file records for an organization
pub async fn clear_failed_files(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<ClearFailedResponse>> {
    // Validate organization_id
    validate_organization_id(&query.organization_id)?;

    let cleared = state.clear_failed_files(&query.organization_id).await;
    Ok(Json(ClearFailedResponse {
        cleared,
        message: format!("Cleared {} failed file records for organization '{}'. You can now retry uploading these files.", cleared, query.organization_id),
    }))
}

#[derive(Debug, Serialize)]
pub struct ClearFailedResponse {
    pub cleared: usize,
    pub message: String,
}

/// DELETE /api/files/:filename - Remove a specific file record
pub async fn delete_file_record(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<DeleteFileResponse>> {
    // Validate inputs
    validate_organization_id(&query.organization_id)?;
    let sanitized_filename = sanitize_filename(&filename)?;

    // First verify the file belongs to this organization
    if let Some(record) = state.get_file_record(&sanitized_filename) {
        if record.organization_id != query.organization_id {
            return Err(Error::DocumentNotFound(format!(
                "File '{}' not found in organization {}",
                sanitized_filename, query.organization_id
            )));
        }
    }

    match state.remove_file_record(&sanitized_filename) {
        Some(record) => Ok(Json(DeleteFileResponse {
            filename: record.filename,
            message: "File record removed. You can re-upload this file.".to_string(),
        })),
        None => Err(Error::DocumentNotFound(format!("File '{}' not found in registry", sanitized_filename))),
    }
}

#[derive(Debug, Serialize)]
pub struct DeleteFileResponse {
    pub filename: String,
    pub message: String,
}

/// GET /api/files/stats - Get file registry statistics for an organization
pub async fn file_stats(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> Json<FileStatsResponse> {
    let stats = state.file_registry_stats(&query.organization_id);
    let failed = state.list_failed_files(&query.organization_id);

    // Group failures by error type (max ~8 distinct categories from categorize_error)
    let mut error_types: std::collections::HashMap<String, usize> = std::collections::HashMap::with_capacity(8);
    for record in &failed {
        let error = record.error_message.as_deref().unwrap_or("Unknown");
        let error_type = categorize_error(error);
        *error_types.entry(error_type).or_insert(0) += 1;
    }

    Json(FileStatsResponse {
        total_files: stats.total,
        successful: stats.success,
        failed: stats.failed,
        skipped: stats.skipped,
        success_rate: if stats.total > 0 {
            (stats.success as f32 / stats.total as f32) * 100.0
        } else {
            0.0
        },
        error_breakdown: error_types,
    })
}

fn categorize_error(error: &str) -> String {
    let error_lower = error.to_lowercase();
    if error_lower.contains("timeout") {
        "Timeout".to_string()
    } else if error_lower.contains("no text") || error_lower.contains("empty") {
        "Empty/No Text".to_string()
    } else if error_lower.contains("ocr") || error_lower.contains("tesseract") {
        "OCR Required".to_string()
    } else if error_lower.contains("libreoffice") {
        "LibreOffice Required".to_string()
    } else if error_lower.contains("unsupported") || error_lower.contains("unknown") {
        "Unsupported Format".to_string()
    } else if error_lower.contains("rate") || error_lower.contains("429") {
        "Rate Limited".to_string()
    } else if error_lower.contains("embedding") {
        "Embedding Error".to_string()
    } else {
        "Other".to_string()
    }
}

#[derive(Debug, Serialize)]
pub struct FileStatsResponse {
    pub total_files: usize,
    pub successful: usize,
    pub failed: usize,
    pub skipped: usize,
    pub success_rate: f32,
    pub error_breakdown: std::collections::HashMap<String, usize>,
}

// ============================================================================
// GCS Sync Endpoints
// ============================================================================

/// Request for syncing from GCS
#[cfg(feature = "gcp")]
#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
}

/// POST /api/files/sync - Sync file registry from GCS bucket
#[cfg(feature = "gcp")]
pub async fn sync_from_gcs(
    State(state): State<AppState>,
    Json(request): Json<SyncRequest>,
) -> Result<Json<SyncResponse>> {
    // Validate organization_id
    validate_organization_id(&request.organization_id)?;

    tracing::info!("Syncing files from GCS for organization: {}", request.organization_id);
    let (synced, failed) = state.sync_from_gcs_for_org(&request.organization_id).await?;

    Ok(Json(SyncResponse {
        success: true,
        files_synced: synced,
        files_failed: failed,
        message: format!(
            "Synced {} files from GCS bucket for org '{}' ({} failed)",
            synced, request.organization_id, failed
        ),
        sync_status: state.get_sync_status().await,
    }))
}

/// GET /api/files/sync/status - Get last sync status for an organization
pub async fn get_sync_status(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<SyncStatusResponse>> {
    // Validate organization_id
    validate_organization_id(&query.organization_id)?;

    let sync_status = state.get_sync_status().await;
    let db_stats = state.database_stats().await;

    Ok(Json(SyncStatusResponse {
        last_sync: sync_status,
        database_stats: db_stats,
    }))
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub success: bool,
    pub files_synced: usize,
    pub files_failed: usize,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_status: Option<SyncStatus>,
}

#[derive(Debug, Serialize)]
pub struct SyncStatusResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<SyncStatus>,
    pub database_stats: crate::storage::FileRegistryDbStats,
}

/// GET /api/files/gcs-counts - Get file counts from GCS bucket for an organization
#[cfg(feature = "gcp")]
pub async fn get_gcs_counts(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<GcsCountsResponse>> {
    // Validate organization_id
    validate_organization_id(&query.organization_id)?;

    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

    // Get counts for the specific organization
    let (originals, plaintext) = document_store.get_file_counts_for_org(&query.organization_id).await?;

    Ok(Json(GcsCountsResponse {
        originals_count: originals,
        plaintext_count: plaintext,
        failed_estimate: originals.saturating_sub(plaintext),
    }))
}

#[derive(Debug, Serialize)]
pub struct GcsCountsResponse {
    /// Number of original files in GCS
    pub originals_count: usize,
    /// Number of plaintext files in GCS (successfully processed)
    pub plaintext_count: usize,
    /// Estimated number of failed files (originals without plaintext)
    pub failed_estimate: usize,
}

// ============================================================================
// Re-vectorization Endpoints
// ============================================================================

/// Request for re-vectorizing chunks to Vertex AI
#[derive(Debug, Deserialize)]
pub struct RevectorizeRequest {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
    /// Specific document ID to re-vectorize (if None, re-vectorizes all for the org)
    #[serde(default)]
    pub document_id: Option<uuid::Uuid>,
    /// Batch size for processing (default 100, max 500)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

fn default_batch_size() -> usize {
    100
}

/// Response for re-vectorization
#[derive(Debug, Serialize)]
pub struct RevectorizeResponse {
    pub success: bool,
    pub message: String,
    pub chunks_processed: usize,
    pub chunks_failed: usize,
    pub batches_sent: usize,
}

/// POST /api/files/revectorize - Re-vectorize existing chunks to Vertex AI
///
/// This endpoint reads chunks from SQLite, generates embeddings, and upserts them
/// to Vertex AI Vector Search. Use this to fix documents that were imported but
/// not properly vectorized.
#[cfg(feature = "gcp")]
pub async fn revectorize_chunks(
    State(state): State<AppState>,
    Json(request): Json<RevectorizeRequest>,
) -> Result<Json<RevectorizeResponse>> {
    // Validate inputs
    validate_organization_id(&request.organization_id)?;
    validate_batch_size(request.batch_size, 500)?;

    tracing::info!("Starting re-vectorization for org '{}' (document_id: {:?}, batch_size: {})",
        request.organization_id, request.document_id, request.batch_size);

    // Get chunks from database, filtered by organization
    let chunks = if let Some(doc_id) = request.document_id {
        // Verify document belongs to this organization before processing
        if let Some(doc) = state.get_document(&doc_id) {
            if doc.organization_id.as_deref() != Some(&request.organization_id) {
                return Err(Error::Validation(format!("Document {} not found in organization {}", doc_id, request.organization_id)));
            }
        }
        state.database().get_chunks_for_document(&doc_id).await?
    } else {
        // Get all documents for this org, then collect their chunks
        let org_docs = state.database().list_documents_by_org(&request.organization_id).await?;
        let mut all_chunks = Vec::new();
        for doc in &org_docs {
            if let Ok(doc_chunks) = state.database().get_chunks_for_document(&doc.id).await {
                all_chunks.extend(doc_chunks);
            }
        }
        all_chunks
    };

    let total_chunks = chunks.len();
    tracing::info!("Found {} chunks to re-vectorize", total_chunks);

    if total_chunks == 0 {
        return Ok(Json(RevectorizeResponse {
            success: true,
            message: "No chunks found to re-vectorize".to_string(),
            chunks_processed: 0,
            chunks_failed: 0,
            batches_sent: 0,
        }));
    }

    let mut chunks_processed = 0;
    let mut chunks_failed = 0;
    let mut batches_sent = 0;

    // Process in batches
    for batch in chunks.chunks(request.batch_size) {
        let mut embedded_chunks = Vec::with_capacity(batch.len());

        for chunk in batch {
            // Generate embedding for the chunk
            match state.embedding_provider().embed(&chunk.content).await {
                Ok(embedding) => {
                    let mut embedded_chunk = chunk.clone();
                    embedded_chunk.embedding = embedding;
                    embedded_chunk.metadata.insert("organization_id".to_string(), serde_json::Value::String(request.organization_id.clone()));
                    embedded_chunks.push(embedded_chunk);
                    chunks_processed += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to embed chunk {}: {}", chunk.id, e);
                    chunks_failed += 1;
                }
            }
        }

        // Insert batch into Vertex AI
        if !embedded_chunks.is_empty() {
            match state.vector_store_provider().insert_chunks(&embedded_chunks).await {
                Ok(_) => {
                    batches_sent += 1;
                    tracing::info!("Batch {} sent: {} chunks to Vertex AI",
                        batches_sent, embedded_chunks.len());
                }
                Err(e) => {
                    tracing::error!("Failed to insert batch to Vertex AI: {}", e);
                    chunks_failed += embedded_chunks.len();
                    chunks_processed -= embedded_chunks.len();
                }
            }
        }
    }

    let message = format!(
        "Re-vectorization complete: {} chunks processed, {} failed, {} batches sent to Vertex AI",
        chunks_processed, chunks_failed, batches_sent
    );
    tracing::info!("{}", message);

    Ok(Json(RevectorizeResponse {
        success: chunks_failed == 0,
        message,
        chunks_processed,
        chunks_failed,
        batches_sent,
    }))
}

/// Request for migrating GCS files to organization folders
#[cfg(feature = "gcp")]
#[derive(Debug, Deserialize)]
pub struct MigrateGcsRequest {
    /// Organization ID to migrate files to (required for multi-tenancy)
    pub organization_id: String,
    /// Limit number of documents to migrate (for testing)
    pub limit: Option<usize>,
    /// Dry run - don't actually move files, just report what would be moved
    #[serde(default)]
    pub dry_run: bool,
}

/// Response for GCS migration
#[cfg(feature = "gcp")]
#[derive(Debug, Serialize)]
pub struct MigrateGcsResponse {
    pub success: bool,
    pub message: String,
    pub documents_processed: usize,
    pub originals_moved: usize,
    pub plaintext_moved: usize,
    pub metadata_moved: usize,
    pub already_migrated: usize,
    pub errors: Vec<String>,
    pub dry_run: bool,
}

/// POST /api/files/migrate-gcs - Migrate GCS files to organization-specific folders
///
/// Moves files from old flat structure:
///   originals/{doc_id}.{ext} -> originals/{org_id}/{doc_id}.{ext}
///   plaintext/{doc_id}.txt -> plaintext/{org_id}/{doc_id}.txt
///
/// This is needed for existing documents after enabling multi-tenancy.
#[cfg(feature = "gcp")]
pub async fn migrate_gcs_files(
    State(state): State<AppState>,
    Json(request): Json<MigrateGcsRequest>,
) -> Result<Json<MigrateGcsResponse>> {
    let dry_run = request.dry_run;
    validate_organization_id(&request.organization_id)?;

    tracing::info!(
        "Starting GCS migration (org_id: {}, limit: {:?}, dry_run: {})",
        request.organization_id, request.limit, dry_run
    );

    // Get document store
    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not configured".to_string()))?;

    // Get documents for this organization only
    let documents = state.database().list_documents_by_org(&request.organization_id).await?;
    let total_docs = documents.len();
    tracing::info!("Found {} documents for org '{}' in database", total_docs, request.organization_id);

    let mut documents_processed = 0;
    let mut originals_moved = 0;
    let mut plaintext_moved = 0;
    let mut metadata_moved = 0;
    let mut already_migrated = 0;
    let mut errors: Vec<String> = Vec::new();

    // Apply limit if specified
    let docs_to_process: Vec<_> = if let Some(limit) = request.limit {
        documents.into_iter().take(limit).collect()
    } else {
        documents
    };

    for doc in docs_to_process {
        // Use the validated organization ID from the request
        let org_id = request.organization_id.clone();

        if dry_run {
            // Just log what would happen
            tracing::info!(
                "[DRY RUN] Would migrate document {} ({}) to org '{}'",
                doc.id, doc.filename, org_id
            );
            documents_processed += 1;
            continue;
        }

        // Perform migration
        match document_store.migrate_document_to_org(&doc.id, &doc.filename, &org_id).await {
            Ok(result) => {
                documents_processed += 1;

                if result.original_moved {
                    originals_moved += 1;
                }
                if result.plaintext_moved {
                    plaintext_moved += 1;
                }
                if result.metadata_moved {
                    metadata_moved += 1;
                }

                // If nothing was moved but no error, files are already in place
                if !result.original_moved && !result.plaintext_moved && !result.metadata_moved {
                    already_migrated += 1;
                }

                if let Some(err) = result.error {
                    errors.push(format!("{}: {}", doc.filename, err));
                }

                // Update document metadata with new URIs if moved
                if result.original_moved || result.plaintext_moved {
                    // Note: The document metadata (original_uri, plaintext_uri) in the database
                    // will be updated on next access or can be manually updated
                    tracing::info!(
                        "Migrated {} to org '{}' (orig: {}, txt: {}, meta: {})",
                        doc.filename, org_id,
                        result.original_moved, result.plaintext_moved, result.metadata_moved
                    );
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", doc.filename, e));
                tracing::error!("Failed to migrate {}: {}", doc.filename, e);
            }
        }
    }

    let message = if dry_run {
        format!(
            "[DRY RUN] Would migrate {} documents to organization folders",
            documents_processed
        )
    } else {
        format!(
            "Migration complete: {} documents processed, {} originals moved, {} plaintext moved, {} metadata moved, {} already migrated, {} errors",
            documents_processed, originals_moved, plaintext_moved, metadata_moved, already_migrated, errors.len()
        )
    };

    tracing::info!("{}", message);

    Ok(Json(MigrateGcsResponse {
        success: errors.is_empty(),
        message,
        documents_processed,
        originals_moved,
        plaintext_moved,
        metadata_moved,
        already_migrated,
        errors,
        dry_run,
    }))
}

// ============================================================================
// New Filename-Based Upload Endpoint
// ============================================================================

/// POST /api/files/upload - Upload a file with original filename to GCS
///
/// Two-phase response:
/// 1. Immediate: Returns when file is uploaded to GCS
/// 2. Async: Processing status available via /api/jobs/{job_id}
///
/// Duplicate handling:
/// - Same content (hash match): Overwrites existing file
/// - Different content: Creates versioned file (file_v2.pdf, file_v3.pdf)
#[cfg(feature = "gcp")]
pub async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<FileUploadResponse>> {
    // Check rate limit for uploads
    if !state.production_controls().allow_upload() {
        return Err(Error::RateLimited(
            "Upload rate limit exceeded. Please try again later.".to_string()
        ));
    }

    // Check backpressure (job queue depth)
    if !state.production_controls().reserve_job_slot() {
        return Err(Error::ServiceUnavailable(
            "Processing queue is full. Please wait for current jobs to complete.".to_string()
        ));
    }

    // Try to acquire upload concurrency slot
    let _upload_permit = match state.production_controls().try_acquire_upload_slot() {
        Some(permit) => permit,
        None => {
            // Release job slot since we can't proceed
            state.production_controls().release_job_slot();
            return Err(Error::ServiceUnavailable(
                "Maximum concurrent uploads reached. Please try again shortly.".to_string()
            ));
        }
    };

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut organization_id: Option<String> = None;

    // Parse multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        Error::Internal(format!("Failed to read multipart field: {}", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                // Get filename from content disposition if not provided separately
                if filename.is_none() {
                    filename = field.file_name().map(|s| s.to_string());
                }
                file_data = Some(field.bytes().await.map_err(|e| {
                    Error::Internal(format!("Failed to read file data: {}", e))
                })?.to_vec());
            }
            "filename" => {
                let value = field.text().await.map_err(|e| {
                    Error::Internal(format!("Failed to read filename: {}", e))
                })?;
                if !value.is_empty() {
                    filename = Some(value);
                }
            }
            "organization_id" => {
                organization_id = Some(field.text().await.map_err(|e| {
                    Error::Internal(format!("Failed to read organization_id: {}", e))
                })?);
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // Validate required fields
    let file_data = file_data.ok_or_else(|| {
        Error::Validation("Missing 'file' in multipart form".to_string())
    })?;
    let raw_filename = filename.ok_or_else(|| {
        Error::Validation("Missing 'filename' - provide via form field or content-disposition".to_string())
    })?;
    let organization_id = organization_id.ok_or_else(|| {
        Error::Validation("Missing 'organization_id' in multipart form".to_string())
    })?;

    // Validate and sanitize inputs
    validate_organization_id(&organization_id)?;
    let filename = sanitize_filename(&raw_filename)?;

    let file_size = file_data.len() as u64;

    // Validate file size (max 100MB)
    const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
    if file_size > MAX_FILE_SIZE {
        return Err(Error::Validation(format!(
            "File size ({} MB) exceeds maximum allowed (100 MB)",
            file_size / (1024 * 1024)
        )));
    }

    if file_size == 0 {
        return Err(Error::Validation("File is empty".to_string()));
    }

    // Calculate content hash (SHA-256)
    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let content_hash = format!("sha256:{:x}", hasher.finalize());

    tracing::info!(
        "Processing file upload: {} for org '{}' ({} bytes, hash: {})",
        filename, organization_id, file_size, content_hash
    );

    // Get GCS document store
    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

    // Check for existing file and determine action
    let (final_filename, action) = match document_store.get_file_metadata_by_name(&filename, &organization_id).await {
        Ok(Some(existing_meta)) => {
            if existing_meta.content_hash == content_hash {
                // Same content - will replace/overwrite
                tracing::info!("File {} exists with same hash, will replace", filename);
                (filename.clone(), UploadAction::Replaced)
            } else {
                // Different content - create versioned filename
                let versioned_name = document_store.find_next_version(&filename, &organization_id).await?;
                tracing::info!("File {} exists with different hash, creating version: {}", filename, versioned_name);
                (versioned_name.clone(), UploadAction::Versioned { new_filename: versioned_name })
            }
        }
        Ok(None) => {
            // New file
            tracing::info!("File {} is new, creating", filename);
            (filename.clone(), UploadAction::Created)
        }
        Err(e) => {
            tracing::warn!("Error checking existing file {}: {}, treating as new", filename, e);
            (filename.clone(), UploadAction::Created)
        }
    };

    // Upload to GCS (with concurrency limiting)
    let _gcs_permit = state.production_controls().acquire_gcs_slot().await;
    let gcs_path = match document_store.store_document_by_name(
        &final_filename,
        &organization_id,
        &file_data,
        &content_hash,
    ).await {
        Ok(path) => {
            state.production_controls().record_success();
            path
        }
        Err(e) => {
            state.production_controls().record_failure();
            state.production_controls().release_job_slot();
            return Err(e);
        }
    };

    tracing::info!("File uploaded to GCS: {}", gcs_path);

    // Create GCS job for async processing
    let gcs_file_ref = GcsFileRef {
        filename: final_filename.clone(),
        organization_id: organization_id.clone(),
        gcs_path: gcs_path.clone(),
        content_hash: content_hash.clone(),
        file_size,
    };

    let gcs_job = GcsJob::new(gcs_file_ref, GcsProcessingOptions::default());
    let job_id = gcs_job.id;

    // Submit job for tracking (the actual processing is spawned below)
    state.job_queue().submit_gcs_job(gcs_job.clone()).await;

    // Spawn async processing task
    let process_state = state.clone();
    let process_filename = final_filename.clone();
    let process_org_id = organization_id.clone();
    let process_job_id = job_id;

    tokio::spawn(async move {
        let result = process_gcs_file(
            process_state.clone(),
            process_job_id,
            &process_filename,
            &process_org_id,
        ).await;

        // Release the job slot when processing completes (success or failure)
        process_state.production_controls().release_job_slot();

        if let Err(e) = result {
            tracing::error!("Failed to process file {}: {}", process_filename, e);
        }
    });

    // Return immediate response
    Ok(Json(FileUploadResponse {
        success: true,
        gcs_uploaded: true,
        file: FileUploadInfo {
            filename: final_filename,
            organization_id,
            gcs_path,
            content_hash,
            file_size,
            action,
        },
        job_id: Some(job_id),
        processing_status_url: Some(format!("/api/jobs/{}", job_id)),
        error: None,
    }))
}

/// Process a file that has already been uploaded to GCS
#[cfg(feature = "gcp")]
async fn process_gcs_file(
    state: AppState,
    job_id: uuid::Uuid,
    filename: &str,
    organization_id: &str,
) -> Result<()> {
    use crate::processing::JobStatus;
    use crate::ingestion::{TextChunker, ParsedDocument, PageContent};
    use crate::types::FileType;

    tracing::info!("Starting GCS file processing for {} (job {})", filename, job_id);

    // Update job status to processing
    state.job_queue().update_status(job_id, JobStatus::Processing, None);

    // Get document store
    let document_store = state.document_store()
        .ok_or_else(|| Error::Internal("GCS document store not available".to_string()))?;

    // Download file from GCS (with concurrency limiting)
    let _gcs_permit = state.production_controls().acquire_gcs_slot().await;
    let file_data = match document_store.get_document_by_name(filename, organization_id).await {
        Ok(data) => {
            state.production_controls().record_success();
            data
        }
        Err(e) => {
            state.production_controls().record_failure();
            let err = format!("Failed to download file from GCS: {}", e);
            state.job_queue().update_status(job_id, JobStatus::Failed, Some(err.clone()));
            return Err(Error::Internal(err));
        }
    };
    drop(_gcs_permit); // Release GCS slot after download completes
    let file_size = file_data.len() as u64;
    tracing::info!("Downloaded {} bytes from GCS for {}", file_data.len(), filename);

    // Determine file type
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    let file_type = FileType::from_extension(&ext);
    tracing::info!("File type for {}: {:?}", filename, file_type);

    // Calculate content hash early (needed for file registry)
    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let content_hash = format!("sha256:{:x}", hasher.finalize());

    // Extract text content
    let text_content = match state.extract_text_from_bytes(filename, &file_data).await {
        Ok(content) => content,
        Err(e) => {
            let err = format!("Failed to extract text: {}", e);
            state.record_file_failed(organization_id, filename, &content_hash, file_size, file_type.clone(), &err, "extraction", Some(job_id));
            state.job_queue().update_status(job_id, JobStatus::Failed, Some(err.clone()));
            return Err(Error::Internal(err));
        }
    };

    if text_content.trim().is_empty() {
        let err = "No text content extracted from file";
        state.record_file_failed(organization_id, filename, &content_hash, file_size, file_type.clone(), err, "extraction", Some(job_id));
        state.job_queue().update_status(job_id, JobStatus::Failed, Some(err.to_string()));
        return Err(Error::Internal(err.to_string()));
    }

    tracing::info!("Extracted {} characters from {}", text_content.len(), filename);

    // Store plaintext in GCS
    match document_store.store_plain_text_by_name(filename, organization_id, &text_content).await {
        Ok(_) => {},
        Err(e) => {
            let err = format!("Failed to store plain text in GCS: {}", e);
            state.record_file_failed(organization_id, filename, &content_hash, file_size, file_type.clone(), &err, "storage", Some(job_id));
            state.job_queue().update_status(job_id, JobStatus::Failed, Some(err.clone()));
            return Err(Error::Internal(err));
        }
    }

    // Create document record
    let doc_id = uuid::Uuid::new_v4();

    let document = crate::types::Document {
        id: doc_id,
        organization_id: Some(organization_id.to_string()),
        filename: filename.to_string(),
        internal_filename: None,
        file_type: file_type.clone(),
        content_hash: content_hash.clone(),
        total_pages: None,
        total_chunks: 0, // Will update after chunking
        file_size,
        ingested_at: chrono::Utc::now(),
        metadata: std::collections::HashMap::new(),
    };

    // Chunk the text using TextChunker
    let config = state.config();
    let chunk_size = config.chunking.chunk_size;
    let chunk_overlap = config.chunking.chunk_overlap;

    let chunker = TextChunker::new(chunk_size, chunk_overlap);

    // Create a ParsedDocument for chunking
    let parsed = ParsedDocument {
        file_type: file_type.clone(),
        content: text_content.clone(),
        content_hash: content_hash.clone(),
        total_pages: Some(1),
        pages: vec![PageContent {
            page_number: 1,
            content: text_content,
            char_offset: 0,
        }],
        metadata: std::collections::HashMap::new(),
    };

    let chunks = chunker.chunk_document(&document, &parsed);
    tracing::info!("Created {} chunks for {}", chunks.len(), filename);

    // Embed chunks and inject organization_id into metadata
    let mut embedded_chunks = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        match state.embedding_provider().embed(&chunk.content).await {
            Ok(embedding) => {
                let mut embedded = chunk;
                embedded.embedding = embedding;
                embedded.metadata.insert("organization_id".to_string(), serde_json::Value::String(organization_id.to_string()));
                embedded_chunks.push(embedded);
            }
            Err(e) => {
                tracing::warn!("Failed to embed chunk: {}", e);
            }
        }
    }

    if embedded_chunks.is_empty() {
        let err = "Failed to embed any chunks";
        state.record_file_failed(organization_id, filename, &content_hash, file_size, file_type.clone(), err, "embedding", Some(job_id));
        state.job_queue().update_status(job_id, JobStatus::Failed, Some(err.to_string()));
        return Err(Error::Internal(err.to_string()));
    }

    // Store in vector database
    match state.vector_store_provider().insert_chunks(&embedded_chunks).await {
        Ok(_) => {},
        Err(e) => {
            let err = format!("Failed to insert chunks into vector store: {}", e);
            state.record_file_failed(organization_id, filename, &content_hash, file_size, file_type.clone(), &err, "vector_store", Some(job_id));
            state.job_queue().update_status(job_id, JobStatus::Failed, Some(err.clone()));
            return Err(Error::Internal(err));
        }
    }
    tracing::info!("Inserted {} chunks into vector store for {}", embedded_chunks.len(), filename);

    // Update document with chunk count and save
    let mut doc = document;
    doc.total_chunks = embedded_chunks.len() as u32;
    let final_doc_id = doc.id;
    let final_file_size = doc.file_size;
    let final_file_type = doc.file_type.clone();
    let final_chunks_count = doc.total_chunks;
    state.add_document(doc);

    // Record file success in file registry (this updates both in-memory cache and database)
    state.record_file_success(
        organization_id,
        filename,
        &content_hash,
        final_file_size,
        final_file_type,
        final_doc_id,
        final_chunks_count,
        Some(job_id),
    );

    // Update job status to complete
    state.job_queue().update_status(job_id, JobStatus::Complete, None);
    state.job_queue().increment_files_processed(job_id);

    tracing::info!("Successfully processed {} (job {})", filename, job_id);
    Ok(())
}
