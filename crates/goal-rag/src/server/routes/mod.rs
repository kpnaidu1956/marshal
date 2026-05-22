//! API routes for the RAG server

pub mod acl;
pub mod analytics;
pub mod analytics_aggregations;
pub mod auth;
pub mod documents;
pub mod files;
pub mod jobs;
pub mod query;
pub mod realtime;
pub mod registration;
pub mod storage;
pub mod trial_admin;
pub mod trial_gdpr;
#[cfg(feature = "postgres")]
pub mod tools;

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
#[cfg(feature = "gcp")]
use axum::extract::DefaultBodyLimit;
use crate::ingestion::ExternalParser;
use crate::server::state::AppState;

/// Build all API routes
pub fn api_routes(#[cfg_attr(not(feature = "gcp"), allow(unused_variables))] max_upload_size: usize) -> Router<AppState> {
    // Public routes (no auth required)
    let public = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/info", get(info))
        .route("/capabilities", get(capabilities))
        // Trial registration (public, no auth)
        .route("/trial/register", post(registration::register))
        .route("/trial/verify-email", post(registration::verify_email))
        .route("/trial/check-org", post(registration::check_org))
        .route("/trial/check-domain", post(registration::check_domain))
        .route("/trial/join-request", post(registration::join_request))
        .route("/trial/eula", get(registration::get_eula));

    // Protected routes (require valid JWT)
    let protected = Router::new()
        // Auth management
        .route("/auth/organizations", get(auth::list_organizations))
        .route("/auth/set-password", post(auth::set_password))
        // Document management
        .route("/documents", get(documents::list_documents))
        .route("/documents/:id", get(documents::get_document))
        .route("/documents/:id/download", get(documents::download_document))
        .route("/documents/:id", delete(documents::delete_document))
        .route("/documents/:id/archive", post(documents::archive_document))
        .route("/documents/:id/unarchive", post(documents::unarchive_document))
        // Job management
        .route("/jobs", get(jobs::list_jobs))
        .route("/jobs/incomplete", get(jobs::list_incomplete_jobs))
        .route("/jobs/:id", get(jobs::get_job_progress))
        .route("/jobs/:id/files", get(jobs::get_job_files_progress))
        .route("/jobs/:id/resume", post(jobs::resume_job))
        // System information
        .route("/system/parsers", get(jobs::get_parsers_status))
        // File status and tracking
        .route("/files", get(files::list_files))
        .route("/files/check", post(files::check_files))
        .route("/files/failed", get(files::list_failed_files))
        .route("/files/failed", delete(files::clear_failed_files))
        .route("/files/stats", get(files::file_stats))
        .route("/files/sync/status", get(files::get_sync_status))
        .route("/files/:filename", get(files::get_file_status))
        .route("/files/:filename", delete(files::delete_file_record))
        // Query
        .route("/query", post(query::query_rag))
        // V2 Query (frontend-friendly format)
        .route("/v2/query", post(query::query_rag_v2))
        // V2 Query streaming (SSE)
        .route("/v2/query/stream", post(query::query_rag_v2_stream))
        // String search
        .route("/string-search", post(query::string_search))
        // Analytics endpoints
        .route("/analytics/info", get(analytics::analytics_info))
        .route("/analytics/analysis/task/:task_id", post(analytics::analyze_task))
        .route("/analytics/analysis/goal/:goal_id", post(analytics::analyze_goal))
        .route("/analytics/jobs/:job_id", get(analytics::get_analysis_job))
        .route("/analytics/timeline/task/:task_id", get(analytics::get_task_timeline))
        .route("/analytics/timeline/goal/:goal_id", get(analytics::get_goal_timeline))
        .route("/analytics/interactions/task/:task_id", get(analytics::get_task_interactions))
        .route("/analytics/interactions/search", post(analytics::search_interactions))
        .route("/analytics/patterns", get(analytics::list_patterns))
        .route("/analytics/patterns/learn", post(analytics::trigger_pattern_learning))
        .route("/analytics/recommendations/task/:task_id", get(analytics::get_task_recommendations))
        .route("/analytics/recommendations/organization", get(analytics::get_org_recommendations))
        .route("/analytics/recommendations/:id/feedback", post(analytics::submit_recommendation_feedback))
        // User Analytics (for frontend dashboards)
        .route("/analytics/user/:user_id/performance", get(analytics::get_user_performance))
        .route("/analytics/user/:user_id/interactions", get(analytics::get_user_interactions))
        .route("/analytics/user/:user_id/sentiment", get(analytics::get_user_sentiment))
        // Batch processing
        .route("/analytics/batch/comments", post(analytics::batch_process_comments))
        // Phase 6: Team & Organization Aggregations
        .route("/analytics/teams", get(analytics_aggregations::list_teams))
        .route("/analytics/teams/sync", post(analytics_aggregations::sync_teams))
        .route("/analytics/teams/:team_id/members", get(analytics_aggregations::get_team_members))
        .route("/analytics/interactions/aggregate", get(analytics_aggregations::get_interaction_aggregations))
        .route("/analytics/interactions/aggregate/team/:team_id", get(analytics_aggregations::get_team_interaction_aggregations))
        .route("/analytics/aggregations/trigger", post(analytics_aggregations::trigger_aggregation))
        .route("/analytics/network/graph", get(analytics_aggregations::get_participation_network))
        .route("/analytics/network/connectors", get(analytics_aggregations::get_connectors))
        .route("/analytics/interventions", post(analytics_aggregations::record_intervention))
        .route("/analytics/interventions/:id/outcome", post(analytics_aggregations::record_outcome))
        .route("/analytics/learning/effectiveness", get(analytics_aggregations::get_learning_effectiveness))
        .route("/analytics/learning/adjust", post(analytics_aggregations::apply_learning_adjustments))
        // WebSocket for real-time updates
        .route("/realtime", get(realtime::websocket_handler))
        // Trial status (requires auth)
        .route("/trial/status", get(registration::get_trial_status))
        // Trial admin endpoints (requires auth + admin role)
        .route("/trial/eula/accept", post(trial_admin::accept_eula))
        .route("/admin/join-requests", get(trial_admin::list_join_requests))
        .route("/admin/join-requests/:id/approve", post(trial_admin::approve_join_request))
        .route("/admin/join-requests/:id/reject", post(trial_admin::reject_join_request))
        .route("/admin/users/:id/promote", post(trial_admin::promote_to_admin))
        .route("/admin/users/:id/demote", post(trial_admin::demote_from_admin))
        // GDPR & Conversion
        .route("/trial/export", get(trial_gdpr::export_org_data))
        .route("/trial/delete-account", post(trial_gdpr::delete_account))
        .route("/trial/convert", post(trial_gdpr::convert_trial));

    // Entity Embedding Intelligence (requires PostgreSQL)
    #[cfg(feature = "postgres")]
    let protected = protected
        .route("/analytics/embeddings/search", post(analytics::search_entity_embeddings))
        .route("/analytics/embeddings/backfill", post(analytics::backfill_entity_embeddings))
        .route("/analytics/embeddings/backfill-sentiment", post(analytics::backfill_entity_sentiment))
        .route("/analytics/embeddings/stats", get(analytics::get_embedding_stats))
        .route("/analytics/embeddings/patterns", post(analytics::find_entity_patterns));

    // LLM tool interface (requires PostgreSQL)
    #[cfg(feature = "postgres")]
    let protected = protected
        .route("/tools/manifest", get(tools::manifest))
        .route("/tools/execute", post(tools::execute_tool))
        .route("/tools/batch", post(tools::batch_execute));

    // Add GCP-specific routes when gcp feature is enabled
    #[cfg(feature = "gcp")]
    let protected = protected
        .route("/files/sync", post(files::sync_from_gcs))
        .route("/files/gcs-counts", get(files::get_gcs_counts))
        .route("/files/revectorize", post(files::revectorize_chunks))
        .route("/files/migrate-gcs", post(files::migrate_gcs_files))
        // New filename-based upload endpoint (two-phase response)
        .route(
            "/files/upload",
            post(files::upload_file).layer(DefaultBodyLimit::max(max_upload_size)),
        )
        // Bucket-based storage for frontend attachments
        // Routes include org_id for multi-tenancy isolation
        .route(
            "/storage/upload",
            post(storage::upload_storage_file).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/storage/:bucket/list", get(storage::list_storage_files))
        .route("/storage/:bucket/:org_id/*path", get(storage::download_storage_file))
        .route("/storage/:bucket/:org_id/*path", delete(storage::delete_storage_file));

    // Apply JWT auth middleware to protected routes
    let protected = protected.layer(middleware::from_fn(auth::require_auth));

    // Merge public and protected
    public.merge(protected)
}

/// API info endpoint
async fn info() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "ruvector-rag",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "RAG system with document ingestion and citation-aware answers",
        "endpoints": {
            "POST /api/files/upload": "Upload file with original filename (two-phase response)",
            "GET /api/jobs": "List all jobs and queue stats",
            "GET /api/jobs/incomplete": "List incomplete jobs that can be resumed",
            "GET /api/jobs/:id": "Get job progress",
            "GET /api/jobs/:id/files": "Get per-file progress with tier and parser details",
            "POST /api/jobs/:id/resume": "Resume an incomplete/failed job",
            "GET /api/system/parsers": "Get available parsers and their status",
            "POST /api/query": "Query with citations (v1)",
            "POST /api/v2/query": "Query with citations (v2 - frontend-friendly format)",
            "POST /api/v2/query/stream": "Streaming query with SSE (v2 - same request, streamed answer)",
            "POST /api/string-search": "Literal string search",
            "GET /api/documents": "List all documents",
            "GET /api/documents/:id": "Get document details",
            "DELETE /api/documents/:id": "Delete a document",
            "POST /api/documents/:id/archive": "Archive a document (exclude from search)",
            "POST /api/documents/:id/unarchive": "Unarchive a document (re-include in search)",
            "GET /api/files": "List all tracked files with status",
            "POST /api/files/check": "Check file status before upload (deduplication)",
            "GET /api/files/failed": "List failed files with error details",
            "DELETE /api/files/failed": "Clear all failed file records for retry",
            "GET /api/files/stats": "Get file registry statistics",
            "GET /api/files/:filename": "Get specific file status",
            "DELETE /api/files/:filename": "Remove file record for re-upload",
            "POST /api/files/sync": "Sync file registry from GCS bucket (GCP only)",
            "GET /api/files/sync/status": "Get last GCS sync status",
            "GET /api/files/gcs-counts": "Get file counts from GCS bucket (GCP only)",
            "POST /api/files/revectorize": "Re-vectorize chunks to Vertex AI (GCP only)",
            "POST /api/files/migrate-gcs": "Migrate GCS files to organization-specific folders (GCP only)",
            "GET /api/capabilities": "Check document extraction capabilities",
            "POST /api/storage/upload": "Upload file to storage bucket (multipart: organization_id, bucket, path, file)",
            "GET /api/storage/:bucket/list": "List files in storage bucket (query: organization_id, prefix?)",
            "GET /api/storage/:bucket/:org_id/*path": "Download file from storage bucket",
            "DELETE /api/storage/:bucket/:org_id/*path": "Delete file from storage bucket",
            "WS /api/realtime": "WebSocket for real-time database change subscriptions (STUB - PostgreSQL NOTIFY pending)",
            "GET /api/tools/manifest": "MCP-compatible tool definitions for LLM agents",
            "POST /api/tools/execute": "Execute a tool by name (JSON body: {tool, params})",
            "POST /api/tools/batch": "Execute multiple tools in one request"
        },
        "features": {
            "gcs_storage": "Original files and plain text stored in GCS",
            "deduplication": "Content-hash based file deduplication",
            "string_search": "Literal text search for words/phrases",
            "answer_caching": "Cached answers with document-based invalidation",
            "grounded_answers": "LLM uses only document content, no external knowledge",
            "bucket_storage": "Bucket-based file storage for frontend attachments",
            "realtime_websocket": "WebSocket API for database change subscriptions"
        },
        "storage_buckets": [
            "task-attachments",
            "user-avatars",
            "organization-logos",
            "message-attachments",
            "goal-attachments"
        ]
    }))
}

/// Document extraction capabilities endpoint
async fn capabilities() -> axum::Json<serde_json::Value> {
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

    axum::Json(serde_json::json!({
        "tools": {
            "pdftotext": {
                "available": has_pdftotext,
                "purpose": "Fast PDF text extraction",
                "install": "apt install poppler-utils"
            },
            "tesseract": {
                "available": has_tesseract,
                "purpose": "OCR for scanned PDFs and images",
                "install": "apt install tesseract-ocr"
            },
            "pdftoppm": {
                "available": has_pdftoppm,
                "purpose": "PDF to image conversion for OCR",
                "install": "apt install poppler-utils"
            },
            "pandoc": {
                "available": has_pandoc,
                "purpose": "Document conversion (DOCX, RTF, EPUB, ODT)",
                "install": "apt install pandoc"
            },
            "libreoffice": {
                "available": has_libreoffice,
                "purpose": "Legacy format conversion (DOC, PPT, XLS)",
                "install": "apt install libreoffice"
            }
        },
        "formats": {
            "pdf": {
                "native": true,
                "enhanced": has_pdftotext,
                "ocr": has_tesseract && has_pdftoppm,
                "status": if has_pdftotext && has_tesseract { "full" } else if has_pdftotext { "good" } else { "basic" }
            },
            "docx": {
                "native": true,
                "fallback": has_pandoc,
                "status": "full"
            },
            "doc": {
                "native": false,
                "conversion": has_libreoffice,
                "fallback": has_pandoc,
                "status": if has_libreoffice || has_pandoc { "available" } else { "unavailable" }
            },
            "pptx": {
                "native": true,
                "fallback": has_pandoc,
                "status": "full"
            },
            "ppt": {
                "native": false,
                "conversion": has_libreoffice,
                "status": if has_libreoffice { "available" } else { "unavailable" }
            },
            "xlsx": {
                "native": true,
                "status": "full"
            },
            "xls": {
                "native": true,
                "fallback": has_libreoffice,
                "status": "full"
            },
            "rtf": {
                "native": false,
                "conversion": has_pandoc || has_libreoffice,
                "status": if has_pandoc || has_libreoffice { "available" } else { "unavailable" }
            },
            "odt": {
                "native": false,
                "conversion": has_pandoc || has_libreoffice,
                "status": if has_pandoc || has_libreoffice { "available" } else { "unavailable" }
            },
            "odp": {
                "native": false,
                "conversion": has_libreoffice,
                "status": if has_libreoffice { "available" } else { "unavailable" }
            },
            "ods": {
                "native": false,
                "conversion": has_libreoffice,
                "status": if has_libreoffice { "available" } else { "unavailable" }
            },
            "epub": {
                "native": false,
                "conversion": has_pandoc,
                "status": if has_pandoc { "available" } else { "unavailable" }
            },
            "images": {
                "native": false,
                "ocr": has_tesseract,
                "formats": ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff"],
                "status": if has_tesseract { "available" } else { "unavailable" }
            },
            "txt": { "native": true, "status": "full" },
            "md": { "native": true, "status": "full" },
            "html": { "native": true, "status": "full" },
            "csv": { "native": true, "status": "full" },
            "code": { "native": true, "status": "full", "extensions": ["rs", "py", "js", "ts", "go", "java", "cpp", "c", "cs", "rb", "php", "swift", "kt", "sql", "sh", "yaml", "json", "xml", "toml"] }
        },
        "recommendations": {
            "for_scanned_pdfs": if !has_tesseract { Some("Install tesseract-ocr for OCR support") } else { None },
            "for_legacy_office": if !has_libreoffice { Some("Install libreoffice for DOC/PPT/XLS support") } else { None },
            "for_better_pdf": if !has_pdftotext { Some("Install poppler-utils for better PDF extraction") } else { None },
            "for_documents": if !has_pandoc { Some("Install pandoc for RTF/EPUB/ODT support") } else { None }
        }
    }))
}
