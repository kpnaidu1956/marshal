//! SQLite database for persistent file registry storage
//!
//! Provides durable storage for file processing status, replacing JSON file storage.
//! Uses r2d2 connection pool for concurrent read access (SQLite WAL mode).

use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::types::{FileRecord, FileRecordStatus, FileType};

/// SQLite-based file registry database.
/// Uses a connection pool to allow concurrent reads (SQLite WAL mode).
pub struct FileRegistryDb {
    pool: Pool<SqliteConnectionManager>,
}

impl FileRegistryDb {
    /// Create or open the database with a connection pool
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(8)
            .build(manager)
            .map_err(|e| Error::Internal(format!("Failed to create registry connection pool: {}", e)))?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Create an in-memory database (for testing)
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder()
            .max_size(1)
            .build(manager)
            .map_err(|e| Error::Internal(format!("Failed to create in-memory pool: {}", e)))?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Get a connection from the pool
    fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool.get()
            .map_err(|e| Error::Internal(format!("Failed to get registry DB connection: {}", e)))
    }

    /// Run database migrations
    fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;

        // Enable WAL mode for better concurrency (10-100x faster concurrent writes)
        conn.execute_batch(r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=10000;
            PRAGMA temp_store=MEMORY;
        "#).map_err(|e| Error::Internal(format!("Failed to set pragmas: {}", e)))?;

        conn.execute_batch(r#"
            -- File registry table
            CREATE TABLE IF NOT EXISTS file_registry (
                id TEXT PRIMARY KEY,
                filename TEXT NOT NULL UNIQUE,
                content_hash TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                file_type TEXT NOT NULL,
                status TEXT NOT NULL,
                document_id TEXT,
                chunks_created INTEGER,
                skip_reason TEXT,
                error_message TEXT,
                failed_at_stage TEXT,
                job_id TEXT,
                first_seen_at TEXT NOT NULL,
                last_processed_at TEXT NOT NULL,
                upload_count INTEGER NOT NULL DEFAULT 1,
                original_url TEXT,
                plaintext_url TEXT,
                gcs_synced INTEGER NOT NULL DEFAULT 0
            );

            -- Index for efficient lookups
            CREATE INDEX IF NOT EXISTS idx_file_registry_status ON file_registry(status);
            CREATE INDEX IF NOT EXISTS idx_file_registry_content_hash ON file_registry(content_hash);
            CREATE INDEX IF NOT EXISTS idx_file_registry_document_id ON file_registry(document_id);

            -- Documents table
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                filename TEXT NOT NULL,
                internal_filename TEXT,
                file_type TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                total_chunks INTEGER,
                total_pages INTEGER,
                ingested_at TEXT NOT NULL,
                metadata TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_documents_filename ON documents(filename);
            CREATE INDEX IF NOT EXISTS idx_documents_content_hash ON documents(content_hash);

            -- Sync status table
            CREATE TABLE IF NOT EXISTS sync_status (
                id INTEGER PRIMARY KEY,
                last_gcs_sync TEXT,
                files_synced INTEGER DEFAULT 0,
                sync_duration_ms INTEGER
            );

            -- Initialize sync status if not exists
            INSERT OR IGNORE INTO sync_status (id, last_gcs_sync, files_synced) VALUES (1, NULL, 0);

            -- Jobs table for job persistence and resumability
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
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
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                options_json TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
            CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);

            -- Job files table for tracking individual files in a job
            CREATE TABLE IF NOT EXISTS job_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id TEXT NOT NULL,
                filename TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                content_hash TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                tier TEXT,
                parser_method TEXT,
                error TEXT,
                started_at TEXT,
                completed_at TEXT,
                duration_ms INTEGER,
                file_data BLOB,
                FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE,
                UNIQUE(job_id, filename)
            );

            CREATE INDEX IF NOT EXISTS idx_job_files_job_id ON job_files(job_id);
            CREATE INDEX IF NOT EXISTS idx_job_files_status ON job_files(status);

            -- Chunks content table for text search (used by GCP backend)
            CREATE TABLE IF NOT EXISTS chunks_content (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                filename TEXT NOT NULL,
                file_type TEXT NOT NULL,
                page_number INTEGER,
                section_title TEXT,
                char_start INTEGER NOT NULL,
                char_end INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_chunks_content_document_id ON chunks_content(document_id);
            CREATE INDEX IF NOT EXISTS idx_chunks_content_filename ON chunks_content(filename);

            -- FTS5 virtual table for full-text search
            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                content,
                chunk_id UNINDEXED,
                document_id UNINDEXED,
                filename UNINDEXED,
                file_type UNINDEXED,
                page_number UNINDEXED,
                content='chunks_content',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS in sync with content table
            CREATE TRIGGER IF NOT EXISTS chunks_content_ai AFTER INSERT ON chunks_content BEGIN
                INSERT INTO chunks_fts(rowid, content, chunk_id, document_id, filename, file_type, page_number)
                VALUES (NEW.rowid, NEW.content, NEW.id, NEW.document_id, NEW.filename, NEW.file_type, NEW.page_number);
            END;

            CREATE TRIGGER IF NOT EXISTS chunks_content_ad AFTER DELETE ON chunks_content BEGIN
                INSERT INTO chunks_fts(chunks_fts, rowid, content, chunk_id, document_id, filename, file_type, page_number)
                VALUES ('delete', OLD.rowid, OLD.content, OLD.id, OLD.document_id, OLD.filename, OLD.file_type, OLD.page_number);
            END;

            CREATE TRIGGER IF NOT EXISTS chunks_content_au AFTER UPDATE ON chunks_content BEGIN
                INSERT INTO chunks_fts(chunks_fts, rowid, content, chunk_id, document_id, filename, file_type, page_number)
                VALUES ('delete', OLD.rowid, OLD.content, OLD.id, OLD.document_id, OLD.filename, OLD.file_type, OLD.page_number);
                INSERT INTO chunks_fts(rowid, content, chunk_id, document_id, filename, file_type, page_number)
                VALUES (NEW.rowid, NEW.content, NEW.id, NEW.document_id, NEW.filename, NEW.file_type, NEW.page_number);
            END;
        "#)
        .map_err(|e| Error::Internal(format!("Failed to run migrations: {}", e)))?;

        // Add organization_id column for multi-tenancy (idempotent migrations)
        self.add_column_if_not_exists(&conn, "documents", "organization_id", "TEXT")?;
        self.add_column_if_not_exists(&conn, "chunks_content", "organization_id", "TEXT")?;
        self.add_column_if_not_exists(&conn, "file_registry", "organization_id", "TEXT")?;

        // Create indexes for organization_id filtering
        conn.execute_batch(r#"
            CREATE INDEX IF NOT EXISTS idx_documents_organization_id ON documents(organization_id);
            CREATE INDEX IF NOT EXISTS idx_chunks_content_organization_id ON chunks_content(organization_id);
            CREATE INDEX IF NOT EXISTS idx_file_registry_organization_id ON file_registry(organization_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_org_filename ON documents(organization_id, filename);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_file_registry_org_filename ON file_registry(organization_id, filename);
        "#).map_err(|e| Error::Internal(format!("Failed to create organization_id indexes: {}", e)))?;

        // Migrate existing documents to default organization (one-time migration)
        // This is idempotent - only updates documents with NULL organization_id
        const DEFAULT_ORG_ID: &str = "demo-org";

        let updated_docs = conn.execute(
            "UPDATE documents SET organization_id = ?1 WHERE organization_id IS NULL",
            params![DEFAULT_ORG_ID],
        ).map_err(|e| Error::Internal(format!("Failed to migrate documents to default org: {}", e)))?;

        let updated_chunks = conn.execute(
            "UPDATE chunks_content SET organization_id = ?1 WHERE organization_id IS NULL",
            params![DEFAULT_ORG_ID],
        ).map_err(|e| Error::Internal(format!("Failed to migrate chunks to default org: {}", e)))?;

        // Also update any documents with old org ID (legacy_org) to new format
        let migrated_docs = conn.execute(
            "UPDATE documents SET organization_id = ?1 WHERE organization_id = 'legacy_org'",
            params![DEFAULT_ORG_ID],
        ).map_err(|e| Error::Internal(format!("Failed to migrate legacy_org documents: {}", e)))?;

        let migrated_chunks = conn.execute(
            "UPDATE chunks_content SET organization_id = ?1 WHERE organization_id = 'legacy_org'",
            params![DEFAULT_ORG_ID],
        ).map_err(|e| Error::Internal(format!("Failed to migrate legacy_org chunks: {}", e)))?;

        let total_docs = updated_docs + migrated_docs;
        let total_chunks = updated_chunks + migrated_chunks;

        if total_docs > 0 || total_chunks > 0 {
            tracing::info!(
                "Migrated {} documents and {} chunks to '{}' organization",
                total_docs, total_chunks, DEFAULT_ORG_ID
            );
        }

        // Rebuild FTS index if needed (handles case where data existed before FTS was added)
        // This is safe because we drop the connection lock before calling rebuild_fts_index
        drop(conn);
        match self.rebuild_fts_index() {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("FTS index verified/rebuilt with {} entries", count);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to rebuild FTS index (non-fatal): {}", e);
            }
        }

        tracing::info!("Database migrations complete");
        Ok(())
    }

    /// Add a column to a table if it doesn't exist (idempotent)
    fn add_column_if_not_exists(&self, conn: &Connection, table: &str, column: &str, col_type: &str) -> Result<()> {
        // Check if column exists by querying table_info
        let column_exists: bool = conn.query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1", table),
            params![column],
            |row| row.get::<_, i64>(0).map(|c| c > 0),
        ).unwrap_or(false);

        if !column_exists {
            conn.execute(
                &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, col_type),
                [],
            ).map_err(|e| Error::Internal(format!("Failed to add column {}.{}: {}", table, column, e)))?;
            tracing::info!("Added column {}.{}", table, column);
        }

        Ok(())
    }

    // ==================== File Registry Operations ====================

    /// Insert or update a file record
    pub fn upsert_file_record(&self, record: &FileRecord) -> Result<()> {
        let conn = self.conn()?;

        let skip_reason_json = record.skip_reason.as_ref()
            .map(|r| serde_json::to_string(r).unwrap_or_default());

        conn.execute(
            r#"
            INSERT INTO file_registry (
                id, filename, content_hash, file_size, file_type, status,
                document_id, chunks_created, skip_reason, error_message, failed_at_stage,
                job_id, first_seen_at, last_processed_at, upload_count, original_url, plaintext_url
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(filename) DO UPDATE SET
                content_hash = excluded.content_hash,
                file_size = excluded.file_size,
                file_type = excluded.file_type,
                status = excluded.status,
                document_id = excluded.document_id,
                chunks_created = excluded.chunks_created,
                skip_reason = excluded.skip_reason,
                error_message = excluded.error_message,
                failed_at_stage = excluded.failed_at_stage,
                job_id = excluded.job_id,
                last_processed_at = excluded.last_processed_at,
                upload_count = file_registry.upload_count + 1,
                original_url = COALESCE(excluded.original_url, file_registry.original_url),
                plaintext_url = COALESCE(excluded.plaintext_url, file_registry.plaintext_url)
            "#,
            params![
                record.id.to_string(),
                record.filename,
                record.content_hash,
                record.file_size as i64,
                file_type_to_extension(&record.file_type),
                status_to_string(&record.status),
                record.document_id.map(|id| id.to_string()),
                record.chunks_created.map(|c| c as i64),
                skip_reason_json,
                record.error_message,
                record.failed_at_stage,
                record.job_id.map(|id| id.to_string()),
                record.first_seen_at.to_rfc3339(),
                record.last_processed_at.to_rfc3339(),
                record.upload_count as i64,
                record.original_url,
                record.plaintext_url,
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert file record: {}", e)))?;

        Ok(())
    }

    /// Get a file record by filename
    pub fn get_file_record(&self, filename: &str) -> Result<Option<FileRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM file_registry WHERE filename = ?1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![filename], |row| {
            row_to_file_record(row)
        }).optional()
        .map_err(|e| Error::Internal(format!("Failed to get file record: {}", e)))?;

        Ok(record)
    }

    /// Get a file record by content hash
    pub fn get_file_record_by_hash(&self, content_hash: &str) -> Result<Option<FileRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM file_registry WHERE content_hash = ?1 LIMIT 1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![content_hash], |row| {
            row_to_file_record(row)
        }).optional()
        .map_err(|e| Error::Internal(format!("Failed to get file record: {}", e)))?;

        Ok(record)
    }

    /// List all file records
    pub fn list_file_records(&self) -> Result<Vec<FileRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare("SELECT * FROM file_registry ORDER BY last_processed_at DESC")
            .map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map([], row_to_file_record)
            .map_err(|e| Error::Internal(format!("Failed to list file records: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// List file records by status
    pub fn list_by_status(&self, status: FileRecordStatus) -> Result<Vec<FileRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM file_registry WHERE status = ?1 ORDER BY last_processed_at DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![status_to_string(&status)], row_to_file_record)
            .map_err(|e| Error::Internal(format!("Failed to list file records: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Delete a file record
    pub fn delete_file_record(&self, filename: &str) -> Result<bool> {
        let conn = self.conn()?;

        let count = conn.execute(
            "DELETE FROM file_registry WHERE filename = ?1",
            params![filename],
        ).map_err(|e| Error::Internal(format!("Failed to delete file record: {}", e)))?;

        Ok(count > 0)
    }

    /// Clear all failed file records
    pub fn clear_failed_files(&self) -> Result<usize> {
        let conn = self.conn()?;

        let count = conn.execute(
            "DELETE FROM file_registry WHERE status = 'failed'",
            [],
        ).map_err(|e| Error::Internal(format!("Failed to clear failed files: {}", e)))?;

        Ok(count)
    }

    /// Get file registry statistics
    pub fn get_stats(&self) -> Result<FileRegistryDbStats> {
        let conn = self.conn()?;

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_registry",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let success: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_registry WHERE status = 'success'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let failed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_registry WHERE status = 'failed'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let skipped: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_registry WHERE status = 'skipped'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        Ok(FileRegistryDbStats {
            total: total as usize,
            success: success as usize,
            failed: failed as usize,
            skipped: skipped as usize,
        })
    }

    // ==================== Job Persistence Operations ====================

    /// Create a new job record
    pub fn create_job(&self, job: &JobRecord) -> Result<()> {
        let conn = self.conn()?;

        let options_json = job.options.as_ref()
            .map(|o| serde_json::to_string(o).unwrap_or_default());

        conn.execute(
            r#"
            INSERT INTO jobs (
                id, status, stage, total_files, files_processed, files_skipped,
                files_failed, total_chunks, chunks_embedded, current_file, error,
                created_at, updated_at, completed_at, options_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                job.id.to_string(),
                job_status_to_string(&job.status),
                job_stage_to_string(&job.stage),
                job.total_files as i64,
                job.files_processed as i64,
                job.files_skipped as i64,
                job.files_failed as i64,
                job.total_chunks as i64,
                job.chunks_embedded as i64,
                job.current_file,
                job.error,
                job.created_at.to_rfc3339(),
                job.updated_at.to_rfc3339(),
                job.completed_at.map(|t| t.to_rfc3339()),
                options_json,
            ],
        ).map_err(|e| Error::Internal(format!("Failed to create job: {}", e)))?;

        Ok(())
    }

    /// Update job progress
    pub fn update_job(&self, job: &JobRecord) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            r#"
            UPDATE jobs SET
                status = ?2,
                stage = ?3,
                files_processed = ?4,
                files_skipped = ?5,
                files_failed = ?6,
                total_chunks = ?7,
                chunks_embedded = ?8,
                current_file = ?9,
                error = ?10,
                updated_at = ?11,
                completed_at = ?12
            WHERE id = ?1
            "#,
            params![
                job.id.to_string(),
                job_status_to_string(&job.status),
                job_stage_to_string(&job.stage),
                job.files_processed as i64,
                job.files_skipped as i64,
                job.files_failed as i64,
                job.total_chunks as i64,
                job.chunks_embedded as i64,
                job.current_file,
                job.error,
                job.updated_at.to_rfc3339(),
                job.completed_at.map(|t| t.to_rfc3339()),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to update job: {}", e)))?;

        Ok(())
    }

    /// Get a job by ID
    pub fn get_job(&self, job_id: Uuid) -> Result<Option<JobRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM jobs WHERE id = ?1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![job_id.to_string()], |row| {
            row_to_job_record(row)
        }).optional()
        .map_err(|e| Error::Internal(format!("Failed to get job: {}", e)))?;

        Ok(record)
    }

    /// Get all incomplete jobs (for resuming on startup)
    pub fn get_incomplete_jobs(&self) -> Result<Vec<JobRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM jobs WHERE status IN ('pending', 'processing') ORDER BY created_at ASC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map([], row_to_job_record)
            .map_err(|e| Error::Internal(format!("Failed to list incomplete jobs: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Get recent jobs
    pub fn get_recent_jobs(&self, limit: usize) -> Result<Vec<JobRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM jobs ORDER BY created_at DESC LIMIT ?1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![limit as i64], row_to_job_record)
            .map_err(|e| Error::Internal(format!("Failed to list jobs: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Delete old completed jobs (cleanup)
    pub fn cleanup_old_jobs(&self, days_to_keep: i64) -> Result<usize> {
        let conn = self.conn()?;

        let cutoff = (Utc::now() - chrono::Duration::days(days_to_keep)).to_rfc3339();

        let count = conn.execute(
            "DELETE FROM jobs WHERE status IN ('complete', 'failed') AND created_at < ?1",
            params![cutoff],
        ).map_err(|e| Error::Internal(format!("Failed to cleanup jobs: {}", e)))?;

        Ok(count)
    }

    // ==================== Job Files Operations ====================

    /// Add a file to a job
    pub fn add_job_file(&self, job_id: Uuid, file: &JobFileRecord) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            r#"
            INSERT INTO job_files (
                job_id, filename, file_size, content_hash, status, tier,
                parser_method, error, started_at, completed_at, duration_ms, file_data
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(job_id, filename) DO UPDATE SET
                status = excluded.status,
                tier = excluded.tier,
                parser_method = excluded.parser_method,
                error = excluded.error,
                started_at = COALESCE(excluded.started_at, job_files.started_at),
                completed_at = excluded.completed_at,
                duration_ms = excluded.duration_ms
            "#,
            params![
                job_id.to_string(),
                file.filename,
                file.file_size as i64,
                file.content_hash,
                job_file_status_to_string(&file.status),
                file.tier,
                file.parser_method,
                file.error,
                file.started_at.map(|t| t.to_rfc3339()),
                file.completed_at.map(|t| t.to_rfc3339()),
                file.duration_ms.map(|d| d as i64),
                file.file_data.as_deref(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to add job file: {}", e)))?;

        Ok(())
    }

    /// Update job file status
    pub fn update_job_file_status(
        &self,
        job_id: Uuid,
        filename: &str,
        status: JobFileStatus,
        error: Option<&str>,
        parser_method: Option<&str>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let conn = self.conn()?;

        let completed_at = if matches!(status, JobFileStatus::Complete | JobFileStatus::Failed | JobFileStatus::Skipped) {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };

        conn.execute(
            r#"
            UPDATE job_files SET
                status = ?3,
                error = ?4,
                parser_method = COALESCE(?5, parser_method),
                completed_at = ?6,
                duration_ms = ?7
            WHERE job_id = ?1 AND filename = ?2
            "#,
            params![
                job_id.to_string(),
                filename,
                job_file_status_to_string(&status),
                error,
                parser_method,
                completed_at,
                duration_ms.map(|d| d as i64),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to update job file: {}", e)))?;

        Ok(())
    }

    /// Get pending files for a job (for resuming)
    pub fn get_pending_job_files(&self, job_id: Uuid) -> Result<Vec<JobFileRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM job_files WHERE job_id = ?1 AND status = 'pending' ORDER BY file_size ASC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![job_id.to_string()], row_to_job_file_record)
            .map_err(|e| Error::Internal(format!("Failed to list pending files: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Get all files for a job
    pub fn get_job_files(&self, job_id: Uuid) -> Result<Vec<JobFileRecord>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM job_files WHERE job_id = ?1 ORDER BY filename"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![job_id.to_string()], row_to_job_file_record)
            .map_err(|e| Error::Internal(format!("Failed to list job files: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Clear file data blobs after processing (to save space)
    pub fn clear_job_file_data(&self, job_id: Uuid) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            "UPDATE job_files SET file_data = NULL WHERE job_id = ?1",
            params![job_id.to_string()],
        ).map_err(|e| Error::Internal(format!("Failed to clear file data: {}", e)))?;

        Ok(())
    }

    // ==================== GCS Sync Operations ====================

    /// Record a file discovered from GCS sync
    #[allow(clippy::too_many_arguments)]
    pub fn sync_from_gcs(
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
        let conn = self.conn()?;

        let status = if has_plaintext { "success" } else { "failed" };
        let error_message = if has_plaintext { None } else {
            Some("No plaintext found in GCS - processing may have failed".to_string())
        };

        let now = Utc::now().to_rfc3339();

        conn.execute(
            r#"
            INSERT INTO file_registry (
                id, filename, content_hash, file_size, file_type, status,
                document_id, chunks_created, error_message, first_seen_at,
                last_processed_at, upload_count, original_url, plaintext_url, gcs_synced
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?13, 1)
            ON CONFLICT(filename) DO UPDATE SET
                document_id = COALESCE(excluded.document_id, file_registry.document_id),
                original_url = COALESCE(excluded.original_url, file_registry.original_url),
                plaintext_url = COALESCE(excluded.plaintext_url, file_registry.plaintext_url),
                gcs_synced = 1
            "#,
            params![
                document_id.to_string(),
                filename,
                content_hash,
                file_size as i64,
                file_type,
                status,
                document_id.to_string(),
                if has_plaintext { Some(0i64) } else { None },  // chunks_created unknown from GCS
                error_message,
                &now,
                &now,
                original_url,
                plaintext_url,
            ],
        ).map_err(|e| Error::Internal(format!("Failed to sync from GCS: {}", e)))?;

        Ok(())
    }

    /// Update last GCS sync timestamp
    pub fn update_sync_status(&self, files_synced: usize, duration_ms: u64) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            "UPDATE sync_status SET last_gcs_sync = ?1, files_synced = ?2, sync_duration_ms = ?3 WHERE id = 1",
            params![Utc::now().to_rfc3339(), files_synced as i64, duration_ms as i64],
        ).map_err(|e| Error::Internal(format!("Failed to update sync status: {}", e)))?;

        Ok(())
    }

    /// Get last sync status
    pub fn get_sync_status(&self) -> Result<Option<SyncStatus>> {
        let conn = self.conn()?;

        let status = conn.query_row(
            "SELECT last_gcs_sync, files_synced, sync_duration_ms FROM sync_status WHERE id = 1",
            [],
            |row| {
                let last_sync: Option<String> = row.get(0)?;
                let files_synced: i64 = row.get(1)?;
                let duration_ms: Option<i64> = row.get(2)?;

                Ok(SyncStatus {
                    last_gcs_sync: last_sync.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                    files_synced: files_synced as usize,
                    sync_duration_ms: duration_ms.map(|d| d as u64),
                })
            },
        ).optional()
        .map_err(|e| Error::Internal(format!("Failed to get sync status: {}", e)))?;

        Ok(status)
    }

    // ==================== Chunk Content Operations (for FTS) ====================

    /// Insert a chunk into the content table (triggers will sync to FTS)
    pub fn insert_chunk_content(&self, chunk: &ChunkContentRecord) -> Result<()> {
        let conn = self.conn()?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO chunks_content (
                id, document_id, chunk_index, content, filename, file_type,
                page_number, section_title, char_start, char_end, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                chunk.id.to_string(),
                chunk.document_id.to_string(),
                chunk.chunk_index as i64,
                chunk.content,
                chunk.filename,
                file_type_to_extension(&chunk.file_type),
                chunk.page_number.map(|p| p as i64),
                chunk.section_title,
                chunk.char_start as i64,
                chunk.char_end as i64,
                Utc::now().to_rfc3339(),
            ],
        ).map_err(|e| Error::Internal(format!("Failed to insert chunk content: {}", e)))?;

        Ok(())
    }

    /// Insert multiple chunks (batch) with transaction for atomicity and performance
    pub fn insert_chunks_content(&self, chunks: &[ChunkContentRecord]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn()?;

        // Use a transaction for better performance (10-50x faster for batch inserts)
        let tx = conn.transaction()
            .map_err(|e| Error::Internal(format!("Failed to begin transaction: {}", e)))?;

        {
            let mut stmt = tx.prepare(
                r#"
                INSERT OR REPLACE INTO chunks_content (
                    id, document_id, chunk_index, content, filename, file_type,
                    page_number, section_title, char_start, char_end, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#
            ).map_err(|e| Error::Internal(format!("Failed to prepare statement: {}", e)))?;

            let now = Utc::now().to_rfc3339();
            for chunk in chunks {
                stmt.execute(params![
                    chunk.id.to_string(),
                    chunk.document_id.to_string(),
                    chunk.chunk_index as i64,
                    chunk.content,
                    chunk.filename,
                    file_type_to_extension(&chunk.file_type),
                    chunk.page_number.map(|p| p as i64),
                    chunk.section_title,
                    chunk.char_start as i64,
                    chunk.char_end as i64,
                    &now,
                ]).map_err(|e| Error::Internal(format!("Failed to insert chunk: {}", e)))?;
            }
        }

        tx.commit()
            .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Get all chunk IDs in the FTS table (for migration deduplication)
    pub fn get_all_chunk_ids(&self) -> Result<Vec<Uuid>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare("SELECT id FROM chunks_content")
            .map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let ids = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            Ok(Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()))
        })
        .map_err(|e| Error::Internal(format!("Failed to query chunk IDs: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(ids)
    }

    /// Full-text search across chunks
    pub fn string_search_chunks(&self, query: &str, limit: usize) -> Result<Vec<ChunkSearchResult>> {
        let conn = self.conn()?;

        // Use FTS5 match syntax for the query
        let fts_query = format!("\"{}\"", query.replace('"', "\"\""));

        let mut stmt = conn.prepare(
            r#"
            SELECT
                c.id, c.document_id, c.chunk_index, c.content, c.filename, c.file_type,
                c.page_number, c.section_title, c.char_start, c.char_end,
                bm25(chunks_fts) as score
            FROM chunks_fts f
            JOIN chunks_content c ON c.rowid = f.rowid
            WHERE chunks_fts MATCH ?1
            ORDER BY score
            LIMIT ?2
            "#
        ).map_err(|e| Error::Internal(format!("Failed to prepare FTS query: {}", e)))?;

        let results = stmt.query_map(params![fts_query, limit as i64], |row| {
            let id: String = row.get(0)?;
            let document_id: String = row.get(1)?;
            let chunk_index: i64 = row.get(2)?;
            let content: String = row.get(3)?;
            let filename: String = row.get(4)?;
            let file_type: String = row.get(5)?;
            let page_number: Option<i64> = row.get(6)?;
            let _section_title: Option<String> = row.get(7)?;
            let char_start: i64 = row.get(8)?;
            let char_end: i64 = row.get(9)?;
            let score: f64 = row.get(10)?;

            Ok(ChunkSearchResult {
                chunk_id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                document_id: Uuid::parse_str(&document_id).unwrap_or_else(|_| Uuid::new_v4()),
                chunk_index: chunk_index as u32,
                content,
                filename,
                file_type: extension_to_file_type(&file_type),
                page_number: page_number.map(|p| p as u32),
                char_start: char_start as usize,
                char_end: char_end as usize,
                score: -score, // BM25 returns negative scores, lower is better
            })
        }).map_err(|e| Error::Internal(format!("Failed to execute FTS query: {}", e)))?;

        let mut search_results = Vec::new();
        for result in results {
            match result {
                Ok(r) => search_results.push(r),
                Err(e) => tracing::warn!("Error reading search result: {}", e),
            }
        }

        Ok(search_results)
    }

    /// Search chunks content with FTS5 filtered by organization_id (for multi-tenancy)
    /// Note: Joins with documents table to get organization_id since chunks may not have it populated
    pub fn string_search_chunks_filtered(&self, query: &str, limit: usize, organization_id: Option<&str>) -> Result<Vec<ChunkSearchResult>> {
        let conn = self.conn()?;

        // Use FTS5 match syntax for the query
        let fts_query = format!("\"{}\"", query.replace('"', "\"\""));

        // Build query with optional organization filter (join with documents for organization_id)
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = if let Some(org_id) = organization_id {
            (
                r#"
                SELECT
                    c.id, c.document_id, c.chunk_index, c.content, c.filename, c.file_type,
                    c.page_number, c.section_title, c.char_start, c.char_end,
                    bm25(chunks_fts) as score
                FROM chunks_fts f
                JOIN chunks_content c ON c.rowid = f.rowid
                JOIN documents d ON d.id = c.document_id
                WHERE chunks_fts MATCH ?1 AND d.organization_id = ?2
                ORDER BY score
                LIMIT ?3
                "#.to_string(),
                vec![
                    Box::new(fts_query) as Box<dyn rusqlite::ToSql>,
                    Box::new(org_id.to_string()),
                    Box::new(limit as i64),
                ]
            )
        } else {
            (
                r#"
                SELECT
                    c.id, c.document_id, c.chunk_index, c.content, c.filename, c.file_type,
                    c.page_number, c.section_title, c.char_start, c.char_end,
                    bm25(chunks_fts) as score
                FROM chunks_fts f
                JOIN chunks_content c ON c.rowid = f.rowid
                WHERE chunks_fts MATCH ?1
                ORDER BY score
                LIMIT ?2
                "#.to_string(),
                vec![
                    Box::new(fts_query) as Box<dyn rusqlite::ToSql>,
                    Box::new(limit as i64),
                ]
            )
        };

        let mut stmt = conn.prepare(&sql)
            .map_err(|e| Error::Internal(format!("Failed to prepare FTS query: {}", e)))?;

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let results = stmt.query_map(params_refs.as_slice(), |row| {
            let id: String = row.get(0)?;
            let document_id: String = row.get(1)?;
            let chunk_index: i64 = row.get(2)?;
            let content: String = row.get(3)?;
            let filename: String = row.get(4)?;
            let file_type: String = row.get(5)?;
            let page_number: Option<i64> = row.get(6)?;
            let _section_title: Option<String> = row.get(7)?;
            let char_start: i64 = row.get(8)?;
            let char_end: i64 = row.get(9)?;
            let score: f64 = row.get(10)?;

            Ok(ChunkSearchResult {
                chunk_id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                document_id: Uuid::parse_str(&document_id).unwrap_or_else(|_| Uuid::new_v4()),
                chunk_index: chunk_index as u32,
                content,
                filename,
                file_type: extension_to_file_type(&file_type),
                page_number: page_number.map(|p| p as u32),
                char_start: char_start as usize,
                char_end: char_end as usize,
                score: -score, // BM25 returns negative scores, lower is better
            })
        }).map_err(|e| Error::Internal(format!("Failed to execute FTS query: {}", e)))?;

        let mut search_results = Vec::new();
        for result in results {
            match result {
                Ok(r) => search_results.push(r),
                Err(e) => tracing::warn!("Error reading search result: {}", e),
            }
        }

        Ok(search_results)
    }

    /// Delete all chunks for a document
    pub fn delete_chunks_by_document(&self, document_id: &Uuid) -> Result<usize> {
        let conn = self.conn()?;

        let deleted = conn.execute(
            "DELETE FROM chunks_content WHERE document_id = ?1",
            params![document_id.to_string()],
        ).map_err(|e| Error::Internal(format!("Failed to delete chunks: {}", e)))?;

        Ok(deleted)
    }

    /// Get chunk count for a document
    pub fn get_chunks_count_for_document(&self, document_id: &Uuid) -> Result<usize> {
        let conn = self.conn()?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks_content WHERE document_id = ?1",
            params![document_id.to_string()],
            |row| row.get(0),
        ).map_err(|e| Error::Internal(format!("Failed to count chunks: {}", e)))?;

        Ok(count as usize)
    }

    /// Get all chunks for a document (for re-vectorization)
    pub fn get_chunks_for_document(&self, document_id: &Uuid) -> Result<Vec<crate::types::Chunk>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            r#"SELECT id, document_id, chunk_index, content, filename, file_type,
                      page_number, section_title, char_start, char_end
               FROM chunks_content WHERE document_id = ?1
               ORDER BY chunk_index"#
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let chunks = stmt.query_map(params![document_id.to_string()], |row| {
            let id_str: String = row.get(0)?;
            let doc_id_str: String = row.get(1)?;
            let chunk_index: i64 = row.get(2)?;
            let content: String = row.get(3)?;
            let filename: String = row.get(4)?;
            let file_type_str: String = row.get(5)?;
            let page_number: Option<i64> = row.get(6)?;
            let section_title: Option<String> = row.get(7)?;
            let char_start: i64 = row.get(8)?;
            let char_end: i64 = row.get(9)?;

            Ok(crate::types::Chunk {
                id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                document_id: Uuid::parse_str(&doc_id_str).unwrap_or_else(|_| Uuid::new_v4()),
                content,
                embedding: Vec::new(),  // Embedding will be generated during re-vectorization
                source: crate::types::ChunkSource {
                    filename,
                    internal_filename: None,
                    file_type: FileType::from_extension(&file_type_str),
                    page_number: page_number.map(|p| p as u32),
                    page_count: None,
                    section_title,
                    heading_hierarchy: Vec::new(),
                    sheet_name: None,
                    row_range: None,
                    line_start: None,
                    line_end: None,
                    code_context: None,
                },
                char_start: char_start as usize,
                char_end: char_end as usize,
                chunk_index: chunk_index as u32,
                metadata: std::collections::HashMap::new(),
            })
        })
        .map_err(|e| Error::Internal(format!("Failed to query chunks: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(chunks)
    }

    /// Get all chunks from all documents (for bulk re-vectorization)
    pub fn get_all_chunks(&self, limit: Option<usize>, offset: Option<usize>) -> Result<Vec<crate::types::Chunk>> {
        let conn = self.conn()?;

        let query = match (limit, offset) {
            (Some(l), Some(o)) => format!(
                r#"SELECT id, document_id, chunk_index, content, filename, file_type,
                          page_number, section_title, char_start, char_end
                   FROM chunks_content ORDER BY document_id, chunk_index
                   LIMIT {} OFFSET {}"#, l, o
            ),
            (Some(l), None) => format!(
                r#"SELECT id, document_id, chunk_index, content, filename, file_type,
                          page_number, section_title, char_start, char_end
                   FROM chunks_content ORDER BY document_id, chunk_index
                   LIMIT {}"#, l
            ),
            _ => r#"SELECT id, document_id, chunk_index, content, filename, file_type,
                          page_number, section_title, char_start, char_end
                   FROM chunks_content ORDER BY document_id, chunk_index"#.to_string(),
        };

        let mut stmt = conn.prepare(&query)
            .map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let chunks = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let doc_id_str: String = row.get(1)?;
            let chunk_index: i64 = row.get(2)?;
            let content: String = row.get(3)?;
            let filename: String = row.get(4)?;
            let file_type_str: String = row.get(5)?;
            let page_number: Option<i64> = row.get(6)?;
            let section_title: Option<String> = row.get(7)?;
            let char_start: i64 = row.get(8)?;
            let char_end: i64 = row.get(9)?;

            Ok(crate::types::Chunk {
                id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                document_id: Uuid::parse_str(&doc_id_str).unwrap_or_else(|_| Uuid::new_v4()),
                content,
                embedding: Vec::new(),
                source: crate::types::ChunkSource {
                    filename,
                    internal_filename: None,
                    file_type: FileType::from_extension(&file_type_str),
                    page_number: page_number.map(|p| p as u32),
                    page_count: None,
                    section_title,
                    heading_hierarchy: Vec::new(),
                    sheet_name: None,
                    row_range: None,
                    line_start: None,
                    line_end: None,
                    code_context: None,
                },
                char_start: char_start as usize,
                char_end: char_end as usize,
                chunk_index: chunk_index as u32,
                metadata: std::collections::HashMap::new(),
            })
        })
        .map_err(|e| Error::Internal(format!("Failed to query chunks: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(chunks)
    }

    /// Get total chunk count
    pub fn get_total_chunks_count(&self) -> Result<usize> {
        let conn = self.conn()?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks_content",
            [],
            |row| row.get(0),
        ).map_err(|e| Error::Internal(format!("Failed to count chunks: {}", e)))?;

        Ok(count as usize)
    }

    /// Rebuild the FTS index from chunks_content table
    /// This should be called if the FTS table is empty but chunks_content has data
    /// (e.g., after migrating a database that predates the FTS feature)
    pub fn rebuild_fts_index(&self) -> Result<usize> {
        let conn = self.conn()?;

        // First check if FTS table needs rebuilding
        let fts_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks_fts",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let content_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks_content",
            [],
            |row| row.get(0),
        ).map_err(|e| Error::Internal(format!("Failed to count chunks_content: {}", e)))?;

        if fts_count >= content_count && content_count > 0 {
            tracing::info!("FTS index already populated ({} entries), skipping rebuild", fts_count);
            return Ok(fts_count as usize);
        }

        tracing::info!(
            "Rebuilding FTS index: chunks_fts has {} entries, chunks_content has {} entries",
            fts_count, content_count
        );

        // Clear and repopulate FTS table
        // For external content FTS5 tables, we use the 'delete-all' command
        conn.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('delete-all')", [])
            .map_err(|e| Error::Internal(format!("Failed to clear FTS index: {}", e)))?;

        // Repopulate from chunks_content
        let inserted = conn.execute(
            r#"
            INSERT INTO chunks_fts(rowid, content, chunk_id, document_id, filename, file_type, page_number)
            SELECT rowid, content, id, document_id, filename, file_type, page_number
            FROM chunks_content
            "#,
            [],
        ).map_err(|e| Error::Internal(format!("Failed to rebuild FTS index: {}", e)))?;

        tracing::info!("FTS index rebuilt with {} entries", inserted);

        Ok(inserted)
    }

    /// Get FTS index row count (for diagnostics)
    pub fn get_fts_count(&self) -> Result<usize> {
        let conn = self.conn()?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks_fts",
            [],
            |row| row.get(0),
        ).map_err(|e| Error::Internal(format!("Failed to count FTS entries: {}", e)))?;

        Ok(count as usize)
    }

    // ==================== Document Operations ====================

    /// Insert or update a document record
    pub fn upsert_document(&self, doc: &crate::types::Document) -> Result<()> {
        let conn = self.conn()?;

        let metadata_json = serde_json::to_string(&doc.metadata).unwrap_or_default();

        conn.execute(
            r#"
            INSERT INTO documents (
                id, filename, internal_filename, file_type, content_hash,
                file_size, total_chunks, total_pages, ingested_at, metadata, organization_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                filename = excluded.filename,
                internal_filename = excluded.internal_filename,
                file_type = excluded.file_type,
                content_hash = excluded.content_hash,
                file_size = excluded.file_size,
                total_chunks = excluded.total_chunks,
                total_pages = excluded.total_pages,
                metadata = excluded.metadata,
                organization_id = excluded.organization_id
            "#,
            params![
                doc.id.to_string(),
                doc.filename,
                doc.internal_filename,
                file_type_to_extension(&doc.file_type),
                doc.content_hash,
                doc.file_size as i64,
                doc.total_chunks as i64,
                doc.total_pages.map(|p| p as i64),
                doc.ingested_at.to_rfc3339(),
                metadata_json,
                doc.organization_id,
            ],
        ).map_err(|e| Error::Internal(format!("Failed to upsert document: {}", e)))?;

        Ok(())
    }

    /// Get a document by ID
    pub fn get_document(&self, id: &Uuid) -> Result<Option<crate::types::Document>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM documents WHERE id = ?1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![id.to_string()], |row| {
            row_to_document(row)
        }).optional()
        .map_err(|e| Error::Internal(format!("Failed to get document: {}", e)))?;

        Ok(record)
    }

    /// Get a document by filename
    pub fn get_document_by_filename(&self, filename: &str) -> Result<Option<crate::types::Document>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM documents WHERE filename = ?1"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![filename], |row| {
            row_to_document(row)
        }).optional()
        .map_err(|e| Error::Internal(format!("Failed to get document: {}", e)))?;

        Ok(record)
    }

    /// Get a document by organization_id + filename (new architecture)
    pub fn get_document_by_org_filename(&self, organization_id: &str, filename: &str) -> Result<Option<crate::types::Document>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM documents WHERE organization_id = ?1 AND filename = ?2"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![organization_id, filename], |row| {
            row_to_document(row)
        }).optional()
        .map_err(|e| Error::Internal(format!("Failed to get document: {}", e)))?;

        Ok(record)
    }

    /// List all documents for an organization
    pub fn list_documents_by_org(&self, organization_id: &str) -> Result<Vec<crate::types::Document>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT * FROM documents WHERE organization_id = ?1 ORDER BY ingested_at DESC"
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map(params![organization_id], row_to_document)
            .map_err(|e| Error::Internal(format!("Failed to list documents: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Delete a document by organization_id + filename
    pub fn delete_document_by_org_filename(&self, organization_id: &str, filename: &str) -> Result<bool> {
        let conn = self.conn()?;

        let count = conn.execute(
            "DELETE FROM documents WHERE organization_id = ?1 AND filename = ?2",
            params![organization_id, filename],
        ).map_err(|e| Error::Internal(format!("Failed to delete document: {}", e)))?;

        Ok(count > 0)
    }

    /// Check if a document exists by organization_id + filename
    pub fn document_exists_by_org_filename(&self, organization_id: &str, filename: &str) -> Result<bool> {
        let conn = self.conn()?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM documents WHERE organization_id = ?1 AND filename = ?2",
            params![organization_id, filename],
            |row| row.get(0),
        ).map_err(|e| Error::Internal(format!("Failed to check document: {}", e)))?;

        Ok(count > 0)
    }

    /// List all documents
    pub fn list_documents(&self) -> Result<Vec<crate::types::Document>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare("SELECT * FROM documents ORDER BY ingested_at DESC")
            .map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let records = stmt.query_map([], row_to_document)
            .map_err(|e| Error::Internal(format!("Failed to list documents: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Delete a document by ID
    pub fn delete_document(&self, id: &Uuid) -> Result<bool> {
        let conn = self.conn()?;

        let count = conn.execute(
            "DELETE FROM documents WHERE id = ?1",
            params![id.to_string()],
        ).map_err(|e| Error::Internal(format!("Failed to delete document: {}", e)))?;

        Ok(count > 0)
    }

    /// Get document count
    pub fn get_document_count(&self) -> Result<usize> {
        let conn = self.conn()?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM documents",
            [],
            |row| row.get(0),
        ).map_err(|e| Error::Internal(format!("Failed to count documents: {}", e)))?;

        Ok(count as usize)
    }

    // ==================== Chunk Content Operations ====================

    /// Get a chunk by ID from the chunks_content table
    /// This provides full chunk content for Vertex AI results (which have truncated metadata)
    pub fn get_chunk_by_id(&self, id: &Uuid) -> Result<Option<crate::types::Chunk>> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            r#"SELECT id, document_id, chunk_index, content, filename, file_type,
                      page_number, section_title, char_start, char_end
               FROM chunks_content WHERE id = ?1"#
        ).map_err(|e| Error::Internal(format!("Failed to prepare query: {}", e)))?;

        let record = stmt.query_row(params![id.to_string()], |row| {
            let id_str: String = row.get(0)?;
            let doc_id_str: String = row.get(1)?;
            let chunk_index: i64 = row.get(2)?;
            let content: String = row.get(3)?;
            let filename: String = row.get(4)?;
            let file_type_str: String = row.get(5)?;
            let page_number: Option<i64> = row.get(6)?;
            let section_title: Option<String> = row.get(7)?;
            let char_start: i64 = row.get(8)?;
            let char_end: i64 = row.get(9)?;

            let chunk_id = Uuid::parse_str(&id_str).unwrap_or_default();
            let document_id = Uuid::parse_str(&doc_id_str).unwrap_or_default();
            let file_type = extension_to_file_type(&file_type_str);

            let source = crate::types::ChunkSource {
                filename: filename.clone(),
                internal_filename: None,
                file_type: file_type.clone(),
                page_number: page_number.map(|p| p as u32),
                page_count: None,
                section_title,
                heading_hierarchy: Vec::new(),
                sheet_name: None,
                row_range: None,
                line_start: None,
                line_end: None,
                code_context: None,
            };

            Ok(crate::types::Chunk {
                id: chunk_id,
                document_id,
                content,
                embedding: Vec::new(), // Embedding not stored in SQLite, not needed for queries
                source,
                char_start: char_start as usize,
                char_end: char_end as usize,
                chunk_index: chunk_index as u32,
                metadata: std::collections::HashMap::new(),
            })
        }).optional()
        .map_err(|e| Error::Internal(format!("Failed to get chunk: {}", e)))?;

        Ok(record)
    }
}

/// Record for inserting chunk content
#[derive(Debug, Clone)]
pub struct ChunkContentRecord {
    pub id: Uuid,
    pub document_id: Uuid,
    pub chunk_index: u32,
    pub content: String,
    pub filename: String,
    pub file_type: FileType,
    pub page_number: Option<u32>,
    pub section_title: Option<String>,
    pub char_start: usize,
    pub char_end: usize,
}

/// Result from chunk string search
#[derive(Debug, Clone)]
pub struct ChunkSearchResult {
    pub chunk_id: Uuid,
    pub document_id: Uuid,
    pub chunk_index: u32,
    pub content: String,
    pub filename: String,
    pub file_type: FileType,
    pub page_number: Option<u32>,
    pub char_start: usize,
    pub char_end: usize,
    pub score: f64,
}

/// Database statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileRegistryDbStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// GCS sync status
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatus {
    pub last_gcs_sync: Option<DateTime<Utc>>,
    pub files_synced: usize,
    pub sync_duration_ms: Option<u64>,
}

// Helper functions

fn status_to_string(status: &FileRecordStatus) -> &'static str {
    match status {
        FileRecordStatus::Success => "success",
        FileRecordStatus::Skipped => "skipped",
        FileRecordStatus::Failed => "failed",
        FileRecordStatus::Processing => "processing",
    }
}

fn string_to_status(s: &str) -> FileRecordStatus {
    match s {
        "success" => FileRecordStatus::Success,
        "skipped" => FileRecordStatus::Skipped,
        "failed" => FileRecordStatus::Failed,
        "processing" => FileRecordStatus::Processing,
        _ => FileRecordStatus::Failed,
    }
}

fn file_type_to_extension(file_type: &FileType) -> &'static str {
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

fn extension_to_file_type(ext: &str) -> FileType {
    match ext.to_lowercase().as_str() {
        "pdf" => FileType::Pdf,
        "docx" => FileType::Docx,
        "doc" => FileType::Doc,
        "pptx" => FileType::Pptx,
        "ppt" => FileType::Ppt,
        "txt" => FileType::Txt,
        "md" | "markdown" => FileType::Markdown,
        "xlsx" => FileType::Xlsx,
        "xls" => FileType::Xls,
        "html" | "htm" => FileType::Html,
        "csv" => FileType::Csv,
        "rtf" => FileType::Rtf,
        "odt" => FileType::Odt,
        "odp" => FileType::Odp,
        "ods" => FileType::Ods,
        "epub" => FileType::Epub,
        "image" | "png" | "jpg" | "jpeg" | "gif" | "webp" => FileType::Image,
        "code" | "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" => FileType::Code(ext.to_string()),
        _ => FileType::Unknown,
    }
}

// ==================== Job Record Types ====================

/// Job status for persistence
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedJobStatus {
    Pending,
    Processing,
    Complete,
    Failed,
}

/// Processing stage for persistence
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedJobStage {
    Queued,
    Uploading,
    Parsing,
    Chunking,
    Embedding,
    Storing,
    Complete,
    Failed,
}

/// Job file status for persistence
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobFileStatus {
    Pending,
    Processing,
    Complete,
    Skipped,
    Failed,
}

/// Persisted job record
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobRecord {
    pub id: Uuid,
    pub status: PersistedJobStatus,
    pub stage: PersistedJobStage,
    pub total_files: usize,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub total_chunks: usize,
    pub chunks_embedded: usize,
    pub current_file: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub options: Option<JobOptions>,
}

/// Job processing options
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobOptions {
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
    pub parallel_embeddings: usize,
}

/// Job file record for persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobFileRecord {
    pub filename: String,
    pub file_size: u64,
    pub content_hash: Option<String>,
    pub status: JobFileStatus,
    pub tier: Option<String>,
    pub parser_method: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub file_data: Option<Vec<u8>>,
}

impl JobRecord {
    pub fn new(id: Uuid, total_files: usize, options: Option<JobOptions>) -> Self {
        let now = Utc::now();
        Self {
            id,
            status: PersistedJobStatus::Pending,
            stage: PersistedJobStage::Queued,
            total_files,
            files_processed: 0,
            files_skipped: 0,
            files_failed: 0,
            total_chunks: 0,
            chunks_embedded: 0,
            current_file: None,
            error: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
            options,
        }
    }
}

impl JobFileRecord {
    pub fn new(filename: String, file_size: u64, file_data: Option<Vec<u8>>) -> Self {
        Self {
            filename,
            file_size,
            content_hash: None,
            status: JobFileStatus::Pending,
            tier: None,
            parser_method: None,
            error: None,
            started_at: None,
            completed_at: None,
            duration_ms: None,
            file_data,
        }
    }
}

fn job_status_to_string(status: &PersistedJobStatus) -> &'static str {
    match status {
        PersistedJobStatus::Pending => "pending",
        PersistedJobStatus::Processing => "processing",
        PersistedJobStatus::Complete => "complete",
        PersistedJobStatus::Failed => "failed",
    }
}

fn string_to_job_status(s: &str) -> PersistedJobStatus {
    match s {
        "pending" => PersistedJobStatus::Pending,
        "processing" => PersistedJobStatus::Processing,
        "complete" => PersistedJobStatus::Complete,
        "failed" => PersistedJobStatus::Failed,
        _ => PersistedJobStatus::Failed,
    }
}

fn job_stage_to_string(stage: &PersistedJobStage) -> &'static str {
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

fn string_to_job_stage(s: &str) -> PersistedJobStage {
    match s {
        "queued" => PersistedJobStage::Queued,
        "uploading" => PersistedJobStage::Uploading,
        "parsing" => PersistedJobStage::Parsing,
        "chunking" => PersistedJobStage::Chunking,
        "embedding" => PersistedJobStage::Embedding,
        "storing" => PersistedJobStage::Storing,
        "complete" => PersistedJobStage::Complete,
        "failed" => PersistedJobStage::Failed,
        _ => PersistedJobStage::Failed,
    }
}

fn job_file_status_to_string(status: &JobFileStatus) -> &'static str {
    match status {
        JobFileStatus::Pending => "pending",
        JobFileStatus::Processing => "processing",
        JobFileStatus::Complete => "complete",
        JobFileStatus::Skipped => "skipped",
        JobFileStatus::Failed => "failed",
    }
}

fn string_to_job_file_status(s: &str) -> JobFileStatus {
    match s {
        "pending" => JobFileStatus::Pending,
        "processing" => JobFileStatus::Processing,
        "complete" => JobFileStatus::Complete,
        "skipped" => JobFileStatus::Skipped,
        "failed" => JobFileStatus::Failed,
        _ => JobFileStatus::Failed,
    }
}

fn row_to_job_record(row: &rusqlite::Row) -> rusqlite::Result<JobRecord> {
    let id_str: String = row.get(0)?;
    let status_str: String = row.get(1)?;
    let stage_str: String = row.get(2)?;
    let total_files: i64 = row.get(3)?;
    let files_processed: i64 = row.get(4)?;
    let files_skipped: i64 = row.get(5)?;
    let files_failed: i64 = row.get(6)?;
    let total_chunks: i64 = row.get(7)?;
    let chunks_embedded: i64 = row.get(8)?;
    let current_file: Option<String> = row.get(9)?;
    let error: Option<String> = row.get(10)?;
    let created_at_str: String = row.get(11)?;
    let updated_at_str: String = row.get(12)?;
    let completed_at_str: Option<String> = row.get(13)?;
    let options_json: Option<String> = row.get(14)?;

    Ok(JobRecord {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        status: string_to_job_status(&status_str),
        stage: string_to_job_stage(&stage_str),
        total_files: total_files as usize,
        files_processed: files_processed as usize,
        files_skipped: files_skipped as usize,
        files_failed: files_failed as usize,
        total_chunks: total_chunks as usize,
        chunks_embedded: chunks_embedded as usize,
        current_file,
        error,
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        completed_at: completed_at_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .ok()
        }),
        options: options_json.and_then(|j| serde_json::from_str(&j).ok()),
    })
}

fn row_to_job_file_record(row: &rusqlite::Row) -> rusqlite::Result<JobFileRecord> {
    let _id: i64 = row.get(0)?;
    let _job_id: String = row.get(1)?;
    let filename: String = row.get(2)?;
    let file_size: i64 = row.get(3)?;
    let content_hash: Option<String> = row.get(4)?;
    let status_str: String = row.get(5)?;
    let tier: Option<String> = row.get(6)?;
    let parser_method: Option<String> = row.get(7)?;
    let error: Option<String> = row.get(8)?;
    let started_at_str: Option<String> = row.get(9)?;
    let completed_at_str: Option<String> = row.get(10)?;
    let duration_ms: Option<i64> = row.get(11)?;
    let file_data: Option<Vec<u8>> = row.get(12)?;

    Ok(JobFileRecord {
        filename,
        file_size: file_size as u64,
        content_hash,
        status: string_to_job_file_status(&status_str),
        tier,
        parser_method,
        error,
        started_at: started_at_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .ok()
        }),
        completed_at: completed_at_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .ok()
        }),
        duration_ms: duration_ms.map(|d| d as u64),
        file_data,
    })
}

fn row_to_file_record(row: &rusqlite::Row) -> rusqlite::Result<FileRecord> {
    let id_str: String = row.get(0)?;
    let filename: String = row.get(1)?;
    let content_hash: String = row.get(2)?;
    let file_size: i64 = row.get(3)?;
    let file_type_str: String = row.get(4)?;
    let status_str: String = row.get(5)?;
    let document_id_str: Option<String> = row.get(6)?;
    let chunks_created: Option<i64> = row.get(7)?;
    let skip_reason_json: Option<String> = row.get(8)?;
    let error_message: Option<String> = row.get(9)?;
    let failed_at_stage: Option<String> = row.get(10)?;
    let job_id_str: Option<String> = row.get(11)?;
    let first_seen_at_str: String = row.get(12)?;
    let last_processed_at_str: String = row.get(13)?;
    let upload_count: i64 = row.get(14)?;
    let original_url: Option<String> = row.get(15)?;
    let plaintext_url: Option<String> = row.get(16)?;
    // gcs_synced is at index 17
    // organization_id is at index 18 (added via migration)
    let organization_id: Option<String> = row.get(18).ok();

    Ok(FileRecord {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id: organization_id.unwrap_or_else(|| "unknown".to_string()),
        filename,
        content_hash,
        file_size: file_size as u64,
        file_type: FileType::from_extension(&file_type_str),
        status: string_to_status(&status_str),
        document_id: document_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
        chunks_created: chunks_created.map(|c| c as u32),
        skip_reason: skip_reason_json.and_then(|j| serde_json::from_str(&j).ok()),
        error_message,
        failed_at_stage,
        job_id: job_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
        first_seen_at: DateTime::parse_from_rfc3339(&first_seen_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        last_processed_at: DateTime::parse_from_rfc3339(&last_processed_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        upload_count: upload_count as u32,
        original_url,
        plaintext_url,
    })
}

fn row_to_document(row: &rusqlite::Row) -> rusqlite::Result<crate::types::Document> {
    let id_str: String = row.get(0)?;
    let filename: String = row.get(1)?;
    let internal_filename: Option<String> = row.get(2)?;
    let file_type_str: String = row.get(3)?;
    let content_hash: String = row.get(4)?;
    let file_size: i64 = row.get(5)?;
    let total_chunks: Option<i64> = row.get(6)?;
    let total_pages: Option<i64> = row.get(7)?;
    let ingested_at_str: String = row.get(8)?;
    let metadata_json: Option<String> = row.get(9)?;
    // organization_id is at index 10 (added by migration)
    let organization_id: Option<String> = row.get(10).ok();

    Ok(crate::types::Document {
        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
        organization_id,
        filename,
        internal_filename,
        file_type: extension_to_file_type(&file_type_str),
        content_hash,
        total_pages: total_pages.map(|p| p as u32),
        total_chunks: total_chunks.unwrap_or(0) as u32,
        file_size: file_size as u64,
        ingested_at: DateTime::parse_from_rfc3339(&ingested_at_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        metadata: metadata_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FileRecordParams;

    #[test]
    fn test_upsert_and_get() {
        let db = FileRegistryDb::in_memory().unwrap();

        let record = FileRecord::success(
            FileRecordParams {
                organization_id: "test-org".to_string(),
                filename: "test.pdf".to_string(),
                content_hash: "abc123".to_string(),
                file_size: 1000,
                file_type: FileType::Pdf,
                job_id: None,
            },
            Uuid::new_v4(),
            10,
        );

        db.upsert_file_record(&record).unwrap();

        let retrieved = db.get_file_record("test.pdf").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().filename, "test.pdf");
    }

    #[test]
    fn test_stats() {
        let db = FileRegistryDb::in_memory().unwrap();

        // Add some records
        db.upsert_file_record(&FileRecord::success(
            FileRecordParams {
                organization_id: "test-org".to_string(),
                filename: "success.pdf".to_string(),
                content_hash: "hash1".to_string(),
                file_size: 100,
                file_type: FileType::Pdf,
                job_id: None,
            },
            Uuid::new_v4(),
            5,
        )).unwrap();

        db.upsert_file_record(&FileRecord::failed(
            FileRecordParams {
                organization_id: "test-org".to_string(),
                filename: "failed.pdf".to_string(),
                content_hash: "hash2".to_string(),
                file_size: 100,
                file_type: FileType::Pdf,
                job_id: None,
            },
            "error".to_string(),
            "parsing".to_string(),
        )).unwrap();

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.failed, 1);
    }
}
