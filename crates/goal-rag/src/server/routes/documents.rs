//! Document management endpoints

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::server::routes::acl;
use crate::server::routes::auth::AuthClaims;
use crate::server::state::AppState;
use crate::types::response::{DocumentListResponse, DocumentSummary};

/// Query parameters for listing documents
#[derive(Debug, Deserialize)]
pub struct ListDocumentsQuery {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
}

/// GET /api/documents - List all documents for an organization
/// Uses rag_file_registry as source of truth to show ORIGINAL uploaded files
/// (not the plaintext-extracted versions used internally for RAG)
pub async fn list_documents(
    State(state): State<AppState>,
    Query(query): Query<ListDocumentsQuery>,
    claims: Option<Extension<AuthClaims>>,
) -> Result<Json<DocumentListResponse>> {
    // Query rag_file_registry for original documents (not plaintext extractions)
    if let Some(pool) = state.pg_pool() {
        let client = pool.get().await.map_err(|e| Error::Internal(e.to_string()))?;

        // ACL filtering: get accessible document IDs for this user
        let acl_doc_ids: Option<Vec<Uuid>> = if let Some(Extension(ref c)) = claims {
            acl::resolve_acl_filter(&state, c, None).await?
        } else {
            None
        };

        // Build query with optional ACL filter
        let rows = if let Some(ref doc_ids) = acl_doc_ids {
            client.query(
                "SELECT fr.document_id, fr.filename, fr.file_type, fr.file_size,
                        fr.chunks_created, fr.last_processed_at,
                        EXISTS(SELECT 1 FROM rag_chunks rc
                               WHERE rc.document_id = fr.document_id
                               AND rc.archived_at IS NOT NULL LIMIT 1) AS archived
                 FROM rag_file_registry fr
                 WHERE fr.organization_id = $1
                   AND fr.status = 'success'
                   AND fr.document_id IS NOT NULL
                   AND fr.document_id = ANY($2)
                 ORDER BY fr.last_processed_at DESC",
                &[&query.organization_id, doc_ids],
            ).await
        } else {
            client.query(
                "SELECT fr.document_id, fr.filename, fr.file_type, fr.file_size,
                        fr.chunks_created, fr.last_processed_at,
                        EXISTS(SELECT 1 FROM rag_chunks rc
                               WHERE rc.document_id = fr.document_id
                               AND rc.archived_at IS NOT NULL LIMIT 1) AS archived
                 FROM rag_file_registry fr
                 WHERE fr.organization_id = $1
                   AND fr.status = 'success'
                   AND fr.document_id IS NOT NULL
                 ORDER BY fr.last_processed_at DESC",
                &[&query.organization_id],
            ).await
        }.map_err(|e| Error::Internal(format!("Failed to query file registry: {e}")))?;

        let documents: Vec<DocumentSummary> = rows.iter().map(|r| {
            let doc_id: Option<Uuid> = r.try_get("document_id").ok().flatten();
            let file_type_str: String = r.try_get("file_type").unwrap_or_default();
            let ingested: Option<chrono::DateTime<chrono::Utc>> = r.try_get("last_processed_at").ok().flatten();

            DocumentSummary {
                id: doc_id.unwrap_or(Uuid::nil()),
                filename: r.try_get("filename").unwrap_or_default(),
                file_type: crate::types::document::FileType::from_extension(&file_type_str),
                total_pages: None,
                total_chunks: r.try_get::<_, Option<i32>>("chunks_created").ok().flatten().unwrap_or(0) as u32,
                file_size: r.try_get::<_, Option<i64>>("file_size").ok().flatten().unwrap_or(0) as u64,
                ingested_at: ingested.unwrap_or_else(chrono::Utc::now),
                archived: r.try_get("archived").unwrap_or(false),
            }
        }).collect();

        let total_count = documents.len();
        Ok(Json(DocumentListResponse { documents, total_count }))
    } else {
        // Fallback to in-memory registry if PostgreSQL not available
        let archived_doc_ids: HashSet<Uuid> = HashSet::new();
        let documents: Vec<DocumentSummary> = state
            .documents()
            .iter()
            .filter(|entry| entry.value().organization_id.as_ref() == Some(&query.organization_id))
            .map(|entry| {
                let mut summary = DocumentSummary::from(entry.value());
                summary.archived = archived_doc_ids.contains(&summary.id);
                summary
            })
            .collect();
        let total_count = documents.len();
        Ok(Json(DocumentListResponse { documents, total_count }))
    }
}

/// Query parameters for document operations requiring org context
#[derive(Debug, Deserialize)]
pub struct OrgQuery {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
}

/// GET /api/documents/:id - Get a specific document
pub async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
    claims: Option<Extension<AuthClaims>>,
) -> Result<Json<DocumentSummary>> {
    let doc = state
        .get_document(&id)
        .ok_or_else(|| Error::DocumentNotFound(id.to_string()))?;

    if doc.organization_id.as_ref() != Some(&query.organization_id) {
        return Err(Error::DocumentNotFound(format!(
            "Document {} not found in organization {}", id, query.organization_id
        )));
    }

    if let Some(Extension(ref c)) = claims {
        acl::enforce_document_acl(&state, c, &id).await?;
    }

    Ok(Json(DocumentSummary::from(&doc)))
}

/// DELETE /api/documents/:id - Delete a document
pub async fn delete_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
    claims: Option<Extension<AuthClaims>>,
) -> Result<Json<serde_json::Value>> {
    let doc = state
        .get_document(&id)
        .ok_or_else(|| Error::DocumentNotFound(id.to_string()))?;

    if doc.organization_id.as_ref() != Some(&query.organization_id) {
        return Err(Error::DocumentNotFound(format!(
            "Document {} not found in organization {}", id, query.organization_id
        )));
    }

    if let Some(Extension(ref c)) = claims {
        acl::enforce_document_acl(&state, c, &id).await?;
    }

    // Delete all chunks first so that if this fails the document remains visible and
    // the operation can be retried.  Only remove from the in-memory registry once the
    // vector store has been cleaned up successfully.
    let deleted_chunks = state.vector_store_provider().delete_by_document(&id).await?;

    // Remove document from registry (safe – chunks are already gone)
    let doc = state
        .remove_document(&id)
        .ok_or_else(|| Error::DocumentNotFound(id.to_string()))?;

    tracing::info!(
        "Deleted document '{}' from org '{}' and {} chunks",
        doc.filename,
        query.organization_id,
        deleted_chunks
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "document_id": id,
        "organization_id": query.organization_id,
        "filename": doc.filename,
        "deleted_chunks": deleted_chunks
    })))
}

/// POST /api/documents/:id/archive - Archive a document (exclude chunks from search)
pub async fn archive_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
    claims: Option<Extension<AuthClaims>>,
) -> Result<Json<serde_json::Value>> {
    // Verify the document exists and belongs to the organization
    let doc = state
        .get_document(&id)
        .ok_or_else(|| Error::DocumentNotFound(id.to_string()))?;

    if doc.organization_id.as_ref() != Some(&query.organization_id) {
        return Err(Error::DocumentNotFound(format!(
            "Document {} not found in organization {}",
            id, query.organization_id
        )));
    }

    if let Some(Extension(ref c)) = claims {
        acl::enforce_document_acl(&state, c, &id).await?;
    }

    let pool = state
        .pg_pool()
        .ok_or_else(|| Error::Internal("PostgreSQL not available".into()))?;
    let client = pool.get().await?;

    let updated = client
        .execute(
            "UPDATE rag_chunks SET archived_at = NOW() WHERE document_id = $1 AND organization_id = $2 AND archived_at IS NULL",
            &[&id, &query.organization_id],
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to archive document: {}", e)))?;

    tracing::info!(
        "Archived document '{}' in org '{}' ({} chunks)",
        doc.filename,
        query.organization_id,
        updated
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "document_id": id,
        "organization_id": query.organization_id,
        "archived_chunks": updated
    })))
}

/// POST /api/documents/:id/unarchive - Unarchive a document (re-include chunks in search)
pub async fn unarchive_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<OrgQuery>,
    claims: Option<Extension<AuthClaims>>,
) -> Result<Json<serde_json::Value>> {
    let doc = state
        .get_document(&id)
        .ok_or_else(|| Error::DocumentNotFound(id.to_string()))?;

    if doc.organization_id.as_ref() != Some(&query.organization_id) {
        return Err(Error::DocumentNotFound(format!(
            "Document {} not found in organization {}",
            id, query.organization_id
        )));
    }

    if let Some(Extension(ref c)) = claims {
        acl::enforce_document_acl(&state, c, &id).await?;
    }

    let pool = state
        .pg_pool()
        .ok_or_else(|| Error::Internal("PostgreSQL not available".into()))?;
    let client = pool.get().await?;

    let updated = client
        .execute(
            "UPDATE rag_chunks SET archived_at = NULL WHERE document_id = $1 AND organization_id = $2 AND archived_at IS NOT NULL",
            &[&id, &query.organization_id],
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to unarchive document: {}", e)))?;

    tracing::info!(
        "Unarchived document '{}' in org '{}' ({} chunks)",
        doc.filename,
        query.organization_id,
        updated
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "document_id": id,
        "organization_id": query.organization_id,
        "unarchived_chunks": updated
    })))
}

/// Query params for document download
#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    pub organization_id: String,
    /// Optional: "pdf" to convert Office docs to PDF for viewing
    #[serde(default)]
    pub format: Option<String>,
}

/// GET /api/documents/:id/download - Download original document
/// Serves from local disk first, falls back to GCS.
/// Pass ?format=pdf to convert Office docs (doc/docx/xls/xlsx/ppt/pptx) to PDF for inline viewing.
pub async fn download_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<DownloadQuery>,
    claims: Option<Extension<AuthClaims>>,
) -> Result<Response> {
    // ACL check before serving file
    if let Some(Extension(ref c)) = claims {
        acl::enforce_document_acl(&state, c, &id).await?;
    }

    let org_id = &query.organization_id;

    // Look up filenames from multiple sources
    let mut filenames_to_try: Vec<String> = Vec::new();

    // 1. From rag_file_registry (gcs_filename + filename)
    if let Some(pool) = state.pg_pool() {
        if let Ok(client) = pool.get().await {
            if let Ok(Some(row)) = client.query_opt(
                "SELECT filename, gcs_filename FROM rag_file_registry WHERE document_id = $1 AND organization_id = $2",
                &[&id, org_id],
            ).await {
                if let Ok(Some(gcs)) = row.try_get::<_, Option<String>>("gcs_filename") {
                    filenames_to_try.push(gcs);
                }
                if let Ok(name) = row.try_get::<_, String>("filename") {
                    if !filenames_to_try.contains(&name) {
                        filenames_to_try.push(name);
                    }
                }
            }
            // From rag_chunks
            if let Ok(Some(row)) = client.query_opt(
                "SELECT DISTINCT filename FROM rag_chunks WHERE document_id = $1 LIMIT 1",
                &[&id],
            ).await {
                if let Ok(name) = row.try_get::<_, String>("filename") {
                    if !filenames_to_try.contains(&name) {
                        filenames_to_try.push(name);
                    }
                }
            }
        }
    }

    // 2. From in-memory registry
    if let Some(doc) = state.get_document(&id) {
        if !filenames_to_try.contains(&doc.filename) {
            filenames_to_try.push(doc.filename.clone());
        }
    }

    // Add extension variants (docx↔doc, xlsx↔xls, pptx↔ppt)
    let mut alt_names: Vec<String> = Vec::new();
    for name in &filenames_to_try {
        if name.ends_with("docx") { alt_names.push(name[..name.len()-1].to_string()); }
        else if name.ends_with("xlsx") { alt_names.push(name[..name.len()-1].to_string()); }
        else if name.ends_with("pptx") { alt_names.push(name[..name.len()-1].to_string()); }
        else if name.ends_with("doc") { alt_names.push(format!("{}x", name)); }
        else if name.ends_with("xls") { alt_names.push(format!("{}x", name)); }
        else if name.ends_with("ppt") { alt_names.push(format!("{}x", name)); }
    }
    for alt in alt_names {
        if !filenames_to_try.contains(&alt) {
            filenames_to_try.push(alt);
        }
    }

    // LOCAL DISK: Try /var/lib/ruvector-rag/documents/originals/{org_id}/{filename}
    let local_base = std::path::PathBuf::from("/var/lib/ruvector-rag/documents/originals").join(org_id);
    let mut found_data: Option<Vec<u8>> = None;
    let mut found_filename = String::new();

    // Try each filename on local disk
    for name in &filenames_to_try {
        let path = local_base.join(name);
        if path.exists() {
            if let Ok(data) = tokio::fs::read(&path).await {
                found_data = Some(data);
                found_filename = name.clone();
                break;
            }
        }
    }

    // If not found by name, scan .meta.json files on disk for matching content_hash
    if found_data.is_none() {
        // Get content_hash from registry for matching
        let mut registry_hash: Option<String> = None;
        if let Some(pool) = state.pg_pool() {
            if let Ok(client) = pool.get().await {
                if let Ok(Some(row)) = client.query_opt(
                    "SELECT content_hash FROM rag_file_registry WHERE document_id = $1 AND organization_id = $2",
                    &[&id, org_id],
                ).await {
                    registry_hash = row.try_get::<_, String>("content_hash").ok();
                }
            }
        }

        if let Ok(entries) = std::fs::read_dir(&local_base) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if !fname.ends_with(".meta.json") { continue; }
                if let Ok(meta_bytes) = std::fs::read(entry.path()) {
                    if let Ok(meta) = serde_json::from_slice::<serde_json::Value>(&meta_bytes) {
                        let meta_hash = meta.get("content_hash").and_then(|v| v.as_str()).unwrap_or("");
                        let meta_name = meta.get("filename").and_then(|v| v.as_str()).unwrap_or("");
                        let actual_file = fname.trim_end_matches(".meta.json");
                        let actual_path = local_base.join(actual_file);
                        if !actual_path.exists() { continue; }

                        // Match by: filename, content_hash, or meta filename
                        let name_match = filenames_to_try.iter().any(|n| n == meta_name || n == actual_file);
                        let hash_match = registry_hash.as_ref().map(|h| h == meta_hash).unwrap_or(false);

                        if name_match || hash_match {
                            if let Ok(data) = tokio::fs::read(&actual_path).await {
                                found_data = Some(data);
                                found_filename = actual_file.to_string();
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // FALLBACK: GCS download removed — document_store() not available in trial build

    let data = found_data.ok_or_else(|| Error::Internal(format!(
        "Document not found. Tried filenames: {}",
        filenames_to_try.join(", ")
    )))?;

    let display_filename = if found_filename.is_empty() {
        filenames_to_try.first().cloned().unwrap_or_else(|| format!("{}.bin", id))
    } else {
        found_filename
    };

    // If format=pdf requested and file is an Office doc, convert via LibreOffice
    let ext = display_filename.rsplit('.').next().unwrap_or("").to_lowercase();
    let is_office = matches!(ext.as_str(), "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf");
    let want_pdf = query.format.as_deref() == Some("pdf");

    let (data, display_filename) = if want_pdf && is_office {
        // Convert to PDF using LibreOffice
        match convert_to_pdf(&data, &display_filename).await {
            Ok(pdf_data) => {
                let pdf_name = format!("{}.pdf", display_filename.rsplit_once('.').map(|(b,_)| b).unwrap_or(&display_filename));
                (pdf_data, pdf_name)
            }
            Err(e) => {
                tracing::warn!("PDF conversion failed for {}: {}", display_filename, e);
                (data, display_filename) // Fall back to original
            }
        }
    } else {
        (data, display_filename)
    };

    // Sanitize filename for Content-Disposition header
    let safe_filename = display_filename
        .replace('"', "")
        .replace('\\', "")
        .replace('\n', "")
        .replace('\r', "");

    // Determine content type from extension
    let content_type = match display_filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "txt" | "md" | "csv" => "text/plain; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    };

    tracing::info!(
        doc_id = %id,
        org = %org_id,
        filename = %safe_filename,
        size = data.len(),
        "Document downloaded from local storage"
    );

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, format!("inline; filename=\"{}\"", safe_filename)),
            (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
        ],
        data,
    ).into_response())
}

/// Convert an Office document to PDF using LibreOffice (headless)
async fn convert_to_pdf(data: &[u8], filename: &str) -> std::result::Result<Vec<u8>, String> {
    use std::process::Stdio;

    // Write source file to temp dir
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("tmpdir: {e}"))?;
    let src_path = tmp_dir.path().join(filename);
    tokio::fs::write(&src_path, data).await.map_err(|e| format!("write: {e}"))?;

    // Run LibreOffice headless conversion
    let output = tokio::process::Command::new("libreoffice")
        .args([
            "--headless",
            "--convert-to", "pdf",
            "--outdir", tmp_dir.path().to_str().unwrap_or("/tmp"),
            src_path.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("libreoffice exec: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("libreoffice failed: {}", stderr.chars().take(200).collect::<String>()));
    }

    // Read the generated PDF
    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let pdf_path = tmp_dir.path().join(format!("{}.pdf", stem));

    tokio::fs::read(&pdf_path)
        .await
        .map_err(|e| format!("read pdf: {e}"))
}
