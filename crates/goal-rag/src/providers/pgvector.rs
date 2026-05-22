//! PostgreSQL pgvector-based vector store
//!
//! Stores vectors in PostgreSQL using the pgvector extension.
//! Provides instant startup (no index rebuild) and persistent storage.
//! Full-text search uses PostgreSQL tsvector + GIN index on rag_chunks.

use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use crate::types::Chunk;
use crate::types::response::StringSearchResult;

use super::vector_store::{SearchFilter, VectorSearchResult, VectorStoreProvider};

/// pgvector-based vector store using PostgreSQL
///
/// Benefits over HNSW:
/// - Instant startup (index persisted in database)
/// - No memory constraints (scales with disk)
/// - Multi-tenancy via organization_id filtering
/// - ACID transactions for data integrity
/// - Full-text search via PostgreSQL tsvector (no SQLite dependency)
pub struct PgVectorStore {
    pool: Arc<PgPool>,
    /// Vector dimensions (must match embedding model)
    dimensions: usize,
}

impl PgVectorStore {
    /// Create a new pgvector store
    ///
    /// Will create the required table and index if they don't exist.
    pub async fn new(
        pool: Arc<PgPool>,
        dimensions: usize,
    ) -> Result<Self> {
        let store = Self {
            pool,
            dimensions,
        };

        // Initialize schema
        store.init_schema().await?;

        Ok(store)
    }

    /// Initialize the pgvector schema (table + index + tsvector FTS)
    async fn init_schema(&self) -> Result<()> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        // Enable pgvector extension
        client
            .execute("CREATE EXTENSION IF NOT EXISTS vector", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to create vector extension: {}", e)))?;

        // Create chunks table with vector column
        let create_table = format!(
            r#"
            CREATE TABLE IF NOT EXISTS rag_chunks (
                id UUID PRIMARY KEY,
                document_id UUID NOT NULL,
                organization_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                filename TEXT NOT NULL,
                file_type TEXT,
                page_number INTEGER,
                section_title TEXT,
                char_start INTEGER,
                char_end INTEGER,
                embedding vector({dimensions}),
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
            dimensions = self.dimensions
        );
        client
            .execute(&create_table, &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to create rag_chunks table: {}", e)))?;

        // Create indexes for common queries
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_rag_chunks_document_id ON rag_chunks(document_id)",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to create document_id index: {}", e)))?;

        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_rag_chunks_org_id ON rag_chunks(organization_id)",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to create org_id index: {}", e)))?;

        // Create HNSW index for vector similarity search
        // Using cosine distance (vector_cosine_ops) which is most common for text embeddings
        // HNSW provides better query performance than IVFFlat for most workloads
        let create_index = format!(
            r#"
            CREATE INDEX IF NOT EXISTS idx_rag_chunks_embedding_hnsw
            ON rag_chunks
            USING hnsw (embedding vector_cosine_ops)
            WITH (m = 16, ef_construction = 200)
            "#
        );
        client
            .execute(&create_index, &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to create HNSW index: {}", e)))?;

        // === tsvector FTS setup ===
        // Add tsvector column for full-text search (replaces SQLite FTS5)
        client
            .execute(
                "ALTER TABLE rag_chunks ADD COLUMN IF NOT EXISTS content_tsv TSVECTOR",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to add content_tsv column: {}", e)))?;

        // GIN index for fast FTS queries
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_rag_chunks_fts ON rag_chunks USING GIN (content_tsv)",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to create FTS GIN index: {}", e)))?;

        // Trigger to auto-populate tsvector on INSERT/UPDATE
        client
            .batch_execute(
                r#"
                CREATE OR REPLACE FUNCTION rag_chunks_tsv_trigger() RETURNS trigger AS $$
                BEGIN
                    NEW.content_tsv := to_tsvector('english', COALESCE(NEW.content, ''));
                    RETURN NEW;
                END;
                $$ LANGUAGE plpgsql;

                DROP TRIGGER IF EXISTS trg_rag_chunks_tsv ON rag_chunks;
                CREATE TRIGGER trg_rag_chunks_tsv
                    BEFORE INSERT OR UPDATE OF content ON rag_chunks
                    FOR EACH ROW EXECUTE FUNCTION rag_chunks_tsv_trigger();
                "#,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to create FTS trigger: {}", e)))?;

        // === archived_at column for soft-archive ===
        client
            .execute(
                "ALTER TABLE rag_chunks ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to add archived_at column: {}", e)))?;

        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_rag_chunks_archived ON rag_chunks(archived_at) WHERE archived_at IS NOT NULL",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to create archived_at index: {}", e)))?;

        // Backfill existing rows that have NULL tsvector
        let backfilled = client
            .execute(
                "UPDATE rag_chunks SET content_tsv = to_tsvector('english', content) WHERE content_tsv IS NULL",
                &[],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to backfill tsvector: {}", e)))?;

        if backfilled > 0 {
            tracing::info!("Backfilled tsvector for {} existing chunks", backfilled);
        }

        tracing::info!(
            "pgvector schema initialized (dimensions: {}, index: HNSW, FTS: tsvector+GIN)",
            self.dimensions
        );

        Ok(())
    }

    /// Convert Chunk to database row and insert
    async fn insert_chunk_internal(&self, chunk: &Chunk, organization_id: &str) -> Result<()> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        // Get embedding from chunk
        if chunk.embedding.is_empty() {
            return Err(Error::Internal("Chunk has no embedding".to_string()));
        }

        if chunk.embedding.len() != self.dimensions {
            return Err(Error::Internal(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                chunk.embedding.len()
            )));
        }

        // Convert to pgvector format
        let embedding_vec = pgvector::Vector::from(chunk.embedding.clone());
        let file_type_str = format!("{:?}", chunk.source.file_type).to_lowercase();

        client
            .execute(
                r#"
                INSERT INTO rag_chunks (
                    id, document_id, organization_id, chunk_index, content,
                    filename, file_type, page_number, section_title,
                    char_start, char_end, embedding
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                ON CONFLICT (id) DO UPDATE SET
                    content = EXCLUDED.content,
                    embedding = EXCLUDED.embedding
                "#,
                &[
                    &chunk.id,
                    &chunk.document_id,
                    &organization_id,
                    &(chunk.chunk_index as i32),
                    &chunk.content,
                    &chunk.source.filename,
                    &file_type_str,
                    &chunk.source.page_number.map(|p| p as i32),
                    &chunk.source.section_title,
                    &(chunk.char_start as i32),
                    &(chunk.char_end as i32),
                    &embedding_vec,
                ],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to insert chunk: {}", e)))?;

        Ok(())
    }

    /// Search for similar vectors using cosine similarity
    /// Organization ID is ALWAYS required — no cross-org embedding leakage
    async fn search_internal(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        filter: &SearchFilter,
    ) -> Result<Vec<VectorSearchResult>> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let query_vec = pgvector::Vector::from(query_embedding.to_vec());
        let limit = top_k as i64;

        let org_id = &filter.organization_id;
        let doc_ids = &filter.document_ids;

        // Increase HNSW search beam width for better recall (default is 40).
        // Uses a transaction block with SET LOCAL so the setting doesn't leak to other pool users.
        client
            .execute("BEGIN", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to begin transaction: {}", e)))?;
        client
            .execute("SET LOCAL hnsw.ef_search = 100", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to set ef_search: {}", e)))?;

        // Always filter by organization_id — no cross-org leakage
        let rows = match doc_ids {
            Some(docs) => {
                // Filter by org + documents
                client.query(
                    r#"
                    SELECT id, document_id, chunk_index, content, filename, file_type,
                           page_number, section_title, char_start, char_end,
                           1 - (embedding <=> $1) as similarity
                    FROM rag_chunks
                    WHERE organization_id = $2 AND document_id = ANY($3) AND archived_at IS NULL
                    ORDER BY embedding <=> $1
                    LIMIT $4
                    "#,
                    &[&query_vec, org_id, docs, &limit],
                ).await
            }
            None => {
                // Filter by org only
                client.query(
                    r#"
                    SELECT id, document_id, chunk_index, content, filename, file_type,
                           page_number, section_title, char_start, char_end,
                           1 - (embedding <=> $1) as similarity
                    FROM rag_chunks
                    WHERE organization_id = $2 AND archived_at IS NULL
                    ORDER BY embedding <=> $1
                    LIMIT $3
                    "#,
                    &[&query_vec, org_id, &limit],
                ).await
            }
        }.map_err(|e| Error::Internal(format!("Failed to search vectors: {}", e)))?;

        // End the transaction so SET LOCAL doesn't leak across the pool
        let _ = client.execute("COMMIT", &[]).await;

        let results: Vec<VectorSearchResult> = rows
            .iter()
            .map(|row| {
                let file_type_str: Option<String> = row.get("file_type");
                let chunk = Chunk {
                    id: row.get("id"),
                    document_id: row.get("document_id"),
                    chunk_index: row.get::<_, i32>("chunk_index") as u32,
                    content: row.get("content"),
                    char_start: row.get::<_, i32>("char_start") as usize,
                    char_end: row.get::<_, i32>("char_end") as usize,
                    embedding: Vec::new(), // Don't return embedding in search results
                    source: crate::types::ChunkSource {
                        filename: row.get("filename"),
                        file_type: crate::types::FileType::from_extension(
                            &file_type_str.unwrap_or_default()
                        ),
                        page_number: row.get::<_, Option<i32>>("page_number").map(|p| p as u32),
                        section_title: row.get("section_title"),
                        internal_filename: None,
                        page_count: None,
                        heading_hierarchy: Vec::new(),
                        sheet_name: None,
                        row_range: None,
                        line_start: None,
                        line_end: None,
                        code_context: None,
                    },
                    metadata: std::collections::HashMap::new(),
                };
                let similarity: f64 = row.get("similarity");
                VectorSearchResult {
                    chunk,
                    similarity: similarity as f32,
                }
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl VectorStoreProvider for PgVectorStore {
    async fn insert_chunk(&self, chunk: &Chunk) -> Result<()> {
        // Extract organization_id from metadata (required)
        let org_id = chunk.metadata.get("organization_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Validation("Chunk metadata missing required 'organization_id'".to_string()))?;

        // Insert into pgvector (tsvector trigger auto-populates FTS column)
        self.insert_chunk_internal(chunk, org_id).await?;

        Ok(())
    }

    async fn insert_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        const BATCH_SIZE: usize = 50;
        const COLS_PER_ROW: usize = 12;

        // Validate all chunks upfront before acquiring a connection
        for chunk in chunks {
            if chunk.embedding.is_empty() {
                return Err(Error::Internal("Chunk has no embedding".to_string()));
            }
            if chunk.embedding.len() != self.dimensions {
                return Err(Error::Internal(format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    self.dimensions,
                    chunk.embedding.len()
                )));
            }
            if chunk.metadata.get("organization_id").and_then(|v| v.as_str()).is_none() {
                return Err(Error::Validation(
                    "Chunk metadata missing required 'organization_id'".to_string(),
                ));
            }
        }

        // Use a single connection for all batches
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        for batch in chunks.chunks(BATCH_SIZE) {
            // Build multi-row VALUES clause: ($1,$2,...,$12), ($13,$14,...,$24), ...
            let mut values_clauses = Vec::with_capacity(batch.len());
            let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> =
                Vec::with_capacity(batch.len() * COLS_PER_ROW);

            for (i, chunk) in batch.iter().enumerate() {
                let offset = i * COLS_PER_ROW;
                values_clauses.push(format!(
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    offset + 1,
                    offset + 2,
                    offset + 3,
                    offset + 4,
                    offset + 5,
                    offset + 6,
                    offset + 7,
                    offset + 8,
                    offset + 9,
                    offset + 10,
                    offset + 11,
                    offset + 12,
                ));

                let org_id = chunk
                    .metadata
                    .get("organization_id")
                    .and_then(|v| v.as_str())
                    .unwrap() // safe: validated above
                    .to_string();
                let embedding_vec = pgvector::Vector::from(chunk.embedding.clone());
                let file_type_str = format!("{:?}", chunk.source.file_type).to_lowercase();

                params.push(Box::new(chunk.id));
                params.push(Box::new(chunk.document_id));
                params.push(Box::new(org_id));
                params.push(Box::new(chunk.chunk_index as i32));
                params.push(Box::new(chunk.content.clone()));
                params.push(Box::new(chunk.source.filename.clone()));
                params.push(Box::new(file_type_str));
                params.push(Box::new(chunk.source.page_number.map(|p| p as i32)));
                params.push(Box::new(chunk.source.section_title.clone()));
                params.push(Box::new(chunk.char_start as i32));
                params.push(Box::new(chunk.char_end as i32));
                params.push(Box::new(embedding_vec));
            }

            let sql = format!(
                r#"
                INSERT INTO rag_chunks (
                    id, document_id, organization_id, chunk_index, content,
                    filename, file_type, page_number, section_title,
                    char_start, char_end, embedding
                ) VALUES {}
                ON CONFLICT (id) DO UPDATE SET
                    content = EXCLUDED.content,
                    embedding = EXCLUDED.embedding
                "#,
                values_clauses.join(", ")
            );

            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                params.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

            client
                .execute(&sql, &param_refs)
                .await
                .map_err(|e| Error::Internal(format!("Failed to batch insert chunks: {}", e)))?;
        }

        tracing::debug!("Batch inserted {} chunks in {} batches", chunks.len(), (chunks.len() + BATCH_SIZE - 1) / BATCH_SIZE);

        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        filter: &SearchFilter,
    ) -> Result<Vec<VectorSearchResult>> {
        self.search_internal(query_embedding, top_k, filter).await
    }

    async fn string_search(
        &self,
        query: &str,
        limit: usize,
        organization_id: &str,
    ) -> Result<Vec<StringSearchResult>> {
        // Use PostgreSQL tsvector FTS on rag_chunks directly (replaces SQLite FTS5)
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let rows = client
            .query(
                r#"
                SELECT id, document_id, chunk_index, content, filename, file_type,
                       page_number, section_title, char_start, char_end,
                       ts_rank(content_tsv, query) as rank
                FROM rag_chunks, plainto_tsquery('english', $1) query
                WHERE organization_id = $2 AND content_tsv @@ query AND archived_at IS NULL
                ORDER BY rank DESC
                LIMIT $3
                "#,
                &[&query, &organization_id, &(limit as i64)],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to execute FTS query: {}", e)))?;

        let query_lower = query.to_lowercase();
        let results: Vec<StringSearchResult> = rows
            .iter()
            .map(|row| {
                let content: String = row.get("content");
                let content_lower = content.to_lowercase();
                let match_positions: Vec<usize> = content_lower
                    .match_indices(&query_lower)
                    .map(|(pos, _)| pos)
                    .collect();

                StringSearchResult {
                    chunk_id: row.get("id"),
                    document_id: row.get("document_id"),
                    filename: row.get("filename"),
                    file_type: {
                        let ft: Option<String> = row.get("file_type");
                        crate::types::FileType::from_extension(&ft.unwrap_or_default())
                    },
                    page_number: row.get::<_, Option<i32>>("page_number").map(|p| p as u32),
                    match_count: match_positions.len(),
                    match_positions,
                    preview: content.chars().take(200).collect(),
                    highlighted_snippet: content,
                }
            })
            .collect();

        Ok(results)
    }

    async fn delete_by_document(&self, document_id: &Uuid) -> Result<usize> {
        // Delete from pgvector (rag_chunks only — no more SQLite)
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let result = client
            .execute(
                "DELETE FROM rag_chunks WHERE document_id = $1",
                &[document_id],
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to delete chunks: {}", e)))?;

        Ok(result as usize)
    }

    async fn len(&self) -> Result<usize> {
        let client = self.pool.get().await.map_err(|e| {
            Error::Internal(format!("Failed to get PG connection: {}", e))
        })?;

        let row = client
            .query_one("SELECT COUNT(*) as count FROM rag_chunks", &[])
            .await
            .map_err(|e| Error::Internal(format!("Failed to count chunks: {}", e)))?;

        let count: i64 = row.get("count");
        Ok(count as usize)
    }

    async fn health_check(&self) -> Result<bool> {
        let client = self.pool.get().await.map_err(|_| {
            Error::Internal("Failed to get PG connection".to_string())
        })?;

        client
            .query_one("SELECT 1", &[])
            .await
            .map(|_| true)
            .map_err(|_| Error::Internal("Health check failed".to_string()))
    }

    fn name(&self) -> &str {
        "pgvector"
    }
}
