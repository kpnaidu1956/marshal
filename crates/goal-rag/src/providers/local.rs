//! Local provider implementations using filesystem and ruvector-core
//!
//! These wrap the existing VectorStore and provide filesystem document storage.
//! Uses PostgreSQL tsvector FTS when postgres feature is enabled, SQLite FTS5 otherwise.

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::retrieval::VectorStore;
use crate::storage::ChunkContentRecord;
#[cfg(not(feature = "postgres"))]
use crate::storage::FileRegistryDb;
#[cfg(feature = "postgres")]
use crate::storage::PgFileRegistry;
use crate::types::Chunk;
use crate::types::response::StringSearchResult;

use super::document_store::{DocumentStoreProvider, StoredDocumentInfo};
use super::vector_store::{SearchFilter, VectorSearchResult, VectorStoreProvider};

/// Local vector store wrapping ruvector-core HNSW index
/// Uses HNSW for vector similarity search, PostgreSQL tsvector or SQLite FTS5 for text search
pub struct LocalVectorStore {
    store: Arc<VectorStore>,
    /// Database for FTS text search (PgFileRegistry or FileRegistryDb)
    #[cfg(feature = "postgres")]
    database: Arc<PgFileRegistry>,
    #[cfg(not(feature = "postgres"))]
    database: Arc<FileRegistryDb>,
    /// Flag indicating if migration is complete
    migration_complete: Arc<AtomicBool>,
}

impl LocalVectorStore {
    /// Create from existing VectorStore and database
    /// Migration runs asynchronously in the background to avoid blocking startup
    #[cfg(feature = "postgres")]
    pub fn new(store: Arc<VectorStore>, database: Arc<PgFileRegistry>) -> Self {
        let migration_complete = Arc::new(AtomicBool::new(false));

        let instance = Self {
            store: Arc::clone(&store),
            database: Arc::clone(&database),
            migration_complete: Arc::clone(&migration_complete),
        };

        // Run migration asynchronously to avoid blocking startup
        let store_clone = Arc::clone(&store);
        let db_clone = Arc::clone(&database);
        let migration_flag = Arc::clone(&migration_complete);

        tokio::spawn(async move {
            if let Err(e) = Self::migrate_to_fts_async(store_clone, db_clone).await {
                tracing::error!("FTS migration failed: {}", e);
            }
            migration_flag.store(true, Ordering::SeqCst);
        });

        instance
    }

    #[cfg(not(feature = "postgres"))]
    pub fn new(store: Arc<VectorStore>, database: Arc<FileRegistryDb>) -> Self {
        let migration_complete = Arc::new(AtomicBool::new(false));

        let instance = Self {
            store: Arc::clone(&store),
            database: Arc::clone(&database),
            migration_complete: Arc::clone(&migration_complete),
        };

        // Run migration asynchronously to avoid blocking startup
        let store_clone = Arc::clone(&store);
        let db_clone = Arc::clone(&database);
        let migration_flag = Arc::clone(&migration_complete);

        tokio::spawn(async move {
            if let Err(e) = Self::migrate_to_fts_async(store_clone, db_clone).await {
                tracing::error!("FTS migration failed: {}", e);
            }
            migration_flag.store(true, Ordering::SeqCst);
        });

        instance
    }

    /// Migrate existing HNSW chunks to FTS tables (async version)
    #[cfg(feature = "postgres")]
    async fn migrate_to_fts_async(
        store: Arc<VectorStore>,
        database: Arc<PgFileRegistry>,
    ) -> Result<()> {
        let (hnsw_count, chunks) = tokio::task::spawn_blocking({
            let store = Arc::clone(&store);
            move || -> Result<(usize, Vec<Chunk>)> {
                let count = store.len()?;
                if count == 0 {
                    return Ok((0, Vec::new()));
                }
                let chunks = store.get_all_chunks()?;
                Ok((count, chunks))
            }
        })
        .await
        .map_err(|e| Error::Internal(format!("Task join error: {}", e)))??;

        if hnsw_count == 0 {
            tracing::debug!("No chunks to migrate");
            return Ok(());
        }

        let existing_ids = database.get_all_chunk_ids().await?;
        let existing_set: std::collections::HashSet<_> = existing_ids.into_iter().collect();

        let new_chunks: Vec<_> = chunks.iter().filter(|c| !existing_set.contains(&c.id)).collect();

        if new_chunks.is_empty() {
            tracing::debug!(
                "FTS migration not needed - all {} HNSW chunks already in PostgreSQL",
                hnsw_count
            );
            return Ok(());
        }

        tracing::info!(
            "Migrating {} new chunks from HNSW to PostgreSQL FTS (existing: {}, total HNSW: {})",
            new_chunks.len(),
            existing_set.len(),
            hnsw_count
        );

        let records: Vec<ChunkContentRecord> = new_chunks
            .iter()
            .map(|c| Self::chunk_to_content_record(c))
            .collect();

        database.insert_chunks_content(&records).await?;

        let new_count = database.get_total_chunks_count().await?;
        tracing::info!("FTS migration complete: {} chunks now in PostgreSQL", new_count);

        Ok(())
    }

    #[cfg(not(feature = "postgres"))]
    async fn migrate_to_fts_async(
        store: Arc<VectorStore>,
        database: Arc<FileRegistryDb>,
    ) -> Result<()> {
        let (hnsw_count, chunks) = tokio::task::spawn_blocking({
            let store = Arc::clone(&store);
            move || -> Result<(usize, Vec<Chunk>)> {
                let count = store.len()?;
                if count == 0 {
                    return Ok((0, Vec::new()));
                }
                let chunks = store.get_all_chunks()?;
                Ok((count, chunks))
            }
        })
        .await
        .map_err(|e| Error::Internal(format!("Task join error: {}", e)))??;

        if hnsw_count == 0 {
            tracing::debug!("No chunks to migrate");
            return Ok(());
        }

        let existing_ids = database.get_all_chunk_ids()?;
        let existing_set: std::collections::HashSet<_> = existing_ids.into_iter().collect();

        let new_chunks: Vec<_> = chunks.iter().filter(|c| !existing_set.contains(&c.id)).collect();

        if new_chunks.is_empty() {
            tracing::debug!(
                "FTS migration not needed - all {} HNSW chunks already in SQLite FTS",
                hnsw_count
            );
            return Ok(());
        }

        tracing::info!(
            "Migrating {} new chunks from HNSW to SQLite FTS (existing FTS: {}, total HNSW: {})",
            new_chunks.len(),
            existing_set.len(),
            hnsw_count
        );

        let records: Vec<ChunkContentRecord> = new_chunks
            .iter()
            .map(|c| Self::chunk_to_content_record(c))
            .collect();

        database.insert_chunks_content(&records)?;

        let new_count = database.get_total_chunks_count()?;
        tracing::info!("FTS migration complete: {} chunks now in SQLite FTS", new_count);

        Ok(())
    }

    /// Check if migration is complete
    pub fn is_migration_complete(&self) -> bool {
        self.migration_complete.load(Ordering::SeqCst)
    }

    /// Get underlying store for direct access
    pub fn inner(&self) -> &Arc<VectorStore> {
        &self.store
    }

    /// Convert Chunk to ChunkContentRecord for storage
    fn chunk_to_content_record(chunk: &Chunk) -> ChunkContentRecord {
        ChunkContentRecord {
            id: chunk.id,
            document_id: chunk.document_id,
            chunk_index: chunk.chunk_index,
            content: chunk.content.clone(),
            filename: chunk.source.filename.clone(),
            file_type: chunk.source.file_type.clone(),
            page_number: chunk.source.page_number,
            section_title: chunk.source.section_title.clone(),
            char_start: chunk.char_start,
            char_end: chunk.char_end,
        }
    }
}

#[async_trait]
impl VectorStoreProvider for LocalVectorStore {
    async fn insert_chunk(&self, chunk: &Chunk) -> Result<()> {
        // Store in HNSW first (more likely to fail due to vector validation)
        let store = self.store.clone();
        let chunk_clone = chunk.clone();
        tokio::task::spawn_blocking(move || store.insert_chunk(&chunk_clone))
            .await
            .map_err(|e| Error::Internal(format!("Task join error: {}", e)))??;

        // Then store in database for FTS
        let record = Self::chunk_to_content_record(chunk);
        #[cfg(feature = "postgres")]
        let db_result = self.database.insert_chunk_content(&record).await;
        #[cfg(not(feature = "postgres"))]
        let db_result = self.database.insert_chunk_content(&record);

        if let Err(e) = db_result {
            // Rollback: delete from HNSW
            tracing::warn!("FTS insert failed, rolling back HNSW insert: {}", e);
            let store = self.store.clone();
            let chunk_id = chunk.id;
            let _ = tokio::task::spawn_blocking(move || {
                store.delete_chunk(&chunk_id.to_string())
            })
            .await;
            return Err(e);
        }

        Ok(())
    }

    async fn insert_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        // Store in HNSW first (collect successful IDs for potential rollback)
        let store = self.store.clone();
        let chunks_vec = chunks.to_vec();
        let inserted_ids: Vec<String> = tokio::task::spawn_blocking(move || {
            let mut ids = Vec::with_capacity(chunks_vec.len());
            for chunk in &chunks_vec {
                store.insert_chunk(chunk)?;
                ids.push(chunk.id.to_string());
            }
            Ok::<_, Error>(ids)
        })
        .await
        .map_err(|e| Error::Internal(format!("Task join error: {}", e)))??;

        // Then store in database for FTS
        let records: Vec<ChunkContentRecord> = chunks
            .iter()
            .map(Self::chunk_to_content_record)
            .collect();

        #[cfg(feature = "postgres")]
        let db_result = self.database.insert_chunks_content(&records).await;
        #[cfg(not(feature = "postgres"))]
        let db_result = self.database.insert_chunks_content(&records);

        if let Err(e) = db_result {
            // Rollback: delete all inserted chunks from HNSW
            tracing::warn!(
                "FTS batch insert failed, rolling back {} HNSW inserts: {}",
                inserted_ids.len(),
                e
            );
            let store = self.store.clone();
            let _ = tokio::task::spawn_blocking(move || {
                for id in &inserted_ids {
                    let _ = store.delete_chunk(id);
                }
            })
            .await;
            return Err(e);
        }

        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        filter: &SearchFilter,
    ) -> Result<Vec<VectorSearchResult>> {
        let store = self.store.clone();
        let query = query_embedding.to_vec();
        // Extract document IDs from filter (local store doesn't support org filtering in HNSW)
        let doc_filter = filter.document_ids.clone();

        tokio::task::spawn_blocking(move || {
            let results = store.search(&query, top_k, doc_filter.as_deref())?;
            Ok(results
                .into_iter()
                .map(|r| VectorSearchResult {
                    chunk: r.chunk,
                    similarity: r.similarity,
                })
                .collect())
        })
        .await
        .map_err(|e| Error::Internal(format!("Task join error: {}", e)))?
    }

    async fn string_search(
        &self,
        query: &str,
        limit: usize,
        organization_id: &str,
    ) -> Result<Vec<StringSearchResult>> {
        #[cfg(feature = "postgres")]
        let fts_results = self.database.string_search_chunks_filtered(query, limit, Some(organization_id)).await?;
        #[cfg(not(feature = "postgres"))]
        let fts_results = self.database.string_search_chunks_filtered(query, limit, Some(organization_id))?;

        // Convert FTS results to StringSearchResult
        let query_lower = query.to_lowercase();
        let results: Vec<StringSearchResult> = fts_results
            .into_iter()
            .map(|r| {
                // Find match positions
                let content_lower = r.content.to_lowercase();
                let match_positions: Vec<usize> = content_lower
                    .match_indices(&query_lower)
                    .map(|(pos, _)| pos)
                    .collect();

                // Create highlighted snippet
                let highlighted = Self::highlight_matches(&r.content, query);
                let preview = Self::create_preview(&r.content, query);

                StringSearchResult {
                    chunk_id: r.chunk_id,
                    document_id: r.document_id,
                    filename: r.filename,
                    file_type: r.file_type,
                    page_number: r.page_number,
                    match_count: match_positions.len(),
                    match_positions,
                    preview,
                    highlighted_snippet: highlighted,
                }
            })
            .collect();

        Ok(results)
    }

    async fn delete_by_document(&self, document_id: &Uuid) -> Result<usize> {
        // Delete from FTS database
        #[cfg(feature = "postgres")]
        self.database.delete_chunks_by_document(document_id).await?;
        #[cfg(not(feature = "postgres"))]
        self.database.delete_chunks_by_document(document_id)?;

        // Delete from HNSW
        let store = self.store.clone();
        let doc_id = *document_id;
        tokio::task::spawn_blocking(move || store.delete_by_document(&doc_id))
            .await
            .map_err(|e| Error::Internal(format!("Task join error: {}", e)))?
    }

    async fn len(&self) -> Result<usize> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.len())
            .await
            .map_err(|e| Error::Internal(format!("Task join error: {}", e)))?
    }

    async fn health_check(&self) -> Result<bool> {
        // Local store is always healthy if it exists
        Ok(true)
    }

    fn name(&self) -> &str {
        "local-hnsw"
    }
}

impl LocalVectorStore {
    /// Highlight query matches in content using <mark> tags
    /// Uses character-based indexing to handle UTF-8 safely
    fn highlight_matches(content: &str, query: &str) -> String {
        if query.is_empty() {
            return content.to_string();
        }

        let query_lower = query.to_lowercase();
        let content_lower = content.to_lowercase();

        // Find all match positions in the lowercase version (byte positions)
        let matches: Vec<usize> = content_lower
            .match_indices(&query_lower)
            .map(|(pos, _)| pos)
            .collect();

        if matches.is_empty() {
            return content.to_string();
        }

        // Build result by finding corresponding positions in original string
        // We need to map byte positions from lowercase to original
        let mut result = String::with_capacity(content.len() + matches.len() * 13);
        let mut last_end = 0;

        for &match_start in &matches {
            // Find the actual match length in original string
            // by iterating characters until we've consumed the same bytes
            let match_in_original = &content[match_start..];
            let query_char_count = query_lower.chars().count();
            let actual_match: String = match_in_original.chars().take(query_char_count).collect();
            let actual_len = actual_match.len();

            // Append text before match
            result.push_str(&content[last_end..match_start]);
            // Append highlighted match
            result.push_str("<mark>");
            result.push_str(&content[match_start..match_start + actual_len]);
            result.push_str("</mark>");

            last_end = match_start + actual_len;
        }

        // Append remaining text
        result.push_str(&content[last_end..]);
        result
    }

    /// Create preview with context around first match
    /// Uses character-based indexing to handle UTF-8 safely
    fn create_preview(content: &str, query: &str) -> String {
        let query_lower = query.to_lowercase();
        let content_lower = content.to_lowercase();

        if let Some(byte_pos) = content_lower.find(&query_lower) {
            // Convert byte position to character position for safe slicing
            let char_pos = content[..byte_pos].chars().count();
            let content_chars: Vec<char> = content.chars().collect();
            let query_char_count = query.chars().count();
            let total_chars = content_chars.len();

            // Calculate preview bounds in characters
            let preview_start = char_pos.saturating_sub(50);
            let preview_end = (char_pos + query_char_count + 50).min(total_chars);

            // Build preview string
            let preview: String = content_chars[preview_start..preview_end].iter().collect();

            let mut result = preview;
            if preview_start > 0 {
                result = format!("...{}", result);
            }
            if preview_end < total_chars {
                result = format!("{}...", result);
            }
            result
        } else {
            content.chars().take(100).collect()
        }
    }
}

/// Local document store using filesystem
pub struct LocalDocumentStore {
    /// Directory to store documents
    storage_dir: PathBuf,
}

impl LocalDocumentStore {
    /// Create a new local document store
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&storage_dir)?;
        Ok(Self { storage_dir })
    }

    /// Get path for a document
    fn doc_path(&self, doc_id: &Uuid) -> PathBuf {
        self.storage_dir.join(format!("{}.bin", doc_id))
    }

    /// Get metadata path for a document
    fn meta_path(&self, doc_id: &Uuid) -> PathBuf {
        self.storage_dir.join(format!("{}.meta.json", doc_id))
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DocumentMeta {
    id: Uuid,
    filename: String,
    size: u64,
}

#[async_trait]
impl DocumentStoreProvider for LocalDocumentStore {
    async fn store_document(
        &self,
        doc_id: &Uuid,
        filename: &str,
        data: &[u8],
        _organization_id: Option<&str>,
    ) -> Result<String> {
        let doc_path = self.doc_path(doc_id);
        let meta_path = self.meta_path(doc_id);

        // Write document data
        tokio::fs::write(&doc_path, data).await?;

        // Write metadata
        let meta = DocumentMeta {
            id: *doc_id,
            filename: filename.to_string(),
            size: data.len() as u64,
        };
        let meta_json = serde_json::to_string_pretty(&meta)?;
        tokio::fs::write(&meta_path, meta_json).await?;

        Ok(doc_path.to_string_lossy().to_string())
    }

    async fn get_document(&self, doc_id: &Uuid, _organization_id: Option<&str>) -> Result<Vec<u8>> {
        let doc_path = self.doc_path(doc_id);
        tokio::fs::read(&doc_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to read document {}: {}", doc_id, e)))
    }

    async fn exists(&self, doc_id: &Uuid, _organization_id: Option<&str>) -> Result<bool> {
        let doc_path = self.doc_path(doc_id);
        Ok(doc_path.exists())
    }

    async fn delete_document(&self, doc_id: &Uuid, _organization_id: Option<&str>) -> Result<()> {
        let doc_path = self.doc_path(doc_id);
        let meta_path = self.meta_path(doc_id);

        if doc_path.exists() {
            tokio::fs::remove_file(&doc_path).await?;
        }
        if meta_path.exists() {
            tokio::fs::remove_file(&meta_path).await?;
        }

        Ok(())
    }

    async fn list_documents(&self, _organization_id: Option<&str>) -> Result<Vec<StoredDocumentInfo>> {
        let mut docs = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.storage_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    if let Ok(meta) = serde_json::from_str::<DocumentMeta>(&content) {
                        let doc_path = self.doc_path(&meta.id);
                        docs.push(StoredDocumentInfo {
                            id: meta.id,
                            filename: meta.filename,
                            uri: doc_path.to_string_lossy().to_string(),
                            size: meta.size,
                            organization_id: None,  // Local store doesn't track org
                        });
                    }
                }
            }
        }

        Ok(docs)
    }

    async fn get_uri(&self, doc_id: &Uuid, _organization_id: Option<&str>) -> Result<Option<String>> {
        let doc_path = self.doc_path(doc_id);
        if doc_path.exists() {
            Ok(Some(doc_path.to_string_lossy().to_string()))
        } else {
            Ok(None)
        }
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(self.storage_dir.exists())
    }

    fn name(&self) -> &str {
        "local-filesystem"
    }
}
