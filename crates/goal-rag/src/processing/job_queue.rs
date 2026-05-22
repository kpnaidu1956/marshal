//! Job queue for background document processing with persistence
//!
//! Jobs are persisted to SQLite for resumability after restart.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{FileCharacteristics, FileTier};
#[cfg(not(feature = "postgres"))]
use crate::storage::FileRegistryDb;
use crate::storage::{
    JobFileRecord, JobFileStatus, JobOptions, JobRecord,
    PersistedJobStage, PersistedJobStatus,
};
#[cfg(feature = "postgres")]
use crate::storage::PgFileRegistry;

/// Processing stage
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStage {
    Queued,
    Uploading,
    Parsing,
    Chunking,
    Embedding,
    Storing,
    Complete,
    Failed,
}

/// Job status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Processing,
    Complete,
    Failed,
}

/// Error details for a file that failed to process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileError {
    pub filename: String,
    pub error: String,
    pub stage: ProcessingStage,
}

/// Parser attempt record for tracking escalation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserAttemptRecord {
    pub parser_name: String,
    pub success: bool,
    pub error: Option<String>,
    pub chars_extracted: Option<usize>,
    pub duration_ms: u64,
}

/// Per-file progress tracking with tier and parser information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProgressRecord {
    /// File name
    pub filename: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// Assigned processing tier
    pub tier: FileTier,
    /// Processing status
    pub status: FileProcessingStatus,
    /// Current/final parser method
    pub parser_method: Option<String>,
    /// All parser attempts made
    pub parser_attempts: Vec<ParserAttemptRecord>,
    /// Processing start time
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Processing end time (if completed)
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Error message if failed
    pub error: Option<String>,
}

/// File processing status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileProcessingStatus {
    Queued,
    Parsing,
    Chunking,
    Embedding,
    Storing,
    Complete,
    Skipped,
    Failed,
}

/// Progress information for a job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub job_id: Uuid,
    /// Organization ID for multi-tenancy
    pub organization_id: Option<String>,
    pub status: JobStatus,
    pub stage: ProcessingStage,
    pub total_files: usize,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub current_file: Option<String>,
    pub total_chunks: usize,
    pub chunks_embedded: usize,
    pub error: Option<String>,
    pub file_errors: Vec<FileError>,
    pub skipped_files: Vec<String>,
    /// Per-file progress with tier and parser details
    pub file_progress: Vec<FileProgressRecord>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl JobProgress {
    pub fn new(job_id: Uuid, total_files: usize) -> Self {
        Self::with_org(job_id, total_files, None)
    }

    pub fn with_org(job_id: Uuid, total_files: usize, organization_id: Option<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            job_id,
            organization_id,
            status: JobStatus::Pending,
            stage: ProcessingStage::Queued,
            total_files,
            files_processed: 0,
            files_skipped: 0,
            files_failed: 0,
            current_file: None,
            total_chunks: 0,
            chunks_embedded: 0,
            error: None,
            file_errors: Vec::new(),
            skipped_files: Vec::new(),
            file_progress: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn percent_complete(&self) -> f32 {
        if self.total_files == 0 {
            return 0.0;
        }

        let file_progress = self.files_processed as f32 / self.total_files as f32;

        // If we're embedding, factor in chunk progress
        if self.stage == ProcessingStage::Embedding && self.total_chunks > 0 {
            let chunk_progress = self.chunks_embedded as f32 / self.total_chunks as f32;
            let current_file_weight = 1.0 / self.total_files as f32;
            return (file_progress + chunk_progress * current_file_weight) * 100.0;
        }

        file_progress * 100.0
    }
}

/// A processing job
#[derive(Debug, Clone)]
pub struct Job {
    pub id: Uuid,
    pub files: Vec<FileData>,
    pub options: ProcessingOptions,
}

/// File data for processing
#[derive(Debug, Clone)]
pub struct FileData {
    pub filename: String,
    pub data: Vec<u8>,
    /// Organization ID for multi-tenancy
    pub organization_id: String,
}

/// Processing options
#[derive(Debug, Clone, Default)]
pub struct ProcessingOptions {
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
    pub parallel_embeddings: usize,
}

impl Default for Job {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            files: Vec::new(),
            options: ProcessingOptions::default(),
        }
    }
}

// ============================================================================
// GCS-Based Job Types (New Architecture - files already in GCS)
// ============================================================================

/// File reference for GCS-based processing (file already uploaded to GCS)
#[derive(Debug, Clone)]
pub struct GcsFileRef {
    /// Original filename
    pub filename: String,
    /// Organization ID
    pub organization_id: String,
    /// GCS path to the file
    pub gcs_path: String,
    /// Content hash (SHA-256)
    pub content_hash: String,
    /// File size in bytes
    pub file_size: u64,
}

/// GCS-based processing options
#[derive(Debug, Clone, Default)]
pub struct GcsProcessingOptions {
    /// Custom chunk size (overrides config)
    pub chunk_size: Option<usize>,
    /// Custom chunk overlap (overrides config)
    pub chunk_overlap: Option<usize>,
    /// Number of parallel embedding workers
    pub parallel_embeddings: usize,
}

/// A GCS-based processing job (file already in GCS, no raw bytes needed)
#[derive(Debug, Clone)]
pub struct GcsJob {
    pub id: Uuid,
    pub file: GcsFileRef,
    pub options: GcsProcessingOptions,
}

impl GcsJob {
    /// Create a new GCS job
    pub fn new(file: GcsFileRef, options: GcsProcessingOptions) -> Self {
        Self {
            id: Uuid::new_v4(),
            file,
            options,
        }
    }
}

/// Enum to represent either a regular job or a GCS job
#[derive(Debug, Clone)]
pub enum ProcessingJob {
    /// Traditional job with raw bytes
    Regular(Job),
    /// GCS-based job (file already in GCS)
    Gcs(GcsJob),
}

impl ProcessingJob {
    /// Get the job ID
    pub fn id(&self) -> Uuid {
        match self {
            ProcessingJob::Regular(job) => job.id,
            ProcessingJob::Gcs(job) => job.id,
        }
    }

    /// Get total file count
    pub fn file_count(&self) -> usize {
        match self {
            ProcessingJob::Regular(job) => job.files.len(),
            ProcessingJob::Gcs(_) => 1, // GCS jobs are single-file
        }
    }
}

/// Job queue for managing background processing with persistence
pub struct JobQueue {
    /// Active jobs with progress
    jobs: Arc<DashMap<Uuid, JobProgress>>,
    /// Channel for sending jobs to workers
    sender: mpsc::Sender<Job>,
    /// Number of workers
    worker_count: usize,
    /// Jobs in queue
    queue_size: Arc<AtomicUsize>,
    /// Database for persistence
    #[cfg(feature = "postgres")]
    database: Arc<PgFileRegistry>,
    #[cfg(not(feature = "postgres"))]
    database: Arc<FileRegistryDb>,
}

impl JobQueue {
    /// Create a new job queue with database persistence
    #[cfg(feature = "postgres")]
    pub fn new(worker_count: usize, database: Arc<PgFileRegistry>) -> (Self, mpsc::Receiver<Job>) {
        let (sender, receiver) = mpsc::channel(1000);

        let queue = Self {
            jobs: Arc::new(DashMap::new()),
            sender,
            worker_count,
            queue_size: Arc::new(AtomicUsize::new(0)),
            database,
        };

        (queue, receiver)
    }

    /// Create a new job queue with database persistence
    #[cfg(not(feature = "postgres"))]
    pub fn new(worker_count: usize, database: Arc<FileRegistryDb>) -> (Self, mpsc::Receiver<Job>) {
        let (sender, receiver) = mpsc::channel(1000);

        let queue = Self {
            jobs: Arc::new(DashMap::new()),
            sender,
            worker_count,
            queue_size: Arc::new(AtomicUsize::new(0)),
            database,
        };

        (queue, receiver)
    }

    /// Submit a job for processing (with persistence)
    pub async fn submit(&self, job: Job) -> Uuid {
        let job_id = job.id;
        let total_files = job.files.len();

        // Create progress entry
        let progress = JobProgress::new(job_id, total_files);
        self.jobs.insert(job_id, progress.clone());
        self.queue_size.fetch_add(1, Ordering::SeqCst);

        // Persist job to database
        let job_record = JobRecord::new(
            job_id,
            total_files,
            Some(JobOptions {
                chunk_size: job.options.chunk_size,
                chunk_overlap: job.options.chunk_overlap,
                parallel_embeddings: job.options.parallel_embeddings,
            }),
        );
        #[cfg(feature = "postgres")]
        if let Err(e) = self.database.create_job(&job_record).await {
            tracing::error!("Failed to persist job to database: {}", e);
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.create_job(&job_record) {
            tracing::error!("Failed to persist job to database: {}", e);
        }

        // Persist each file with its data for resumability
        for file in &job.files {
            let file_record = JobFileRecord::new(
                file.filename.clone(),
                file.data.len() as u64,
                Some(file.data.clone()),
            );
            #[cfg(feature = "postgres")]
            if let Err(e) = self.database.add_job_file(job_id, &file_record).await {
                tracing::error!("Failed to persist job file {}: {}", file.filename, e);
            }
            #[cfg(not(feature = "postgres"))]
            if let Err(e) = self.database.add_job_file(job_id, &file_record) {
                tracing::error!("Failed to persist job file {}: {}", file.filename, e);
            }
        }

        // Send to workers
        if let Err(e) = self.sender.send(job).await {
            tracing::error!("Failed to submit job: {}", e);
            self.update_status(job_id, JobStatus::Failed, Some(e.to_string()));
        }

        job_id
    }

    /// Submit a GCS-based job for processing
    ///
    /// This creates a job entry for tracking but doesn't persist raw bytes.
    /// The worker will download the file from GCS for processing.
    ///
    /// Returns the job ID for status polling.
    pub async fn submit_gcs_job(&self, gcs_job: GcsJob) -> Uuid {
        let job_id = gcs_job.id;

        // Create progress entry with organization_id from the file
        let mut progress = JobProgress::with_org(job_id, 1, Some(gcs_job.file.organization_id.clone())); // Single file per GCS job
        progress.file_progress.push(FileProgressRecord {
            filename: gcs_job.file.filename.clone(),
            size_bytes: gcs_job.file.file_size,
            tier: FileTier::Medium, // Will be re-evaluated by worker
            status: FileProcessingStatus::Queued,
            parser_method: None,
            parser_attempts: Vec::new(),
            started_at: chrono::Utc::now(),
            completed_at: None,
            duration_ms: None,
            error: None,
        });
        self.jobs.insert(job_id, progress);
        self.queue_size.fetch_add(1, Ordering::SeqCst);

        // Persist job to database (without file data - it's in GCS)
        let job_record = JobRecord::new(
            job_id,
            1,
            Some(JobOptions {
                chunk_size: gcs_job.options.chunk_size,
                chunk_overlap: gcs_job.options.chunk_overlap,
                parallel_embeddings: gcs_job.options.parallel_embeddings,
            }),
        );
        #[cfg(feature = "postgres")]
        if let Err(e) = self.database.create_job(&job_record).await {
            tracing::error!("Failed to persist GCS job to database: {}", e);
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.create_job(&job_record) {
            tracing::error!("Failed to persist GCS job to database: {}", e);
        }

        // Create a file record (without data blob - it's in GCS)
        let file_record = JobFileRecord {
            filename: gcs_job.file.filename.clone(),
            file_size: gcs_job.file.file_size,
            content_hash: Some(gcs_job.file.content_hash.clone()),
            status: JobFileStatus::Pending,
            tier: None,
            parser_method: None,
            error: None,
            started_at: None,
            completed_at: None,
            duration_ms: None,
            file_data: None, // No raw bytes - file is in GCS
        };
        #[cfg(feature = "postgres")]
        if let Err(e) = self.database.add_job_file(job_id, &file_record).await {
            tracing::error!("Failed to persist GCS job file record: {}", e);
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.add_job_file(job_id, &file_record) {
            tracing::error!("Failed to persist GCS job file record: {}", e);
        }

        // Note: For GCS jobs, the endpoint spawns its own processing task
        // since we need access to the GCS client to download the file.
        // The job queue only provides progress tracking.

        tracing::info!(
            "Created GCS job {} for file {} (org: {})",
            job_id,
            gcs_job.file.filename,
            gcs_job.file.organization_id
        );

        job_id
    }

    /// Get incomplete jobs from database (for resuming on startup)
    #[cfg(feature = "postgres")]
    pub async fn get_incomplete_jobs(&self) -> Vec<JobRecord> {
        match self.database.get_incomplete_jobs().await {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!("Failed to get incomplete jobs: {}", e);
                Vec::new()
            }
        }
    }

    /// Get incomplete jobs from database (for resuming on startup)
    #[cfg(not(feature = "postgres"))]
    pub fn get_incomplete_jobs(&self) -> Vec<JobRecord> {
        match self.database.get_incomplete_jobs() {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!("Failed to get incomplete jobs: {}", e);
                Vec::new()
            }
        }
    }

    /// Get pending files for a job (for resuming)
    #[cfg(feature = "postgres")]
    pub async fn get_pending_files(&self, job_id: Uuid) -> Vec<JobFileRecord> {
        match self.database.get_pending_job_files(job_id).await {
            Ok(files) => files,
            Err(e) => {
                tracing::error!("Failed to get pending files for job {}: {}", job_id, e);
                Vec::new()
            }
        }
    }

    /// Get pending files for a job (for resuming)
    #[cfg(not(feature = "postgres"))]
    pub fn get_pending_files(&self, job_id: Uuid) -> Vec<JobFileRecord> {
        match self.database.get_pending_job_files(job_id) {
            Ok(files) => files,
            Err(e) => {
                tracing::error!("Failed to get pending files for job {}: {}", job_id, e);
                Vec::new()
            }
        }
    }

    /// Resume an incomplete job
    pub async fn resume_job(&self, job_record: JobRecord) -> Option<Uuid> {
        let job_id = job_record.id;

        // Get pending files with their data
        #[cfg(feature = "postgres")]
        let pending_files = self.get_pending_files(job_id).await;
        #[cfg(not(feature = "postgres"))]
        let pending_files = self.get_pending_files(job_id);
        if pending_files.is_empty() {
            tracing::info!("Job {} has no pending files, marking complete", job_id);
            self.update_status(job_id, JobStatus::Complete, None);
            return None;
        }

        // Restore progress entry
        let progress = JobProgress {
            job_id,
            organization_id: None, // Will be set from file context during processing
            status: JobStatus::Processing,
            stage: ProcessingStage::Parsing,
            total_files: job_record.total_files,
            files_processed: job_record.files_processed,
            files_skipped: job_record.files_skipped,
            files_failed: job_record.files_failed,
            current_file: None,
            total_chunks: job_record.total_chunks,
            chunks_embedded: job_record.chunks_embedded,
            error: None,
            file_errors: Vec::new(),
            skipped_files: Vec::new(),
            file_progress: Vec::new(),
            created_at: job_record.created_at,
            updated_at: chrono::Utc::now(),
        };
        self.jobs.insert(job_id, progress);
        self.queue_size.fetch_add(1, Ordering::SeqCst);

        // Convert to Job with only pending files
        // Note: organization_id defaults to "unknown" for legacy resumed jobs
        let files: Vec<FileData> = pending_files
            .into_iter()
            .filter_map(|f| {
                f.file_data.map(|data| FileData {
                    filename: f.filename,
                    data,
                    organization_id: "unknown".to_string(), // Legacy job resume - org context not stored
                })
            })
            .collect();

        if files.is_empty() {
            tracing::warn!("Job {} has pending files but no file data stored", job_id);
            return None;
        }

        let job = Job {
            id: job_id,
            files,
            options: job_record.options.map(|o| ProcessingOptions {
                chunk_size: o.chunk_size,
                chunk_overlap: o.chunk_overlap,
                parallel_embeddings: o.parallel_embeddings,
            }).unwrap_or_default(),
        };

        tracing::info!(
            "Resuming job {} with {} pending files (previously processed: {})",
            job_id, job.files.len(), job_record.files_processed
        );

        // Send to workers
        if let Err(e) = self.sender.send(job).await {
            tracing::error!("Failed to resume job {}: {}", job_id, e);
            self.update_status(job_id, JobStatus::Failed, Some(e.to_string()));
            return None;
        }

        Some(job_id)
    }

    /// Persist current job state to database
    fn persist_job_state(&self, job_id: Uuid) {
        if let Some(progress) = self.jobs.get(&job_id) {
            let job_record = JobRecord {
                id: job_id,
                status: match progress.status {
                    JobStatus::Pending => PersistedJobStatus::Pending,
                    JobStatus::Processing => PersistedJobStatus::Processing,
                    JobStatus::Complete => PersistedJobStatus::Complete,
                    JobStatus::Failed => PersistedJobStatus::Failed,
                },
                stage: match progress.stage {
                    ProcessingStage::Queued => PersistedJobStage::Queued,
                    ProcessingStage::Uploading => PersistedJobStage::Uploading,
                    ProcessingStage::Parsing => PersistedJobStage::Parsing,
                    ProcessingStage::Chunking => PersistedJobStage::Chunking,
                    ProcessingStage::Embedding => PersistedJobStage::Embedding,
                    ProcessingStage::Storing => PersistedJobStage::Storing,
                    ProcessingStage::Complete => PersistedJobStage::Complete,
                    ProcessingStage::Failed => PersistedJobStage::Failed,
                },
                total_files: progress.total_files,
                files_processed: progress.files_processed,
                files_skipped: progress.files_skipped,
                files_failed: progress.files_failed,
                total_chunks: progress.total_chunks,
                chunks_embedded: progress.chunks_embedded,
                current_file: progress.current_file.clone(),
                error: progress.error.clone(),
                created_at: progress.created_at,
                updated_at: progress.updated_at,
                completed_at: if progress.status == JobStatus::Complete || progress.status == JobStatus::Failed {
                    Some(chrono::Utc::now())
                } else {
                    None
                },
                options: None,
            };

            #[cfg(feature = "postgres")]
            {
                let db = self.database.clone();
                let record = job_record;
                tokio::spawn(async move {
                    if let Err(e) = db.update_job(&record).await {
                        tracing::error!("Failed to persist job state to PG: {}", e);
                    }
                });
            }
            #[cfg(not(feature = "postgres"))]
            if let Err(e) = self.database.update_job(&job_record) {
                tracing::error!("Failed to persist job {} state: {}", job_id, e);
            }
        }
    }

    /// Mark a file as processing in the database
    pub fn mark_file_processing(&self, job_id: Uuid, filename: &str) {
        #[cfg(feature = "postgres")]
        {
            let db = self.database.clone();
            let fname = filename.to_string();
            tokio::spawn(async move {
                if let Err(e) = db.update_job_file_status(job_id, &fname, JobFileStatus::Processing, None, None, None).await {
                    tracing::error!("Failed to mark file {} as processing: {}", fname, e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.update_job_file_status(
            job_id, filename, JobFileStatus::Processing, None, None, None,
        ) {
            tracing::error!("Failed to mark file {} as processing: {}", filename, e);
        }
    }

    /// Mark a file as complete in the database
    pub fn mark_file_complete(&self, job_id: Uuid, filename: &str, parser_method: Option<&str>, duration_ms: u64) {
        #[cfg(feature = "postgres")]
        {
            let db = self.database.clone();
            let fname = filename.to_string();
            let method = parser_method.map(|s| s.to_string());
            tokio::spawn(async move {
                if let Err(e) = db.update_job_file_status(job_id, &fname, JobFileStatus::Complete, None, method.as_deref(), Some(duration_ms)).await {
                    tracing::error!("Failed to mark file {} as complete: {}", fname, e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.update_job_file_status(
            job_id, filename, JobFileStatus::Complete, None, parser_method, Some(duration_ms),
        ) {
            tracing::error!("Failed to mark file {} as complete: {}", filename, e);
        }
    }

    /// Mark a file as failed in the database
    pub fn mark_file_failed(&self, job_id: Uuid, filename: &str, error: &str, duration_ms: u64) {
        #[cfg(feature = "postgres")]
        {
            let db = self.database.clone();
            let fname = filename.to_string();
            let err = error.to_string();
            tokio::spawn(async move {
                if let Err(e) = db.update_job_file_status(job_id, &fname, JobFileStatus::Failed, Some(&err), None, Some(duration_ms)).await {
                    tracing::error!("Failed to mark file {} as failed: {}", fname, e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.update_job_file_status(
            job_id, filename, JobFileStatus::Failed, Some(error), None, Some(duration_ms),
        ) {
            tracing::error!("Failed to mark file {} as failed: {}", filename, e);
        }
    }

    /// Mark a file as skipped in the database
    pub fn mark_file_skipped(&self, job_id: Uuid, filename: &str, reason: &str) {
        #[cfg(feature = "postgres")]
        {
            let db = self.database.clone();
            let fname = filename.to_string();
            let rsn = reason.to_string();
            tokio::spawn(async move {
                if let Err(e) = db.update_job_file_status(job_id, &fname, JobFileStatus::Skipped, Some(&rsn), None, None).await {
                    tracing::error!("Failed to mark file {} as skipped: {}", fname, e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.update_job_file_status(
            job_id, filename, JobFileStatus::Skipped, Some(reason), None, None,
        ) {
            tracing::error!("Failed to mark file {} as skipped: {}", filename, e);
        }
    }

    /// Clear file data after job completion (to save space)
    pub fn clear_job_file_data(&self, job_id: Uuid) {
        #[cfg(feature = "postgres")]
        {
            let db = self.database.clone();
            tokio::spawn(async move {
                if let Err(e) = db.clear_job_file_data(job_id).await {
                    tracing::error!("Failed to clear file data for job {}: {}", job_id, e);
                }
            });
        }
        #[cfg(not(feature = "postgres"))]
        if let Err(e) = self.database.clear_job_file_data(job_id) {
            tracing::error!("Failed to clear file data for job {}: {}", job_id, e);
        }
    }

    /// Get job progress
    pub fn get_progress(&self, job_id: Uuid) -> Option<JobProgress> {
        self.jobs.get(&job_id).map(|p| p.clone())
    }

    /// Get all jobs
    pub fn list_jobs(&self) -> Vec<JobProgress> {
        self.jobs.iter().map(|e| e.value().clone()).collect()
    }

    /// Update job stage
    pub fn update_stage(&self, job_id: Uuid, stage: ProcessingStage) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.stage = stage;
            progress.updated_at = chrono::Utc::now();
            if stage == ProcessingStage::Complete {
                progress.status = JobStatus::Complete;
                self.queue_size.fetch_sub(1, Ordering::SeqCst);
                // Clear file data to save space on completion
                self.clear_job_file_data(job_id);
            }
            drop(progress); // Release lock before persisting
            self.persist_job_state(job_id);
        }
    }

    /// Update job status
    pub fn update_status(&self, job_id: Uuid, status: JobStatus, error: Option<String>) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.status = status;
            progress.error = error;
            progress.updated_at = chrono::Utc::now();
            if status == JobStatus::Failed || status == JobStatus::Complete {
                self.queue_size.fetch_sub(1, Ordering::SeqCst);
                // Clear file data to save space on completion/failure
                self.clear_job_file_data(job_id);
            }
            drop(progress); // Release lock before persisting
            self.persist_job_state(job_id);
        }
    }

    /// Update current file being processed
    pub fn update_current_file(&self, job_id: Uuid, filename: &str) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.current_file = Some(filename.to_string());
            progress.updated_at = chrono::Utc::now();
        }
        // Note: Don't persist for current_file updates - too frequent
    }

    /// Increment files processed
    pub fn increment_files_processed(&self, job_id: Uuid) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.files_processed += 1;
            progress.updated_at = chrono::Utc::now();
            drop(progress); // Release lock before persisting
            self.persist_job_state(job_id);
        }
    }

    /// Set total chunks for current file
    pub fn set_total_chunks(&self, job_id: Uuid, total: usize) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.total_chunks = total;
            progress.chunks_embedded = 0;
            progress.updated_at = chrono::Utc::now();
        }
    }

    /// Increment chunks embedded
    pub fn increment_chunks_embedded(&self, job_id: Uuid, count: usize) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.chunks_embedded += count;
            progress.updated_at = chrono::Utc::now();
        }
    }

    /// Add a file error
    pub fn add_file_error(&self, job_id: Uuid, filename: &str, error: &str, stage: ProcessingStage) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.files_failed += 1;
            progress.file_errors.push(FileError {
                filename: filename.to_string(),
                error: error.to_string(),
                stage,
            });
            progress.updated_at = chrono::Utc::now();
            drop(progress); // Release lock before persisting
            self.persist_job_state(job_id);
        }
    }

    /// Add a skipped file
    pub fn add_skipped_file(&self, job_id: Uuid, filename: &str, reason: &str) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            progress.files_skipped += 1;
            progress.skipped_files.push(format!("{}: {}", filename, reason));
            progress.updated_at = chrono::Utc::now();
            drop(progress); // Release lock before persisting
            self.persist_job_state(job_id);
        }
    }

    /// Start tracking a file with tier information
    pub fn start_file_progress(
        &self,
        job_id: Uuid,
        filename: &str,
        size_bytes: u64,
        characteristics: &FileCharacteristics,
    ) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            let file_record = FileProgressRecord {
                filename: filename.to_string(),
                size_bytes,
                tier: characteristics.tier,
                status: FileProcessingStatus::Parsing,
                parser_method: None,
                parser_attempts: Vec::new(),
                started_at: chrono::Utc::now(),
                completed_at: None,
                duration_ms: None,
                error: None,
            };
            progress.file_progress.push(file_record);
            progress.updated_at = chrono::Utc::now();
        }
    }

    /// Update file progress with parser attempt
    #[allow(clippy::too_many_arguments)]
    pub fn add_parser_attempt(
        &self,
        job_id: Uuid,
        filename: &str,
        parser_name: &str,
        success: bool,
        error: Option<&str>,
        chars_extracted: Option<usize>,
        duration_ms: u64,
    ) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            if let Some(file_record) = progress.file_progress.iter_mut()
                .find(|f| f.filename == filename)
            {
                file_record.parser_attempts.push(ParserAttemptRecord {
                    parser_name: parser_name.to_string(),
                    success,
                    error: error.map(String::from),
                    chars_extracted,
                    duration_ms,
                });
                if success {
                    file_record.parser_method = Some(parser_name.to_string());
                }
            }
            progress.updated_at = chrono::Utc::now();
        }
    }

    /// Complete file progress tracking
    pub fn complete_file_progress(
        &self,
        job_id: Uuid,
        filename: &str,
        status: FileProcessingStatus,
        error: Option<&str>,
    ) {
        if let Some(mut progress) = self.jobs.get_mut(&job_id) {
            if let Some(file_record) = progress.file_progress.iter_mut()
                .find(|f| f.filename == filename)
            {
                let now = chrono::Utc::now();
                file_record.status = status;
                file_record.completed_at = Some(now);
                file_record.duration_ms = Some(
                    (now - file_record.started_at).num_milliseconds() as u64
                );
                file_record.error = error.map(String::from);
            }
            progress.updated_at = chrono::Utc::now();
        }
    }

    /// Get file progress for a job
    pub fn get_file_progress(&self, job_id: Uuid) -> Option<Vec<FileProgressRecord>> {
        self.jobs.get(&job_id).map(|p| p.file_progress.clone())
    }

    /// Get queue statistics
    pub fn stats(&self) -> QueueStats {
        let total = self.jobs.len();
        let pending = self.jobs.iter().filter(|j| j.status == JobStatus::Pending).count();
        let processing = self.jobs.iter().filter(|j| j.status == JobStatus::Processing).count();
        let complete = self.jobs.iter().filter(|j| j.status == JobStatus::Complete).count();
        let failed = self.jobs.iter().filter(|j| j.status == JobStatus::Failed).count();

        QueueStats {
            total_jobs: total,
            pending,
            processing,
            complete,
            failed,
            worker_count: self.worker_count,
        }
    }

    /// Get jobs reference for workers
    pub fn jobs_ref(&self) -> Arc<DashMap<Uuid, JobProgress>> {
        self.jobs.clone()
    }
}

/// Queue statistics
#[derive(Debug, Clone, Serialize)]
pub struct QueueStats {
    pub total_jobs: usize,
    pub pending: usize,
    pub processing: usize,
    pub complete: usize,
    pub failed: usize,
    pub worker_count: usize,
}
