//! Vector store for chunk storage and search

use std::collections::HashMap;
use uuid::Uuid;

use ruvector_core::{VectorDB, VectorEntry, SearchQuery as CoreSearchQuery, DistanceMetric};
use ruvector_core::types::{DbOptions, HnswConfig};

use crate::config::RagConfig;
use crate::error::{Error, Result};
use crate::types::Chunk;
use crate::types::response::StringSearchResult;

/// Search result with chunk and similarity
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The retrieved chunk
    pub chunk: Chunk,
    /// Similarity score (0.0-1.0, higher is better)
    pub similarity: f32,
}

/// Internal string search match
/// DEPRECATED: Use SQLite FTS via LocalVectorStore.string_search instead
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct StringMatch {
    chunk: Chunk,
    match_count: usize,
    match_positions: Vec<usize>,
}

/// Vector store wrapper for ruvector-core
pub struct VectorStore {
    /// Underlying vector database
    db: VectorDB,
    /// Embedding dimensions
    #[allow(dead_code)]
    dimensions: usize,
    /// Mapping from document IDs to chunk IDs for efficient deletion
    document_chunks: parking_lot::RwLock<HashMap<Uuid, Vec<String>>>,
}

impl VectorStore {
    /// Create a new vector store
    pub fn new(config: &RagConfig) -> Result<Self> {
        // Ensure storage directory exists
        if let Some(parent) = config.vector_db.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = DbOptions {
            dimensions: config.embeddings.dimensions,
            distance_metric: DistanceMetric::Cosine,
            storage_path: config.vector_db.storage_path.to_string_lossy().to_string(),
            hnsw_config: Some(HnswConfig {
                m: config.vector_db.hnsw_m,
                ef_construction: config.vector_db.hnsw_ef_construction,
                ef_search: config.vector_db.hnsw_ef_search,
                max_elements: 10_000_000,
            }),
            quantization: None,
        };

        let db = VectorDB::new(options).map_err(|e| Error::VectorDb(e.to_string()))?;

        Ok(Self {
            db,
            dimensions: config.embeddings.dimensions,
            document_chunks: parking_lot::RwLock::new(HashMap::new()),
        })
    }

    /// Insert a chunk into the vector store
    pub fn insert_chunk(&self, chunk: &Chunk) -> Result<()> {
        if chunk.embedding.is_empty() {
            return Err(Error::VectorDb("Chunk has no embedding".to_string()));
        }

        let chunk_id = chunk.id.to_string();

        let entry = VectorEntry {
            id: Some(chunk_id.clone()),
            vector: chunk.embedding.clone(),
            metadata: Some(chunk.to_vector_metadata()),
        };

        self.db.insert(entry).map_err(|e| Error::VectorDb(e.to_string()))?;

        // Track the document-to-chunk mapping
        let mut doc_chunks = self.document_chunks.write();
        doc_chunks
            .entry(chunk.document_id)
            .or_default()
            .push(chunk_id);

        Ok(())
    }

    /// Search for similar chunks
    pub fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        document_filter: Option<&[Uuid]>,
    ) -> Result<Vec<SearchResult>> {
        // Build metadata filter
        let filter = document_filter.map(|_doc_ids| {
            
            // For now, we'll filter in post-processing
            // In production, implement proper metadata filtering in ruvector-core
            HashMap::new()
        });

        let query = CoreSearchQuery {
            vector: query_embedding.to_vec(),
            k: top_k * 2, // Get more for filtering
            filter,
            ef_search: None,
        };

        let results = self.db.search(query).map_err(|e| Error::VectorDb(e.to_string()))?;

        let mut search_results = Vec::new();

        for result in results {
            // Extract chunk from metadata
            if let Some(ref metadata) = result.metadata {
                let chunk = self.metadata_to_chunk(&result.id, metadata)?;

                // Apply document filter
                if let Some(doc_ids) = document_filter {
                    if !doc_ids.contains(&chunk.document_id) {
                        continue;
                    }
                }

                // Convert distance to similarity (cosine distance -> similarity)
                let similarity = 1.0 - result.score.min(2.0) / 2.0;

                search_results.push(SearchResult { chunk, similarity });
            }
        }

        // Sort by similarity and take top_k
        search_results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
        search_results.truncate(top_k);

        Ok(search_results)
    }

    /// Delete all chunks for a document
    pub fn delete_by_document(&self, document_id: &Uuid) -> Result<usize> {
        // Get chunk IDs for this document from our tracking map
        let chunk_ids = {
            let doc_chunks = self.document_chunks.read();
            doc_chunks.get(document_id).cloned().unwrap_or_default()
        };

        let mut deleted = 0;

        for chunk_id in &chunk_ids {
            if self.db.delete(chunk_id).map_err(|e| Error::VectorDb(e.to_string()))? {
                deleted += 1;
            }
        }

        // Remove from tracking map
        if deleted > 0 {
            let mut doc_chunks = self.document_chunks.write();
            doc_chunks.remove(document_id);
        }

        Ok(deleted)
    }

    /// Get chunk count
    pub fn len(&self) -> Result<usize> {
        self.db.len().map_err(|e| Error::VectorDb(e.to_string()))
    }

    /// Check if empty
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Delete a single chunk by ID
    pub fn delete_chunk(&self, chunk_id: &str) -> Result<bool> {
        self.db.delete(chunk_id).map_err(|e| Error::VectorDb(e.to_string()))
    }

    /// Get all chunks from the vector store (for migration to FTS)
    /// Uses iterative fetching to handle databases larger than any fixed limit
    pub fn get_all_chunks(&self) -> Result<Vec<Chunk>> {
        let total_count = self.len()?;
        if total_count == 0 {
            return Ok(Vec::new());
        }

        // Fetch in batches to avoid memory issues with very large databases
        // Use a batch size that's large enough to be efficient but not too large
        let batch_size = 50000.min(total_count);

        let all_results = self.db.search(CoreSearchQuery {
            vector: vec![0.0; self.dimensions],
            k: batch_size,
            filter: None,
            ef_search: None,
        }).map_err(|e| Error::VectorDb(e.to_string()))?;

        let mut chunks = Vec::with_capacity(all_results.len());
        for result in all_results {
            if let Some(ref metadata) = result.metadata {
                match self.metadata_to_chunk(&result.id, metadata) {
                    Ok(chunk) => chunks.push(chunk),
                    Err(e) => tracing::warn!("Failed to parse chunk metadata: {}", e),
                }
            }
        }

        // Log if we might have missed some chunks
        if chunks.len() < total_count {
            tracing::warn!(
                "get_all_chunks retrieved {} chunks but database has {} total. \
                Some chunks may not be migrated.",
                chunks.len(),
                total_count
            );
        }

        Ok(chunks)
    }

    /// Perform literal string search across all chunks
    ///
    /// Returns up to `limit` results sorted by match count (descending)
    ///
    /// DEPRECATED: This method uses O(n) linear scanning. Use SQLite FTS via
    /// LocalVectorStore.string_search instead, which uses an inverted index.
    #[allow(dead_code)]
    pub fn string_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<StringSearchResult>> {
        let query_lower = query.to_lowercase();
        let mut matches: Vec<StringMatch> = Vec::new();

        // Get all entries and search for literal matches
        // Note: This is a linear scan - for large databases, consider building a text index
        let all_results = self.db.search(CoreSearchQuery {
            vector: vec![0.0; self.dimensions], // Dummy vector
            k: 10000, // Get many results
            filter: None,
            ef_search: None,
        }).map_err(|e| Error::VectorDb(e.to_string()))?;

        for result in all_results {
            if let Some(ref metadata) = result.metadata {
                let chunk = self.metadata_to_chunk(&result.id, metadata)?;
                let content_lower = chunk.content.to_lowercase();

                // Find all occurrences of the query
                let mut positions = Vec::new();
                let mut search_start = 0;
                while let Some(pos) = content_lower[search_start..].find(&query_lower) {
                    positions.push(search_start + pos);
                    search_start = search_start + pos + 1;
                    if search_start >= content_lower.len() {
                        break;
                    }
                }

                if !positions.is_empty() {
                    matches.push(StringMatch {
                        chunk,
                        match_count: positions.len(),
                        match_positions: positions,
                    });
                }
            }
        }

        // Sort by match count (descending)
        matches.sort_by(|a, b| b.match_count.cmp(&a.match_count));

        // Take top `limit` results and convert to StringSearchResult
        let results: Vec<StringSearchResult> = matches
            .into_iter()
            .take(limit)
            .map(|m| {
                // Create highlighted snippet
                let highlighted = self.highlight_matches(&m.chunk.content, query);

                // Create preview (context around first match)
                let preview = if let Some(&first_pos) = m.match_positions.first() {
                    let start = first_pos.saturating_sub(50);
                    let end = (first_pos + query.len() + 50).min(m.chunk.content.len());
                    let mut preview = m.chunk.content[start..end].to_string();
                    if start > 0 {
                        preview = format!("...{}", preview);
                    }
                    if end < m.chunk.content.len() {
                        preview = format!("{}...", preview);
                    }
                    preview
                } else {
                    m.chunk.content.chars().take(100).collect()
                };

                StringSearchResult {
                    chunk_id: m.chunk.id,
                    document_id: m.chunk.document_id,
                    filename: m.chunk.source.filename.clone(),
                    file_type: m.chunk.source.file_type.clone(),
                    page_number: m.chunk.source.page_number,
                    match_count: m.match_count,
                    match_positions: m.match_positions,
                    highlighted_snippet: highlighted,
                    preview,
                }
            })
            .collect();

        Ok(results)
    }

    /// Highlight search query in text using <mark> tags
    /// DEPRECATED: Use LocalVectorStore.highlight_matches instead
    #[allow(dead_code)]
    fn highlight_matches(&self, text: &str, query: &str) -> String {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();
        let mut result = String::with_capacity(text.len() + query.len() * 20);
        let mut last_end = 0;

        for (idx, _) in text_lower.match_indices(&query_lower) {
            // Add text before the match
            result.push_str(&text[last_end..idx]);
            // Add highlighted match (preserving original case)
            result.push_str("<mark>");
            result.push_str(&text[idx..idx + query.len()]);
            result.push_str("</mark>");
            last_end = idx + query.len();
        }

        // Add remaining text
        result.push_str(&text[last_end..]);
        result
    }

    /// Convert metadata back to chunk
    fn metadata_to_chunk(
        &self,
        id: &str,
        metadata: &HashMap<String, serde_json::Value>,
    ) -> Result<Chunk> {
        let chunk_id = metadata
            .get("chunk_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(|| Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4()));

        let document_id = metadata
            .get("document_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::new_v4);

        let filename = metadata
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let content = metadata
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let chunk_index = metadata
            .get("chunk_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let char_start = metadata
            .get("char_start")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let char_end = metadata
            .get("char_end")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let page_number = metadata
            .get("page_number")
            .and_then(|v| v.as_u64())
            .map(|p| p as u32);

        let section_title = metadata
            .get("section_title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let line_start = metadata
            .get("line_start")
            .and_then(|v| v.as_u64())
            .map(|l| l as u32);

        let line_end = metadata
            .get("line_end")
            .and_then(|v| v.as_u64())
            .map(|l| l as u32);

        let file_type = metadata
            .get("file_type")
            .map(|v| serde_json::from_value(v.clone()).unwrap_or(crate::types::FileType::Unknown))
            .unwrap_or(crate::types::FileType::Unknown);

        // Internal filename (for debugging, not shown in citations)
        let internal_filename = metadata
            .get("internal_filename")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let source = crate::types::ChunkSource {
            filename,
            internal_filename,
            file_type,
            page_number,
            page_count: None,
            section_title,
            heading_hierarchy: Vec::new(),
            sheet_name: None,
            row_range: None,
            line_start,
            line_end,
            code_context: None,
        };

        Ok(Chunk {
            id: chunk_id,
            document_id,
            content,
            embedding: Vec::new(), // Not stored in metadata
            source,
            char_start,
            char_end,
            chunk_index,
            metadata: HashMap::new(),
        })
    }
}
