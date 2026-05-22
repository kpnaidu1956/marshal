//! File record types for tracking file processing status

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::FileType;

/// Common parameters for creating file records
pub struct FileRecordParams {
    pub organization_id: String,
    pub filename: String,
    pub content_hash: String,
    pub file_size: u64,
    pub file_type: FileType,
    pub job_id: Option<Uuid>,
}

/// Status of a file in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileRecordStatus {
    /// File processed successfully
    Success,
    /// File was skipped (duplicate/unchanged)
    Skipped,
    /// File processing failed
    Failed,
    /// File is currently being processed
    Processing,
}

/// Reason why a file was skipped
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Same file with same content already exists
    Unchanged,
    /// Same content exists under different filename
    Duplicate { existing_filename: String },
    /// File type not supported
    UnsupportedFormat,
    /// File is empty or has no extractable content
    EmptyContent,
}

/// Record of a file that has been processed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    /// Unique record ID
    pub id: Uuid,
    /// Organization ID for multi-tenancy
    pub organization_id: String,
    /// Original filename as uploaded
    pub filename: String,
    /// Content hash (SHA-256)
    pub content_hash: String,
    /// File size in bytes
    pub file_size: u64,
    /// File type detected
    pub file_type: FileType,
    /// Processing status
    pub status: FileRecordStatus,
    /// Document ID if successfully processed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<Uuid>,
    /// Number of chunks created (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunks_created: Option<u32>,
    /// Skip reason (if skipped)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<SkipReason>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Processing stage where failure occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_at_stage: Option<String>,
    /// Job ID that processed this file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<Uuid>,
    /// When the file was first seen
    pub first_seen_at: DateTime<Utc>,
    /// When the file was last processed
    pub last_processed_at: DateTime<Utc>,
    /// Number of times this file was uploaded
    pub upload_count: u32,
    /// GCS URL for original file (if stored)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_url: Option<String>,
    /// GCS URL for plain text (if stored)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plaintext_url: Option<String>,
}

impl FileRecord {
    /// Create a new file record for a successfully processed file
    pub fn success(
        params: FileRecordParams,
        document_id: Uuid,
        chunks_created: u32,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id: params.organization_id,
            filename: params.filename,
            content_hash: params.content_hash,
            file_size: params.file_size,
            file_type: params.file_type,
            status: FileRecordStatus::Success,
            document_id: Some(document_id),
            chunks_created: Some(chunks_created),
            skip_reason: None,
            error_message: None,
            failed_at_stage: None,
            job_id: params.job_id,
            first_seen_at: now,
            last_processed_at: now,
            upload_count: 1,
            original_url: None,
            plaintext_url: None,
        }
    }

    /// Create a new file record for a skipped file
    pub fn skipped(params: FileRecordParams, skip_reason: SkipReason) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id: params.organization_id,
            filename: params.filename,
            content_hash: params.content_hash,
            file_size: params.file_size,
            file_type: params.file_type,
            status: FileRecordStatus::Skipped,
            document_id: None,
            chunks_created: None,
            skip_reason: Some(skip_reason),
            error_message: None,
            failed_at_stage: None,
            job_id: params.job_id,
            first_seen_at: now,
            last_processed_at: now,
            upload_count: 1,
            original_url: None,
            plaintext_url: None,
        }
    }

    /// Create a new file record for a failed file
    pub fn failed(
        params: FileRecordParams,
        error_message: String,
        failed_at_stage: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id: params.organization_id,
            filename: params.filename,
            content_hash: params.content_hash,
            file_size: params.file_size,
            file_type: params.file_type,
            status: FileRecordStatus::Failed,
            document_id: None,
            chunks_created: None,
            skip_reason: None,
            error_message: Some(error_message),
            failed_at_stage: Some(failed_at_stage),
            job_id: params.job_id,
            first_seen_at: now,
            last_processed_at: now,
            upload_count: 1,
            original_url: None,
            plaintext_url: None,
        }
    }

    /// Update record for re-upload
    pub fn update_for_reupload(&mut self, job_id: Option<Uuid>) {
        self.upload_count += 1;
        self.last_processed_at = Utc::now();
        self.job_id = job_id;
    }

    /// Mark as success after reprocessing
    pub fn mark_success(&mut self, document_id: Uuid, chunks_created: u32) {
        self.status = FileRecordStatus::Success;
        self.document_id = Some(document_id);
        self.chunks_created = Some(chunks_created);
        self.error_message = None;
        self.failed_at_stage = None;
        self.skip_reason = None;
        self.last_processed_at = Utc::now();
    }

    /// Mark as failed
    pub fn mark_failed(&mut self, error_message: String, stage: String) {
        self.status = FileRecordStatus::Failed;
        self.error_message = Some(error_message);
        self.failed_at_stage = Some(stage);
        self.document_id = None;
        self.chunks_created = None;
        self.skip_reason = None;
        self.last_processed_at = Utc::now();
    }
}

/// Request to check status of files before upload
#[derive(Debug, Clone, Deserialize)]
pub struct FileCheckRequest {
    /// Organization ID for multi-tenancy (REQUIRED)
    pub organization_id: String,
    /// List of files to check
    pub files: Vec<FileCheckItem>,
}

/// Item in file check request
#[derive(Debug, Clone, Deserialize)]
pub struct FileCheckItem {
    /// Filename
    pub filename: String,
    /// Content hash (optional, if client can compute it)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// File size in bytes
    pub file_size: u64,
}

/// Status of a file for upload decision
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileUploadAdvice {
    /// File should be uploaded (new or modified)
    Upload,
    /// File can be skipped (unchanged)
    Skip { reason: String, existing_document_id: Option<Uuid> },
    /// File previously failed - retry recommended
    Retry { previous_error: String },
}

/// Response for file check request
#[derive(Debug, Clone, Serialize)]
pub struct FileCheckResponse {
    /// Status for each file
    pub files: Vec<FileCheckResult>,
    /// Summary statistics
    pub summary: FileCheckSummary,
}

/// Result for individual file check
#[derive(Debug, Clone, Serialize)]
pub struct FileCheckResult {
    /// Filename checked
    pub filename: String,
    /// Advice on what to do
    pub advice: FileUploadAdvice,
    /// Existing record if found
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_record: Option<FileRecordSummary>,
}

/// Summary of file check
#[derive(Debug, Clone, Serialize)]
pub struct FileCheckSummary {
    /// Total files checked
    pub total_checked: usize,
    /// Files that need uploading
    pub needs_upload: usize,
    /// Files that can be skipped
    pub can_skip: usize,
    /// Files that should retry
    pub should_retry: usize,
}

/// Summary of a file record for API responses
#[derive(Debug, Clone, Serialize)]
pub struct FileRecordSummary {
    pub organization_id: String,
    pub filename: String,
    pub status: FileRecordStatus,
    pub file_type: String,
    pub file_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunks_created: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub last_processed_at: DateTime<Utc>,
}

impl From<&FileRecord> for FileRecordSummary {
    fn from(record: &FileRecord) -> Self {
        Self {
            organization_id: record.organization_id.clone(),
            filename: record.filename.clone(),
            status: record.status.clone(),
            file_type: record.file_type.display_name().to_string(),
            file_size: record.file_size,
            document_id: record.document_id,
            chunks_created: record.chunks_created,
            error_message: record.error_message.clone(),
            last_processed_at: record.last_processed_at,
        }
    }
}
