//! PostgreSQL-based file registry storage
//!
//! Replaces SQLite FileRegistryDb for production builds with --features postgres.
//! All methods are async, using deadpool-postgres connection pool.

use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use crate::types::{Document, FileRecord, FileRecordStatus, FileType};

// Re-use the persisted types from the SQLite module (they're just data structs)
use super::database::{
    ChunkContentRecord, ChunkSearchResult,
    FileRegistryDbStats, JobFileRecord, JobFileStatus, JobRecord,
    PersistedJobStage, PersistedJobStatus, SyncStatus,
};

/// PostgreSQL-based file registry database.
/// Replaces SQLite FileRegistryDb for postgres builds.
pub struct PgFileRegistry {
    pool: Arc<PgPool>,
}

impl PgFileRegistry {
    /// Create a new PgFileRegistry and initialize schema
    pub async fn new(pool: Arc<PgPool>) -> Result<Self> {
        let registry = Self { pool };
        registry.init_schema().await?;
        Ok(registry)
    }

    /// Initialize PostgreSQL schema (creates tables if they don't exist)
    async fn init_schema(&self) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .batch_execute(
                r#"
            -- File registry table (replaces SQLite file_registry)
            CREATE TABLE IF NOT EXISTS rag_file_registry (
                id UUID PRIMARY KEY,
                organization_id TEXT NOT NULL,
                filename TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                file_size BIGINT NOT NULL,
                file_type TEXT NOT NULL,
                status TEXT NOT NULL,
                document_id UUID,
                chunks_created INTEGER,
                skip_reason JSONB,
                error_message TEXT,
                failed_at_stage TEXT,
                job_id UUID,
                first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                last_processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                upload_count INTEGER NOT NULL DEFAULT 1,
                original_url TEXT,
                plaintext_url TEXT,
                gcs_synced BOOLEAN NOT NULL DEFAULT FALSE,
                UNIQUE(organization_id, filename)
            );

            CREATE INDEX IF NOT EXISTS idx_rag_file_registry_status ON rag_file_registry(status);
            CREATE INDEX IF NOT EXISTS idx_rag_file_registry_content_hash ON rag_file_registry(content_hash);
            CREATE INDEX IF NOT EXISTS idx_rag_file_registry_document_id ON rag_file_registry(document_id);
            CREATE INDEX IF NOT EXISTS idx_rag_file_registry_org_id ON rag_file_registry(organization_id);
            CREATE INDEX IF NOT EXISTS idx_rag_file_registry_filename ON rag_file_registry(filename);

            -- Documents table (replaces SQLite documents)
            CREATE TABLE IF NOT EXISTS rag_documents (
                id UUID PRIMARY KEY,
                organization_id TEXT,
                filename TEXT NOT NULL,
                internal_filename TEXT,
                file_type TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                file_size BIGINT NOT NULL,
                total_chunks INTEGER DEFAULT 0,
                total_pages INTEGER,
                ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                metadata JSONB DEFAULT '{}'::jsonb
            );

            CREATE INDEX IF NOT EXISTS idx_rag_documents_filename ON rag_documents(filename);
            CREATE INDEX IF NOT EXISTS idx_rag_documents_content_hash ON rag_documents(content_hash);
            CREATE INDEX IF NOT EXISTS idx_rag_documents_org_id ON rag_documents(organization_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_rag_documents_org_filename
                ON rag_documents(organization_id, filename);

            -- Sync status table (replaces SQLite sync_status)
            CREATE TABLE IF NOT EXISTS rag_sync_status (
                id INTEGER PRIMARY KEY DEFAULT 1,
                last_gcs_sync TIMESTAMPTZ,
                files_synced INTEGER DEFAULT 0,
                sync_duration_ms BIGINT
            );

            -- Initialize sync status if not exists
            INSERT INTO rag_sync_status (id, last_gcs_sync, files_synced)
            VALUES (1, NULL, 0)
            ON CONFLICT (id) DO NOTHING;

            -- Jobs table (replaces SQLite jobs)
            CREATE TABLE IF NOT EXISTS rag_jobs (
                id UUID PRIMARY KEY,
                status TEXT NOT NULL,
                stage TEXT NOT NULL,
                total_files INTEGER NOT NULL,
                files_processed INTEGER NOT NULL DEFAULT 0,
                files_skipped INTEGER NOT NULL DEFAULT 0,
                files_failed INTEGER NOT NULL DEFAULT 0,
                total_chunks INTEGER NOT NULL DEFAULT 0,
                chunks_embedded INTEGER NOT NULL DEFAULT 0,
                current_file TEXT,
                error TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                completed_at TIMESTAMPTZ,
                options_json JSONB
            );

            CREATE INDEX IF NOT EXISTS idx_rag_jobs_status ON rag_jobs(status);
            CREATE INDEX IF NOT EXISTS idx_rag_jobs_created_at ON rag_jobs(created_at);

            -- Job files table (replaces SQLite job_files)
            CREATE TABLE IF NOT EXISTS rag_job_files (
                id BIGSERIAL PRIMARY KEY,
                job_id UUID NOT NULL REFERENCES rag_jobs(id) ON DELETE CASCADE,
                filename TEXT NOT NULL,
                file_size BIGINT NOT NULL,
                content_hash TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                tier TEXT,
                parser_method TEXT,
                error TEXT,
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                duration_ms BIGINT,
                file_data BYTEA,
                UNIQUE(job_id, filename)
            );

            CREATE INDEX IF NOT EXISTS idx_rag_job_files_job_id ON rag_job_files(job_id);
            CREATE INDEX IF NOT EXISTS idx_rag_job_files_status ON rag_job_files(status);
            "#,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to initialize PG registry schema: {}", e)))?;

        tracing::info!("PgFileRegistry schema initialized");
        Ok(())
    }

    // ==================== File Registry Operations ====================

    /// Insert or update a file record
    pub async fn upsert_file_record(&self, record: &FileRecord) -> Result<()> {
        let client = self.pool.get().await?;

        let skip_reason_json: Option<serde_json::Value> = record
            .skip_reason
            .as_ref()
            .and_then(|r| serde_json::to_value(r).ok());

        client
            .execute(
                r#"
                INSERT INTO rag_file_registry (
                    id, organization_id, filename, content_hash, file_size, file_type, status,
                    document_id, chunks_created, skip_reason, error_message, failed_at_stage,
                    job_id, first_seen_at, last_processed_at, upload_count, original_url, plaintext_url
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
                ON CONFLICT(organization_id, filename) DO UPDATE SET
                    content_hash = EXCLUDED.content_hash,
                    file_size = EXCLUDED.file_size,
                    file_type = EXCLUDED.file_type,
                    status = EXCLUDED.status,
                    document_id = EXCLUDED.document_id,
                    chunks_created = EXCLUDED.chunks_created,
                    skip_reason = EXCLUDED.skip_reason,
                    error_message = EXCLUDED.error_message,
                    failed_at_stage = EXCLUDED.failed_at_stage,
                    job_id = EXCLUDED.job_id,
                    last_processed_at = EXCLUDED.last_processed_at,
                    upload_count = rag_file_registry.upload_count + 1,
                    original_url = COALESCE(EXCLUDED.original_url, rag_file_registry.original_url),
                    plaintext_url = COALESCE(EXCLUDED.plaintext_url, rag_file_registry.plaintext_url)
                "#,
                &[
                    &record.id,
                    &record.organization_id,
                    &record.filename,
                    &record.content_hash,
                    &(record.file_size as i64),
                    &file_type_to_ext(&record.file_type),
                    &status_to_str(&record.status),
                    &record.document_id,
                    &record.chunks_created.map(|c| c as i32),
                    &skip_reason_json,
                    &record.error_message,
                    &record.failed_at_stage,
                    &record.job_id,
                    &record.first_seen_at,
                    &record.last_processed_at,
                    &(record.upload_count as i32),
                    &record.original_url,
                    &record.plaintext_url,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upsert file record: {}", e)))?;

        Ok(())
    }

    /// Get a file record by filename
    pub async fn get_file_record(&self, filename: &str) -> Result<Option<FileRecord>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                r#"SELECT id, organization_id, filename, content_hash, file_size, file_type, status,
                          document_id, chunks_created, skip_reason, error_message, failed_at_stage,
                          job_id, first_seen_at, last_processed_at, upload_count, original_url, plaintext_url
                   FROM rag_file_registry WHERE filename = $1"#,
                &[&filename],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get file record: {}", e)))?;

        Ok(row.map(|r| row_to_file_record(&r)))
    }

    /// Get a file record by content hash
    pub async fn get_file_record_by_hash(&self, content_hash: &str) -> Result<Option<FileRecord>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                r#"SELECT id, organization_id, filename, content_hash, file_size, file_type, status,
                          document_id, chunks_created, skip_reason, error_message, failed_at_stage,
                          job_id, first_seen_at, last_processed_at, upload_count, original_url, plaintext_url
                   FROM rag_file_registry WHERE content_hash = $1 LIMIT 1"#,
                &[&content_hash],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get file record by hash: {}", e)))?;

        Ok(row.map(|r| row_to_file_record(&r)))
    }

    /// List all file records
    pub async fn list_file_records(&self) -> Result<Vec<FileRecord>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT id, organization_id, filename, content_hash, file_size, file_type, status,
                          document_id, chunks_created, skip_reason, error_message, failed_at_stage,
                          job_id, first_seen_at, last_processed_at, upload_count, original_url, plaintext_url
                   FROM rag_file_registry ORDER BY last_processed_at DESC"#,
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to list file records: {}", e)))?;

        Ok(rows.iter().map(row_to_file_record).collect())
    }

    /// List file records by status
    pub async fn list_by_status(&self, status: FileRecordStatus) -> Result<Vec<FileRecord>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT id, organization_id, filename, content_hash, file_size, file_type, status,
                          document_id, chunks_created, skip_reason, error_message, failed_at_stage,
                          job_id, first_seen_at, last_processed_at, upload_count, original_url, plaintext_url
                   FROM rag_file_registry WHERE status = $1 ORDER BY last_processed_at DESC"#,
                &[&status_to_str(&status)],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to list by status: {}", e)))?;

        Ok(rows.iter().map(row_to_file_record).collect())
    }

    /// Delete a file record
    pub async fn delete_file_record(&self, filename: &str) -> Result<bool> {
        let client = self.pool.get().await?;

        let count = client
            .execute(
                "DELETE FROM rag_file_registry WHERE filename = $1",
                &[&filename],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to delete file record: {}", e)))?;

        Ok(count > 0)
    }

    /// Clear all failed file records
    pub async fn clear_failed_files(&self) -> Result<usize> {
        let client = self.pool.get().await?;

        let count = client
            .execute(
                "DELETE FROM rag_file_registry WHERE status = 'failed'",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to clear failed files: {}", e)))?;

        Ok(count as usize)
    }

    /// Get file registry statistics
    pub async fn get_stats(&self) -> Result<FileRegistryDbStats> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                r#"SELECT
                    COUNT(*) as total,
                    COUNT(*) FILTER (WHERE status = 'success') as success,
                    COUNT(*) FILTER (WHERE status = 'failed') as failed,
                    COUNT(*) FILTER (WHERE status = 'skipped') as skipped
                   FROM rag_file_registry"#,
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get stats: {}", e)))?;

        Ok(FileRegistryDbStats {
            total: row.get::<_, i64>("total") as usize,
            success: row.get::<_, i64>("success") as usize,
            failed: row.get::<_, i64>("failed") as usize,
            skipped: row.get::<_, i64>("skipped") as usize,
        })
    }

    // ==================== Document Operations ====================

    /// Insert or update a document record
    pub async fn upsert_document(&self, doc: &Document) -> Result<()> {
        let client = self.pool.get().await?;

        let metadata_json: serde_json::Value =
            serde_json::to_value(&doc.metadata).unwrap_or(serde_json::Value::Object(Default::default()));

        client
            .execute(
                r#"
                INSERT INTO rag_documents (
                    id, organization_id, filename, internal_filename, file_type, content_hash,
                    file_size, total_chunks, total_pages, ingested_at, metadata
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT(id) DO UPDATE SET
                    organization_id = EXCLUDED.organization_id,
                    filename = EXCLUDED.filename,
                    internal_filename = EXCLUDED.internal_filename,
                    file_type = EXCLUDED.file_type,
                    content_hash = EXCLUDED.content_hash,
                    file_size = EXCLUDED.file_size,
                    total_chunks = EXCLUDED.total_chunks,
                    total_pages = EXCLUDED.total_pages,
                    metadata = EXCLUDED.metadata
                "#,
                &[
                    &doc.id,
                    &doc.organization_id,
                    &doc.filename,
                    &doc.internal_filename,
                    &file_type_to_ext(&doc.file_type),
                    &doc.content_hash,
                    &(doc.file_size as i64),
                    &(doc.total_chunks as i32),
                    &doc.total_pages.map(|p| p as i32),
                    &doc.ingested_at,
                    &metadata_json,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upsert document: {}", e)))?;

        Ok(())
    }

    /// Get a document by ID
    pub async fn get_document(&self, id: &Uuid) -> Result<Option<Document>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                r#"SELECT id, organization_id, filename, internal_filename, file_type, content_hash,
                          file_size, total_chunks, total_pages, ingested_at, metadata
                   FROM rag_documents WHERE id = $1"#,
                &[id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get document: {}", e)))?;

        Ok(row.map(|r| row_to_document(&r)))
    }

    /// Get a document by filename
    pub async fn get_document_by_filename(&self, filename: &str) -> Result<Option<Document>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                r#"SELECT id, organization_id, filename, internal_filename, file_type, content_hash,
                          file_size, total_chunks, total_pages, ingested_at, metadata
                   FROM rag_documents WHERE filename = $1"#,
                &[&filename],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get document by filename: {}", e)))?;

        Ok(row.map(|r| row_to_document(&r)))
    }

    /// Get a document by organization_id + filename
    pub async fn get_document_by_org_filename(
        &self,
        organization_id: &str,
        filename: &str,
    ) -> Result<Option<Document>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                r#"SELECT id, organization_id, filename, internal_filename, file_type, content_hash,
                          file_size, total_chunks, total_pages, ingested_at, metadata
                   FROM rag_documents WHERE organization_id = $1 AND filename = $2"#,
                &[&organization_id, &filename],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get document by org+filename: {}", e)))?;

        Ok(row.map(|r| row_to_document(&r)))
    }

    /// List all documents
    pub async fn list_documents(&self) -> Result<Vec<Document>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT id, organization_id, filename, internal_filename, file_type, content_hash,
                          file_size, total_chunks, total_pages, ingested_at, metadata
                   FROM rag_documents ORDER BY ingested_at DESC"#,
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to list documents: {}", e)))?;

        Ok(rows.iter().map(row_to_document).collect())
    }

    /// List all documents for an organization
    pub async fn list_documents_by_org(&self, organization_id: &str) -> Result<Vec<Document>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT id, organization_id, filename, internal_filename, file_type, content_hash,
                          file_size, total_chunks, total_pages, ingested_at, metadata
                   FROM rag_documents WHERE organization_id = $1 ORDER BY ingested_at DESC"#,
                &[&organization_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to list documents by org: {}", e)))?;

        Ok(rows.iter().map(row_to_document).collect())
    }

    /// Delete a document by ID
    pub async fn delete_document(&self, id: &Uuid) -> Result<bool> {
        let client = self.pool.get().await?;

        let count = client
            .execute("DELETE FROM rag_documents WHERE id = $1", &[id])
            .await
            .map_err(|e| Error::Internal(format!("Failed to delete document: {}", e)))?;

        Ok(count > 0)
    }

    /// Delete a document by organization_id + filename
    pub async fn delete_document_by_org_filename(
        &self,
        organization_id: &str,
        filename: &str,
    ) -> Result<bool> {
        let client = self.pool.get().await?;

        let count = client
            .execute(
                "DELETE FROM rag_documents WHERE organization_id = $1 AND filename = $2",
                &[&organization_id, &filename],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to delete document: {}", e)))?;

        Ok(count > 0)
    }

    /// Check if a document exists by organization_id + filename
    pub async fn document_exists_by_org_filename(
        &self,
        organization_id: &str,
        filename: &str,
    ) -> Result<bool> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "SELECT COUNT(*) FROM rag_documents WHERE organization_id = $1 AND filename = $2",
                &[&organization_id, &filename],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to check document: {}", e)))?;

        let count: i64 = row.get(0);
        Ok(count > 0)
    }

    /// Get document count
    pub async fn get_document_count(&self) -> Result<usize> {
        let client = self.pool.get().await?;

        let row = client
            .query_one("SELECT COUNT(*) FROM rag_documents", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to count documents: {}", e)))?;

        let count: i64 = row.get(0);
        Ok(count as usize)
    }

    // ==================== GCS Sync Operations ====================

    /// Record a file discovered from GCS sync
    #[allow(clippy::too_many_arguments)]
    pub async fn sync_from_gcs(
        &self,
        filename: &str,
        document_id: Uuid,
        content_hash: &str,
        file_size: u64,
        file_type: &str,
        has_plaintext: bool,
        original_url: &str,
        plaintext_url: Option<&str>,
    ) -> Result<()> {
        let client = self.pool.get().await?;

        let status = if has_plaintext { "success" } else { "failed" };
        let error_message: Option<String> = if has_plaintext {
            None
        } else {
            Some("No plaintext found in GCS - processing may have failed".to_string())
        };

        client
            .execute(
                r#"
                INSERT INTO rag_file_registry (
                    id, organization_id, filename, content_hash, file_size, file_type, status,
                    document_id, chunks_created, error_message, first_seen_at,
                    last_processed_at, upload_count, original_url, plaintext_url, gcs_synced
                ) VALUES ($1, 'unknown', $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW(), 1, $10, $11, TRUE)
                ON CONFLICT(organization_id, filename) DO UPDATE SET
                    document_id = COALESCE(EXCLUDED.document_id, rag_file_registry.document_id),
                    original_url = COALESCE(EXCLUDED.original_url, rag_file_registry.original_url),
                    plaintext_url = COALESCE(EXCLUDED.plaintext_url, rag_file_registry.plaintext_url),
                    gcs_synced = TRUE
                "#,
                &[
                    &document_id,
                    &filename,
                    &content_hash,
                    &(file_size as i64),
                    &file_type,
                    &status,
                    &document_id,
                    &if has_plaintext { Some(0i32) } else { None },
                    &error_message,
                    &original_url,
                    &plaintext_url,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to sync from GCS: {}", e)))?;

        Ok(())
    }

    /// Update last GCS sync timestamp
    pub async fn update_sync_status(&self, files_synced: usize, duration_ms: u64) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE rag_sync_status SET last_gcs_sync = NOW(), files_synced = $1, sync_duration_ms = $2 WHERE id = 1",
                &[&(files_synced as i32), &(duration_ms as i64)],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to update sync status: {}", e)))?;

        Ok(())
    }

    /// Get last sync status
    pub async fn get_sync_status(&self) -> Result<Option<SyncStatus>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                "SELECT last_gcs_sync, files_synced, sync_duration_ms FROM rag_sync_status WHERE id = 1",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get sync status: {}", e)))?;

        Ok(row.map(|r| {
            let last_gcs_sync: Option<DateTime<Utc>> = r.get("last_gcs_sync");
            let files_synced: i32 = r.get("files_synced");
            let sync_duration_ms: Option<i64> = r.get("sync_duration_ms");

            SyncStatus {
                last_gcs_sync,
                files_synced: files_synced as usize,
                sync_duration_ms: sync_duration_ms.map(|d| d as u64),
            }
        }))
    }

    // ==================== Job Persistence Operations ====================

    /// Create a new job record
    pub async fn create_job(&self, job: &JobRecord) -> Result<()> {
        let client = self.pool.get().await?;

        let options_json: Option<serde_json::Value> = job
            .options
            .as_ref()
            .and_then(|o| serde_json::to_value(o).ok());

        client
            .execute(
                r#"
                INSERT INTO rag_jobs (
                    id, status, stage, total_files, files_processed, files_skipped,
                    files_failed, total_chunks, chunks_embedded, current_file, error,
                    created_at, updated_at, completed_at, options_json
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                "#,
                &[
                    &job.id,
                    &job_status_to_str(&job.status),
                    &job_stage_to_str(&job.stage),
                    &(job.total_files as i32),
                    &(job.files_processed as i32),
                    &(job.files_skipped as i32),
                    &(job.files_failed as i32),
                    &(job.total_chunks as i32),
                    &(job.chunks_embedded as i32),
                    &job.current_file,
                    &job.error,
                    &job.created_at,
                    &job.updated_at,
                    &job.completed_at,
                    &options_json,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to create job: {}", e)))?;

        Ok(())
    }

    /// Update job progress
    pub async fn update_job(&self, job: &JobRecord) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .execute(
                r#"
                UPDATE rag_jobs SET
                    status = $2,
                    stage = $3,
                    files_processed = $4,
                    files_skipped = $5,
                    files_failed = $6,
                    total_chunks = $7,
                    chunks_embedded = $8,
                    current_file = $9,
                    error = $10,
                    updated_at = $11,
                    completed_at = $12
                WHERE id = $1
                "#,
                &[
                    &job.id,
                    &job_status_to_str(&job.status),
                    &job_stage_to_str(&job.stage),
                    &(job.files_processed as i32),
                    &(job.files_skipped as i32),
                    &(job.files_failed as i32),
                    &(job.total_chunks as i32),
                    &(job.chunks_embedded as i32),
                    &job.current_file,
                    &job.error,
                    &job.updated_at,
                    &job.completed_at,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to update job: {}", e)))?;

        Ok(())
    }

    /// Get a job by ID
    pub async fn get_job(&self, job_id: Uuid) -> Result<Option<JobRecord>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                r#"SELECT id, status, stage, total_files, files_processed, files_skipped,
                          files_failed, total_chunks, chunks_embedded, current_file, error,
                          created_at, updated_at, completed_at, options_json
                   FROM rag_jobs WHERE id = $1"#,
                &[&job_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get job: {}", e)))?;

        Ok(row.map(|r| row_to_job_record(&r)))
    }

    /// Get all incomplete jobs (for resuming on startup)
    pub async fn get_incomplete_jobs(&self) -> Result<Vec<JobRecord>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT id, status, stage, total_files, files_processed, files_skipped,
                          files_failed, total_chunks, chunks_embedded, current_file, error,
                          created_at, updated_at, completed_at, options_json
                   FROM rag_jobs WHERE status IN ('pending', 'processing') ORDER BY created_at ASC"#,
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get incomplete jobs: {}", e)))?;

        Ok(rows.iter().map(row_to_job_record).collect())
    }

    /// Get recent jobs
    pub async fn get_recent_jobs(&self, limit: usize) -> Result<Vec<JobRecord>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT id, status, stage, total_files, files_processed, files_skipped,
                          files_failed, total_chunks, chunks_embedded, current_file, error,
                          created_at, updated_at, completed_at, options_json
                   FROM rag_jobs ORDER BY created_at DESC LIMIT $1"#,
                &[&(limit as i64)],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get recent jobs: {}", e)))?;

        Ok(rows.iter().map(row_to_job_record).collect())
    }

    /// Delete old completed jobs (cleanup)
    pub async fn cleanup_old_jobs(&self, days_to_keep: i64) -> Result<usize> {
        let client = self.pool.get().await?;

        let cutoff = Utc::now() - chrono::Duration::days(days_to_keep);

        let count = client
            .execute(
                "DELETE FROM rag_jobs WHERE status IN ('complete', 'failed') AND created_at < $1",
                &[&cutoff],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to cleanup jobs: {}", e)))?;

        Ok(count as usize)
    }

    // ==================== Job Files Operations ====================

    /// Add a file to a job
    pub async fn add_job_file(&self, job_id: Uuid, file: &JobFileRecord) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .execute(
                r#"
                INSERT INTO rag_job_files (
                    job_id, filename, file_size, content_hash, status, tier,
                    parser_method, error, started_at, completed_at, duration_ms, file_data
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                ON CONFLICT(job_id, filename) DO UPDATE SET
                    status = EXCLUDED.status,
                    tier = EXCLUDED.tier,
                    parser_method = EXCLUDED.parser_method,
                    error = EXCLUDED.error,
                    started_at = COALESCE(EXCLUDED.started_at, rag_job_files.started_at),
                    completed_at = EXCLUDED.completed_at,
                    duration_ms = EXCLUDED.duration_ms
                "#,
                &[
                    &job_id,
                    &file.filename,
                    &(file.file_size as i64),
                    &file.content_hash,
                    &job_file_status_to_str(&file.status),
                    &file.tier,
                    &file.parser_method,
                    &file.error,
                    &file.started_at,
                    &file.completed_at,
                    &file.duration_ms.map(|d| d as i64),
                    &file.file_data.as_deref(),
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to add job file: {}", e)))?;

        Ok(())
    }

    /// Update job file status
    pub async fn update_job_file_status(
        &self,
        job_id: Uuid,
        filename: &str,
        status: JobFileStatus,
        error: Option<&str>,
        parser_method: Option<&str>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let client = self.pool.get().await?;

        let completed_at: Option<DateTime<Utc>> =
            if matches!(status, JobFileStatus::Complete | JobFileStatus::Failed | JobFileStatus::Skipped) {
                Some(Utc::now())
            } else {
                None
            };

        client
            .execute(
                r#"
                UPDATE rag_job_files SET
                    status = $3,
                    error = $4,
                    parser_method = COALESCE($5, parser_method),
                    completed_at = $6,
                    duration_ms = $7
                WHERE job_id = $1 AND filename = $2
                "#,
                &[
                    &job_id,
                    &filename,
                    &job_file_status_to_str(&status),
                    &error,
                    &parser_method,
                    &completed_at,
                    &duration_ms.map(|d| d as i64),
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to update job file: {}", e)))?;

        Ok(())
    }

    /// Get pending files for a job (for resuming)
    pub async fn get_pending_job_files(&self, job_id: Uuid) -> Result<Vec<JobFileRecord>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT filename, file_size, content_hash, status, tier,
                          parser_method, error, started_at, completed_at, duration_ms, file_data
                   FROM rag_job_files WHERE job_id = $1 AND status = 'pending' ORDER BY file_size ASC"#,
                &[&job_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get pending files: {}", e)))?;

        Ok(rows.iter().map(row_to_job_file_record).collect())
    }

    /// Get all files for a job
    pub async fn get_job_files(&self, job_id: Uuid) -> Result<Vec<JobFileRecord>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT filename, file_size, content_hash, status, tier,
                          parser_method, error, started_at, completed_at, duration_ms, file_data
                   FROM rag_job_files WHERE job_id = $1 ORDER BY filename"#,
                &[&job_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get job files: {}", e)))?;

        Ok(rows.iter().map(row_to_job_file_record).collect())
    }

    /// Clear file data blobs after processing (to save space)
    pub async fn clear_job_file_data(&self, job_id: Uuid) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .execute(
                "UPDATE rag_job_files SET file_data = NULL WHERE job_id = $1",
                &[&job_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to clear file data: {}", e)))?;

        Ok(())
    }

    // ==================== Chunk Queries (via rag_chunks) ====================

    /// Get a chunk by ID from rag_chunks
    pub async fn get_chunk_by_id(&self, id: &Uuid) -> Result<Option<crate::types::Chunk>> {
        let client = self.pool.get().await?;

        let row = client
            .query_opt(
                r#"SELECT id, document_id, chunk_index, content, filename, file_type,
                          page_number, section_title, char_start, char_end
                   FROM rag_chunks WHERE id = $1"#,
                &[id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get chunk: {}", e)))?;

        Ok(row.map(|r| row_to_chunk(&r)))
    }

    /// Get all chunks for a document from rag_chunks
    pub async fn get_chunks_for_document(
        &self,
        document_id: &Uuid,
    ) -> Result<Vec<crate::types::Chunk>> {
        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"SELECT id, document_id, chunk_index, content, filename, file_type,
                          page_number, section_title, char_start, char_end
                   FROM rag_chunks WHERE document_id = $1 ORDER BY chunk_index"#,
                &[document_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to get chunks for document: {}", e)))?;

        Ok(rows.iter().map(row_to_chunk).collect())
    }

    // ==================== Chunk Write + FTS Operations (via rag_chunks) ====================
    // These methods support LocalVectorStore / VertexVectorSearch when postgres is enabled
    // but pgvector is NOT the vector backend (e.g., Local HNSW or Vertex AI Vector Search).

    /// Insert a single chunk content record into rag_chunks
    pub async fn insert_chunk_content(&self, record: &ChunkContentRecord) -> Result<()> {
        let client = self.pool.get().await?;

        client
            .execute(
                r#"INSERT INTO rag_chunks (
                    id, document_id, chunk_index, content, filename, file_type,
                    page_number, section_title, char_start, char_end
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (id) DO UPDATE SET
                    content = EXCLUDED.content,
                    filename = EXCLUDED.filename
                "#,
                &[
                    &record.id,
                    &record.document_id,
                    &(record.chunk_index as i32),
                    &record.content,
                    &record.filename,
                    &file_type_to_ext(&record.file_type),
                    &record.page_number.map(|p| p as i32),
                    &record.section_title,
                    &(record.char_start as i32),
                    &(record.char_end as i32),
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to insert chunk: {}", e)))?;

        Ok(())
    }

    /// Batch insert chunk content records into rag_chunks
    pub async fn insert_chunks_content(&self, records: &[ChunkContentRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let client = self.pool.get().await?;

        for record in records {
            client
                .execute(
                    r#"INSERT INTO rag_chunks (
                        id, document_id, chunk_index, content, filename, file_type,
                        page_number, section_title, char_start, char_end
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    ON CONFLICT (id) DO NOTHING
                    "#,
                    &[
                        &record.id,
                        &record.document_id,
                        &(record.chunk_index as i32),
                        &record.content,
                        &record.filename,
                        &file_type_to_ext(&record.file_type),
                        &record.page_number.map(|p| p as i32),
                        &record.section_title,
                        &(record.char_start as i32),
                        &(record.char_end as i32),
                    ],
                )
                .await
                .map_err(|e| Error::Internal(format!("Failed to insert chunk batch: {}", e)))?;
        }

        Ok(())
    }

    /// Delete all chunks for a document from rag_chunks
    pub async fn delete_chunks_by_document(&self, document_id: &Uuid) -> Result<usize> {
        let client = self.pool.get().await?;

        let count = client
            .execute(
                "DELETE FROM rag_chunks WHERE document_id = $1",
                &[document_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to delete chunks: {}", e)))?;

        Ok(count as usize)
    }

    /// Full-text search on rag_chunks using PostgreSQL tsvector
    pub async fn string_search_chunks_filtered(
        &self,
        query: &str,
        limit: usize,
        organization_id: Option<&str>,
    ) -> Result<Vec<ChunkSearchResult>> {
        let client = self.pool.get().await?;

        let rows = if let Some(org_id) = organization_id {
            client
                .query(
                    r#"SELECT c.id, c.document_id, c.chunk_index, c.content, c.filename,
                              c.file_type, c.page_number, c.char_start, c.char_end,
                              ts_rank(c.content_tsv, query) as rank
                       FROM rag_chunks c, plainto_tsquery('english', $1) query
                       WHERE c.organization_id = $2 AND c.content_tsv @@ query AND c.archived_at IS NULL
                       ORDER BY rank DESC
                       LIMIT $3"#,
                    &[&query, &org_id, &(limit as i64)],
                )
                .await
        } else {
            client
                .query(
                    r#"SELECT c.id, c.document_id, c.chunk_index, c.content, c.filename,
                              c.file_type, c.page_number, c.char_start, c.char_end,
                              ts_rank(c.content_tsv, query) as rank
                       FROM rag_chunks c, plainto_tsquery('english', $1) query
                       WHERE c.content_tsv @@ query AND c.archived_at IS NULL
                       ORDER BY rank DESC
                       LIMIT $2"#,
                    &[&query, &(limit as i64)],
                )
                .await
        }
        .map_err(|e| Error::Internal(format!("Failed to execute FTS query: {}", e)))?;

        let results: Vec<ChunkSearchResult> = rows
            .iter()
            .map(|row| {
                let file_type_str: Option<String> = row.get("file_type");
                ChunkSearchResult {
                    chunk_id: row.get("id"),
                    document_id: row.get("document_id"),
                    chunk_index: row.get::<_, i32>("chunk_index") as u32,
                    content: row.get("content"),
                    filename: row.get("filename"),
                    file_type: FileType::from_extension(&file_type_str.unwrap_or_default()),
                    page_number: row.get::<_, Option<i32>>("page_number").map(|p| p as u32),
                    char_start: row.get::<_, i32>("char_start") as usize,
                    char_end: row.get::<_, i32>("char_end") as usize,
                    score: row.get::<_, f32>("rank") as f64,
                }
            })
            .collect();

        Ok(results)
    }

    /// Get all chunk IDs from rag_chunks (for migration dedup)
    pub async fn get_all_chunk_ids(&self) -> Result<Vec<Uuid>> {
        let client = self.pool.get().await?;

        let rows = client
            .query("SELECT id FROM rag_chunks", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to get chunk IDs: {}", e)))?;

        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Get total chunk count from rag_chunks
    pub async fn get_total_chunks_count(&self) -> Result<usize> {
        let client = self.pool.get().await?;

        let row = client
            .query_one("SELECT COUNT(*) FROM rag_chunks", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to count chunks: {}", e)))?;

        let count: i64 = row.get(0);
        Ok(count as usize)
    }

    /// Get chunk count for a specific document
    pub async fn get_chunks_count_for_document(&self, document_id: &Uuid) -> Result<usize> {
        let client = self.pool.get().await?;

        let row = client
            .query_one(
                "SELECT COUNT(*) FROM rag_chunks WHERE document_id = $1",
                &[document_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to count chunks for document: {}", e)))?;

        let count: i64 = row.get(0);
        Ok(count as usize)
    }
}

// ==================== Row Conversion Helpers ====================

fn row_to_file_record(row: &tokio_postgres::Row) -> FileRecord {
    let skip_reason_json: Option<serde_json::Value> = row.get("skip_reason");

    FileRecord {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        filename: row.get("filename"),
        content_hash: row.get("content_hash"),
        file_size: row.get::<_, i64>("file_size") as u64,
        file_type: FileType::from_extension(row.get::<_, &str>("file_type")),
        status: str_to_status(row.get("status")),
        document_id: row.get("document_id"),
        chunks_created: row.get::<_, Option<i32>>("chunks_created").map(|c| c as u32),
        skip_reason: skip_reason_json.and_then(|j| serde_json::from_value(j).ok()),
        error_message: row.get("error_message"),
        failed_at_stage: row.get("failed_at_stage"),
        job_id: row.get("job_id"),
        first_seen_at: row.get("first_seen_at"),
        last_processed_at: row.get("last_processed_at"),
        upload_count: row.get::<_, i32>("upload_count") as u32,
        original_url: row.get("original_url"),
        plaintext_url: row.get("plaintext_url"),
    }
}

fn row_to_document(row: &tokio_postgres::Row) -> Document {
    let metadata_json: Option<serde_json::Value> = row.get("metadata");

    Document {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        filename: row.get("filename"),
        internal_filename: row.get("internal_filename"),
        file_type: FileType::from_extension(row.get::<_, &str>("file_type")),
        content_hash: row.get("content_hash"),
        file_size: row.get::<_, i64>("file_size") as u64,
        total_chunks: row.get::<_, Option<i32>>("total_chunks").unwrap_or(0) as u32,
        total_pages: row.get::<_, Option<i32>>("total_pages").map(|p| p as u32),
        ingested_at: row.get("ingested_at"),
        metadata: metadata_json
            .and_then(|j| serde_json::from_value(j).ok())
            .unwrap_or_default(),
    }
}

fn row_to_job_record(row: &tokio_postgres::Row) -> JobRecord {
    let options_json: Option<serde_json::Value> = row.get("options_json");

    JobRecord {
        id: row.get("id"),
        status: str_to_job_status(row.get("status")),
        stage: str_to_job_stage(row.get("stage")),
        total_files: row.get::<_, i32>("total_files") as usize,
        files_processed: row.get::<_, i32>("files_processed") as usize,
        files_skipped: row.get::<_, i32>("files_skipped") as usize,
        files_failed: row.get::<_, i32>("files_failed") as usize,
        total_chunks: row.get::<_, i32>("total_chunks") as usize,
        chunks_embedded: row.get::<_, i32>("chunks_embedded") as usize,
        current_file: row.get("current_file"),
        error: row.get("error"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        completed_at: row.get("completed_at"),
        options: options_json.and_then(|j| serde_json::from_value(j).ok()),
    }
}

fn row_to_job_file_record(row: &tokio_postgres::Row) -> JobFileRecord {
    JobFileRecord {
        filename: row.get("filename"),
        file_size: row.get::<_, i64>("file_size") as u64,
        content_hash: row.get("content_hash"),
        status: str_to_job_file_status(row.get("status")),
        tier: row.get("tier"),
        parser_method: row.get("parser_method"),
        error: row.get("error"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        duration_ms: row.get::<_, Option<i64>>("duration_ms").map(|d| d as u64),
        file_data: row.get("file_data"),
    }
}

fn row_to_chunk(row: &tokio_postgres::Row) -> crate::types::Chunk {
    let file_type_str: Option<String> = row.get("file_type");

    crate::types::Chunk {
        id: row.get("id"),
        document_id: row.get("document_id"),
        chunk_index: row.get::<_, i32>("chunk_index") as u32,
        content: row.get("content"),
        embedding: Vec::new(),
        source: crate::types::ChunkSource {
            filename: row.get("filename"),
            internal_filename: None,
            file_type: crate::types::FileType::from_extension(
                &file_type_str.unwrap_or_default(),
            ),
            page_number: row
                .get::<_, Option<i32>>("page_number")
                .map(|p| p as u32),
            page_count: None,
            section_title: row.get("section_title"),
            heading_hierarchy: Vec::new(),
            sheet_name: None,
            row_range: None,
            line_start: None,
            line_end: None,
            code_context: None,
        },
        char_start: row.get::<_, i32>("char_start") as usize,
        char_end: row.get::<_, i32>("char_end") as usize,
        metadata: std::collections::HashMap::new(),
    }
}

// ==================== String Conversion Helpers ====================

fn file_type_to_ext(file_type: &FileType) -> &'static str {
    match file_type {
        FileType::Pdf => "pdf",
        FileType::Docx => "docx",
        FileType::Doc => "doc",
        FileType::Pptx => "pptx",
        FileType::Ppt => "ppt",
        FileType::Txt => "txt",
        FileType::Markdown => "md",
        FileType::Xlsx => "xlsx",
        FileType::Xls => "xls",
        FileType::Html => "html",
        FileType::Csv => "csv",
        FileType::Rtf => "rtf",
        FileType::Odt => "odt",
        FileType::Odp => "odp",
        FileType::Ods => "ods",
        FileType::Epub => "epub",
        FileType::Image => "image",
        FileType::Code(_) => "code",
        FileType::Unknown => "unknown",
    }
}

fn status_to_str(status: &FileRecordStatus) -> &'static str {
    match status {
        FileRecordStatus::Success => "success",
        FileRecordStatus::Skipped => "skipped",
        FileRecordStatus::Failed => "failed",
        FileRecordStatus::Processing => "processing",
    }
}

fn str_to_status(s: &str) -> FileRecordStatus {
    match s {
        "success" => FileRecordStatus::Success,
        "skipped" => FileRecordStatus::Skipped,
        "failed" => FileRecordStatus::Failed,
        "processing" => FileRecordStatus::Processing,
        _ => FileRecordStatus::Failed,
    }
}

fn job_status_to_str(status: &PersistedJobStatus) -> &'static str {
    match status {
        PersistedJobStatus::Pending => "pending",
        PersistedJobStatus::Processing => "processing",
        PersistedJobStatus::Complete => "complete",
        PersistedJobStatus::Failed => "failed",
    }
}

fn str_to_job_status(s: &str) -> PersistedJobStatus {
    match s {
        "pending" => PersistedJobStatus::Pending,
        "processing" => PersistedJobStatus::Processing,
        "complete" => PersistedJobStatus::Complete,
        _ => PersistedJobStatus::Failed,
    }
}

fn job_stage_to_str(stage: &PersistedJobStage) -> &'static str {
    match stage {
        PersistedJobStage::Queued => "queued",
        PersistedJobStage::Uploading => "uploading",
        PersistedJobStage::Parsing => "parsing",
        PersistedJobStage::Chunking => "chunking",
        PersistedJobStage::Embedding => "embedding",
        PersistedJobStage::Storing => "storing",
        PersistedJobStage::Complete => "complete",
        PersistedJobStage::Failed => "failed",
    }
}

fn str_to_job_stage(s: &str) -> PersistedJobStage {
    match s {
        "queued" => PersistedJobStage::Queued,
        "uploading" => PersistedJobStage::Uploading,
        "parsing" => PersistedJobStage::Parsing,
        "chunking" => PersistedJobStage::Chunking,
        "embedding" => PersistedJobStage::Embedding,
        "storing" => PersistedJobStage::Storing,
        "complete" => PersistedJobStage::Complete,
        _ => PersistedJobStage::Failed,
    }
}

fn job_file_status_to_str(status: &JobFileStatus) -> &'static str {
    match status {
        JobFileStatus::Pending => "pending",
        JobFileStatus::Processing => "processing",
        JobFileStatus::Complete => "complete",
        JobFileStatus::Skipped => "skipped",
        JobFileStatus::Failed => "failed",
    }
}

fn str_to_job_file_status(s: &str) -> JobFileStatus {
    match s {
        "pending" => JobFileStatus::Pending,
        "processing" => JobFileStatus::Processing,
        "complete" => JobFileStatus::Complete,
        "skipped" => JobFileStatus::Skipped,
        _ => JobFileStatus::Failed,
    }
}
