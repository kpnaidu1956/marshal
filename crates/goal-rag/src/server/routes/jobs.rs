//! Job management and progress endpoints

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::server::state::AppState;

/// Query parameters for job operations requiring org context
#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
}

/// GET /api/jobs/:id - Get job progress
pub async fn get_job_progress(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<JobProgressResponse>> {
    let progress = state
        .job_queue()
        .get_progress(job_id)
        .ok_or_else(|| Error::DocumentNotFound(format!("Job {} not found", job_id)))?;

    // Verify job belongs to the requested organization
    if let Some(ref job_org) = progress.organization_id {
        if job_org != &query.organization_id {
            return Err(Error::DocumentNotFound(format!(
                "Job {} not found in organization {}",
                job_id, query.organization_id
            )));
        }
    }

    let file_errors: Vec<FileErrorResponse> = progress
        .file_errors
        .iter()
        .map(|e| FileErrorResponse {
            filename: e.filename.clone(),
            error: e.error.clone(),
            stage: format!("{:?}", e.stage).to_lowercase(),
        })
        .collect();

    // Parse skipped files into structured format
    let skipped_files: Vec<SkippedFileInfo> = progress
        .skipped_files
        .iter()
        .map(|s| {
            // Format is "filename: reason"
            let parts: Vec<&str> = s.splitn(2, ": ").collect();
            let (filename, reason) = if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                (s.clone(), "unknown".to_string())
            };

            // Detect reason type from the reason text
            let reason_type = if reason.contains("duplicate") {
                "duplicate"
            } else if reason.contains("unchanged") || reason.contains("hash:") {
                "unchanged"
            } else if reason.contains("empty") || reason.contains("no text") {
                "empty"
            } else if reason.contains("unsupported") || reason.contains("format") {
                "unsupported"
            } else {
                "error"
            };

            SkippedFileInfo {
                filename,
                reason,
                reason_type: reason_type.to_string(),
            }
        })
        .collect();

    Ok(Json(JobProgressResponse {
        job_id: progress.job_id,
        organization_id: progress.organization_id.clone(),
        status: format!("{:?}", progress.status).to_lowercase(),
        stage: format!("{:?}", progress.stage).to_lowercase(),
        percent_complete: progress.percent_complete(),
        total_files: progress.total_files,
        files_processed: progress.files_processed,
        files_skipped: progress.files_skipped,
        files_failed: progress.files_failed,
        current_file: progress.current_file,
        total_chunks: progress.total_chunks,
        chunks_embedded: progress.chunks_embedded,
        error: progress.error,
        file_errors,
        skipped_files,
        created_at: progress.created_at.to_rfc3339(),
        updated_at: progress.updated_at.to_rfc3339(),
    }))
}

/// GET /api/jobs - List all jobs for an organization
pub async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> Json<JobListResponse> {
    // Filter jobs by organization
    let jobs_list: Vec<_> = state.job_queue().list_jobs()
        .into_iter()
        .filter(|job| {
            // Include jobs that match the org or have no org set (legacy jobs)
            job.organization_id.as_ref() == Some(&query.organization_id)
                || job.organization_id.is_none()
        })
        .collect();
    let stats = state.job_queue().stats();

    // Calculate aggregate stats
    let total_files_processed: usize = jobs_list.iter().map(|j| j.files_processed).sum();
    let total_files_skipped: usize = jobs_list.iter().map(|j| j.files_skipped).sum();
    let total_files_failed: usize = jobs_list.iter().map(|j| j.files_failed).sum();

    // Collect all file errors across all jobs
    let all_file_errors: Vec<FileErrorWithJob> = jobs_list
        .iter()
        .flat_map(|job| {
            job.file_errors.iter().map(move |e| FileErrorWithJob {
                job_id: job.job_id,
                filename: e.filename.clone(),
                error: e.error.clone(),
                stage: format!("{:?}", e.stage).to_lowercase(),
            })
        })
        .collect();

    let jobs: Vec<JobSummary> = jobs_list
        .into_iter()
        .map(|p| {
            let file_errors: Vec<FileErrorResponse> = p
                .file_errors
                .iter()
                .map(|e| FileErrorResponse {
                    filename: e.filename.clone(),
                    error: e.error.clone(),
                    stage: format!("{:?}", e.stage).to_lowercase(),
                })
                .collect();

            JobSummary {
                job_id: p.job_id,
                status: format!("{:?}", p.status).to_lowercase(),
                stage: format!("{:?}", p.stage).to_lowercase(),
                percent_complete: p.percent_complete(),
                total_files: p.total_files,
                files_processed: p.files_processed,
                files_skipped: p.files_skipped,
                files_failed: p.files_failed,
                error: p.error,
                file_errors,
            }
        })
        .collect();

    Json(JobListResponse {
        jobs,
        total_jobs: stats.total_jobs,
        pending: stats.pending,
        processing: stats.processing,
        complete: stats.complete,
        failed: stats.failed,
        worker_count: stats.worker_count,
        total_files_processed,
        total_files_skipped,
        total_files_failed,
        all_file_errors,
    })
}

#[derive(Debug, Serialize)]
pub struct JobProgressResponse {
    pub job_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    pub status: String,
    pub stage: String,
    pub percent_complete: f32,
    pub total_files: usize,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub current_file: Option<String>,
    pub total_chunks: usize,
    pub chunks_embedded: usize,
    pub error: Option<String>,
    pub file_errors: Vec<FileErrorResponse>,
    pub skipped_files: Vec<SkippedFileInfo>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct FileErrorResponse {
    pub filename: String,
    pub error: String,
    pub stage: String,
}

#[derive(Debug, Serialize)]
pub struct SkippedFileInfo {
    pub filename: String,
    pub reason: String,
    pub reason_type: String,  // "duplicate", "unchanged", "empty", "unsupported", "error"
}

#[derive(Debug, Serialize)]
pub struct JobSummary {
    pub job_id: Uuid,
    pub status: String,
    pub stage: String,
    pub percent_complete: f32,
    pub total_files: usize,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub error: Option<String>,
    /// File-level errors (only included if there are failures)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_errors: Vec<FileErrorResponse>,
}

#[derive(Debug, Serialize)]
pub struct JobListResponse {
    pub jobs: Vec<JobSummary>,
    pub total_jobs: usize,
    pub pending: usize,
    pub processing: usize,
    pub complete: usize,
    pub failed: usize,
    pub worker_count: usize,
    /// Aggregate stats across all jobs
    pub total_files_processed: usize,
    pub total_files_skipped: usize,
    pub total_files_failed: usize,
    /// All file errors across all jobs (for quick overview)
    pub all_file_errors: Vec<FileErrorWithJob>,
}

#[derive(Debug, Serialize)]
pub struct FileErrorWithJob {
    pub job_id: Uuid,
    pub filename: String,
    pub error: String,
    pub stage: String,
}

/// GET /api/jobs/:id/files - Get per-file progress with tier and parser details
pub async fn get_job_files_progress(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<JobFilesProgressResponse>> {
    let progress = state
        .job_queue()
        .get_progress(job_id)
        .ok_or_else(|| Error::DocumentNotFound(format!("Job {} not found", job_id)))?;

    // Verify job belongs to the requested organization
    if let Some(ref job_org) = progress.organization_id {
        if job_org != &query.organization_id {
            return Err(Error::DocumentNotFound(format!(
                "Job {} not found in organization {}",
                job_id, query.organization_id
            )));
        }
    }

    let files: Vec<FileProgressResponse> = progress
        .file_progress
        .iter()
        .map(|f| {
            let parser_attempts: Vec<ParserAttemptResponse> = f
                .parser_attempts
                .iter()
                .map(|a| ParserAttemptResponse {
                    parser_name: a.parser_name.clone(),
                    success: a.success,
                    error: a.error.clone(),
                    chars_extracted: a.chars_extracted,
                    duration_ms: a.duration_ms,
                })
                .collect();

            FileProgressResponse {
                filename: f.filename.clone(),
                size_bytes: f.size_bytes,
                tier: format!("{:?}", f.tier).to_lowercase(),
                status: format!("{:?}", f.status).to_lowercase(),
                parser_method: f.parser_method.clone(),
                parser_attempts,
                started_at: f.started_at.to_rfc3339(),
                completed_at: f.completed_at.map(|t| t.to_rfc3339()),
                duration_ms: f.duration_ms,
                error: f.error.clone(),
            }
        })
        .collect();

    // Calculate tier summary
    let tier_summary = TierSummary {
        fast: files.iter().filter(|f| f.tier == "fast").count(),
        medium: files.iter().filter(|f| f.tier == "medium").count(),
        heavy: files.iter().filter(|f| f.tier == "heavy").count(),
        complex: files.iter().filter(|f| f.tier == "complex").count(),
    };

    // Calculate status summary
    let status_summary = StatusSummary {
        queued: files.iter().filter(|f| f.status == "queued").count(),
        parsing: files.iter().filter(|f| f.status == "parsing").count(),
        chunking: files.iter().filter(|f| f.status == "chunking").count(),
        embedding: files.iter().filter(|f| f.status == "embedding").count(),
        storing: files.iter().filter(|f| f.status == "storing").count(),
        complete: files.iter().filter(|f| f.status == "complete").count(),
        skipped: files.iter().filter(|f| f.status == "skipped").count(),
        failed: files.iter().filter(|f| f.status == "failed").count(),
    };

    Ok(Json(JobFilesProgressResponse {
        job_id,
        total_files: files.len(),
        files,
        tier_summary,
        status_summary,
    }))
}

/// Response for per-file progress
#[derive(Debug, Serialize)]
pub struct JobFilesProgressResponse {
    pub job_id: Uuid,
    pub total_files: usize,
    pub files: Vec<FileProgressResponse>,
    pub tier_summary: TierSummary,
    pub status_summary: StatusSummary,
}

#[derive(Debug, Serialize)]
pub struct FileProgressResponse {
    pub filename: String,
    pub size_bytes: u64,
    pub tier: String,
    pub status: String,
    pub parser_method: Option<String>,
    pub parser_attempts: Vec<ParserAttemptResponse>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParserAttemptResponse {
    pub parser_name: String,
    pub success: bool,
    pub error: Option<String>,
    pub chars_extracted: Option<usize>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct TierSummary {
    pub fast: usize,
    pub medium: usize,
    pub heavy: usize,
    pub complex: usize,
}

#[derive(Debug, Serialize)]
pub struct StatusSummary {
    pub queued: usize,
    pub parsing: usize,
    pub chunking: usize,
    pub embedding: usize,
    pub storing: usize,
    pub complete: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// GET /api/system/parsers?organization_id=xxx - Get available parsers and their status
pub async fn get_parsers_status(
    Query(_query): Query<OrgQuery>,
) -> Json<ParsersStatusResponse> {
    use crate::ingestion::ExternalParser;

    // Run blocking process checks on a blocking thread to avoid stalling the async runtime
    let (has_pdftotext, has_tesseract, has_pdftoppm, has_pandoc, has_libreoffice) =
        tokio::task::spawn_blocking(|| {
            let pdftotext = ExternalParser::has_pdftotext();
            let tesseract = ExternalParser::has_tesseract();
            let pdftoppm = ExternalParser::has_pdftoppm();
            let pandoc = ExternalParser::has_pandoc();
            let libreoffice = std::process::Command::new("libreoffice")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            (pdftotext, tesseract, pdftoppm, pandoc, libreoffice)
        })
        .await
        .unwrap_or((false, false, false, false, false));

    // Check for Unstructured API
    let has_unstructured = std::env::var("UNSTRUCTURED_API_KEY").is_ok();

    // Check for Document AI
    let has_document_ai = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok()
        && std::env::var("DOCUMENT_AI_PROCESSOR_ID").is_ok();

    let parsers = vec![
        ParserInfo {
            name: "native_rust".to_string(),
            level: 1,
            available: true,
            purpose: "Fast in-memory parsing (pdf-extract, docx_rs, calamine)".to_string(),
            install_hint: None,
        },
        ParserInfo {
            name: "pdftotext".to_string(),
            level: 2,
            available: has_pdftotext,
            purpose: "Fast PDF extraction with good font handling".to_string(),
            install_hint: Some("apt install poppler-utils".to_string()),
        },
        ParserInfo {
            name: "tesseract_ocr".to_string(),
            level: 3,
            available: has_tesseract && has_pdftoppm,
            purpose: "OCR for scanned PDFs and images".to_string(),
            install_hint: Some("apt install tesseract-ocr poppler-utils".to_string()),
        },
        ParserInfo {
            name: "unstructured_api".to_string(),
            level: 4,
            available: has_unstructured,
            purpose: "Cloud API for complex document parsing".to_string(),
            install_hint: Some("Set UNSTRUCTURED_API_KEY environment variable".to_string()),
        },
        ParserInfo {
            name: "document_ai".to_string(),
            level: 5,
            available: has_document_ai,
            purpose: "GCP Document AI for best OCR quality".to_string(),
            install_hint: Some("Set GOOGLE_APPLICATION_CREDENTIALS and DOCUMENT_AI_PROCESSOR_ID".to_string()),
        },
        ParserInfo {
            name: "pandoc".to_string(),
            level: 2,
            available: has_pandoc,
            purpose: "Document conversion (DOCX, RTF, EPUB, ODT)".to_string(),
            install_hint: Some("apt install pandoc".to_string()),
        },
        ParserInfo {
            name: "libreoffice".to_string(),
            level: 3,
            available: has_libreoffice,
            purpose: "Legacy format conversion (DOC, PPT, XLS)".to_string(),
            install_hint: Some("apt install libreoffice".to_string()),
        },
    ];

    let available_count = parsers.iter().filter(|p| p.available).count();
    let escalation_depth = parsers.iter()
        .filter(|p| p.available && p.level <= 5)
        .map(|p| p.level)
        .max()
        .unwrap_or(1);

    Json(ParsersStatusResponse {
        parsers,
        available_count,
        total_count: 7,
        escalation_depth,
        recommendations: get_parser_recommendations(
            has_pdftotext,
            has_tesseract,
            has_unstructured,
            has_document_ai,
        ),
    })
}

fn get_parser_recommendations(
    has_pdftotext: bool,
    has_tesseract: bool,
    has_unstructured: bool,
    has_document_ai: bool,
) -> Vec<String> {
    let mut recommendations = Vec::new();

    if !has_pdftotext {
        recommendations.push("Install poppler-utils for faster PDF extraction".to_string());
    }
    if !has_tesseract {
        recommendations.push("Install tesseract-ocr for scanned document support".to_string());
    }
    if !has_unstructured && !has_document_ai {
        recommendations.push("Configure Unstructured API or Document AI for complex document handling".to_string());
    }
    if recommendations.is_empty() {
        recommendations.push("All recommended parsers are available".to_string());
    }

    recommendations
}

#[derive(Debug, Serialize)]
pub struct ParsersStatusResponse {
    pub parsers: Vec<ParserInfo>,
    pub available_count: usize,
    pub total_count: usize,
    pub escalation_depth: u8,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ParserInfo {
    pub name: String,
    pub level: u8,
    pub available: bool,
    pub purpose: String,
    pub install_hint: Option<String>,
}

/// POST /api/jobs/:id/resume - Resume an incomplete/failed job
pub async fn resume_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
) -> Result<Json<ResumeJobResponse>> {
    // Check if job exists in memory first
    if let Some(progress) = state.job_queue().get_progress(job_id) {
        // Verify job belongs to the requested organization
        if let Some(ref job_org) = progress.organization_id {
            if job_org != &query.organization_id {
                return Err(Error::DocumentNotFound(format!(
                    "Job {} not found in organization {}",
                    job_id, query.organization_id
                )));
            }
        }

        // If job is already processing or pending, don't resume
        if progress.status == crate::processing::JobStatus::Processing {
            return Err(Error::Internal("Job is already processing".to_string()));
        }
        if progress.status == crate::processing::JobStatus::Pending {
            return Err(Error::Internal("Job is already pending".to_string()));
        }
    }

    // Try to get the job from the database
    #[cfg(feature = "postgres")]
    let job_record = state.database().get_job(job_id).await
        .map_err(|e| Error::Internal(format!("Failed to get job: {}", e)))?
        .ok_or_else(|| Error::DocumentNotFound(format!("Job {} not found in database", job_id)))?;
    #[cfg(not(feature = "postgres"))]
    let job_record = state.database().get_job(job_id)
        .map_err(|e| Error::Internal(format!("Failed to get job: {}", e)))?
        .ok_or_else(|| Error::DocumentNotFound(format!("Job {} not found in database", job_id)))?;

    // Check if job has pending files
    #[cfg(feature = "postgres")]
    let pending_files = state.job_queue().get_pending_files(job_id).await;
    #[cfg(not(feature = "postgres"))]
    let pending_files = state.job_queue().get_pending_files(job_id);
    if pending_files.is_empty() {
        return Err(Error::Internal("No pending files to resume".to_string()));
    }

    // Resume the job
    match state.job_queue().resume_job(job_record).await {
        Some(resumed_id) => {
            Ok(Json(ResumeJobResponse {
                job_id: resumed_id,
                pending_files: pending_files.len(),
                already_processed: state.job_queue().get_progress(resumed_id)
                    .map(|p| p.files_processed)
                    .unwrap_or(0),
                message: format!(
                    "Job {} resumed with {} pending files",
                    resumed_id,
                    pending_files.len()
                ),
            }))
        }
        None => {
            Err(Error::Internal("Failed to resume job - no file data available".to_string()))
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ResumeJobResponse {
    pub job_id: Uuid,
    pub pending_files: usize,
    pub already_processed: usize,
    pub message: String,
}

/// GET /api/jobs/incomplete - Get all incomplete jobs that can be resumed for an organization
pub async fn list_incomplete_jobs(
    State(state): State<AppState>,
    Query(query): Query<OrgQuery>,
) -> Json<IncompleteJobsResponse> {
    #[cfg(feature = "postgres")]
    let incomplete_jobs = state.job_queue().get_incomplete_jobs().await;
    #[cfg(not(feature = "postgres"))]
    let incomplete_jobs = state.job_queue().get_incomplete_jobs();

    // Filter by organization and build job info list
    let mut jobs: Vec<IncompleteJobInfo> = Vec::new();
    for job in incomplete_jobs {
        // Filter by organization: check in-memory job progress for org_id match
        let matches_org = state.job_queue().get_progress(job.id)
            .and_then(|p| p.organization_id)
            .map(|org| org == query.organization_id)
            .unwrap_or(false);
        if !matches_org {
            continue;
        }

        #[cfg(feature = "postgres")]
        let pending_files = state.job_queue().get_pending_files(job.id).await;
        #[cfg(not(feature = "postgres"))]
        let pending_files = state.job_queue().get_pending_files(job.id);

        jobs.push(IncompleteJobInfo {
            job_id: job.id,
            status: format!("{:?}", job.status).to_lowercase(),
            stage: format!("{:?}", job.stage).to_lowercase(),
            total_files: job.total_files,
            files_processed: job.files_processed,
            files_remaining: job.total_files.saturating_sub(job.files_processed),
            pending_files_with_data: pending_files.iter().filter(|f| f.file_data.is_some()).count(),
            can_resume: !pending_files.is_empty() && pending_files.iter().any(|f| f.file_data.is_some()),
            created_at: job.created_at.to_rfc3339(),
            updated_at: job.updated_at.to_rfc3339(),
            error: job.error,
        });
    }

    let total_incomplete = jobs.len();
    Json(IncompleteJobsResponse {
        jobs,
        total_incomplete,
    })
}

#[derive(Debug, Serialize)]
pub struct IncompleteJobsResponse {
    pub jobs: Vec<IncompleteJobInfo>,
    pub total_incomplete: usize,
}

#[derive(Debug, Serialize)]
pub struct IncompleteJobInfo {
    pub job_id: Uuid,
    pub status: String,
    pub stage: String,
    pub total_files: usize,
    pub files_processed: usize,
    pub files_remaining: usize,
    pub pending_files_with_data: usize,
    pub can_resume: bool,
    pub created_at: String,
    pub updated_at: String,
    pub error: Option<String>,
}
