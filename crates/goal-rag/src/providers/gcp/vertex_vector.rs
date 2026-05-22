//! Vertex AI Vector Search provider
//!
//! Provides managed HNSW vector similarity search with database FTS for text search.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::auth::GcpAuth;
use crate::error::{Error, Result};
use crate::providers::vector_store::{SearchFilter, VectorSearchResult, VectorStoreProvider};
use crate::storage::ChunkContentRecord;
#[cfg(not(feature = "postgres"))]
use crate::storage::FileRegistryDb;
#[cfg(feature = "postgres")]
use crate::storage::PgFileRegistry;
use crate::types::Chunk;
use crate::types::response::StringSearchResult;

/// Vertex AI Vector Search provider
pub struct VertexVectorSearch {
    auth: Arc<GcpAuth>,
    location: String,
    /// Index resource for upsert/delete operations
    index: String,
    /// IndexEndpoint resource for query operations
    index_endpoint: String,
    /// Public endpoint domain for queries (required for public endpoints)
    public_domain: Option<String>,
    deployed_index_id: String,
    /// Endpoint for data plane operations (upsert, delete)
    data_endpoint: Option<String>,
    /// Database for chunk content (FTS) and document-chunk mapping
    #[cfg(feature = "postgres")]
    database: Arc<PgFileRegistry>,
    #[cfg(not(feature = "postgres"))]
    database: Arc<FileRegistryDb>,
}

impl VertexVectorSearch {
    /// Create a new Vertex Vector Search provider
    #[cfg(feature = "postgres")]
    pub fn new(
        auth: Arc<GcpAuth>,
        location: String,
        index: String,
        index_endpoint: String,
        public_domain: Option<String>,
        deployed_index_id: String,
        database: Arc<PgFileRegistry>,
    ) -> Self {
        Self {
            auth,
            location,
            index,
            index_endpoint,
            public_domain,
            deployed_index_id,
            data_endpoint: None,
            database,
        }
    }

    /// Create a new Vertex Vector Search provider
    #[cfg(not(feature = "postgres"))]
    pub fn new(
        auth: Arc<GcpAuth>,
        location: String,
        index: String,
        index_endpoint: String,
        public_domain: Option<String>,
        deployed_index_id: String,
        database: Arc<FileRegistryDb>,
    ) -> Self {
        Self {
            auth,
            location,
            index,
            index_endpoint,
            public_domain,
            deployed_index_id,
            data_endpoint: None,
            database,
        }
    }

    /// Set a custom data endpoint for mutations
    pub fn with_data_endpoint(mut self, endpoint: String) -> Self {
        self.data_endpoint = Some(endpoint);
        self
    }

    /// Store multiple chunk contents in database for FTS
    async fn store_chunks_content(&self, chunks: &[Chunk]) -> Result<()> {
        let records: Vec<ChunkContentRecord> = chunks.iter().map(|chunk| {
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
        }).collect();
        #[cfg(feature = "postgres")]
        self.database.insert_chunks_content(&records).await?;
        #[cfg(not(feature = "postgres"))]
        self.database.insert_chunks_content(&records)?;
        Ok(())
    }

    /// Get search endpoint URL
    fn search_endpoint(&self) -> String {
        if let Some(ref domain) = self.public_domain {
            // Use public endpoint domain for queries
            format!(
                "https://{}/v1/{}:findNeighbors",
                domain, self.index_endpoint
            )
        } else {
            // Fall back to standard API endpoint
            format!(
                "https://{}-aiplatform.googleapis.com/v1/{}:findNeighbors",
                self.location, self.index_endpoint
            )
        }
    }

    /// Convert chunk to vector search datapoint with optional organization_id
    fn chunk_to_datapoint(chunk: &Chunk, organization_id: Option<&str>) -> DataPoint {
        // Add document_id and organization_id as restricts for filtering
        let mut restricts = vec![Restrict {
            namespace: "document_id".to_string(),
            allow: vec![chunk.document_id.to_string()],
            deny: vec![],
        }];

        // Add organization_id restrict if provided
        if let Some(org_id) = organization_id {
            restricts.push(Restrict {
                namespace: "organization_id".to_string(),
                allow: vec![org_id.to_string()],
                deny: vec![],
            });
        }

        // Store chunk metadata in crowding tag (up to 1KB)
        let metadata = serde_json::json!({
            "chunk_id": chunk.id.to_string(),
            "document_id": chunk.document_id.to_string(),
            "organization_id": organization_id,
            "filename": chunk.source.filename,
            "content": chunk.content.chars().take(500).collect::<String>(),
            "chunk_index": chunk.chunk_index,
            "char_start": chunk.char_start,
            "char_end": chunk.char_end,
            "page_number": chunk.source.page_number,
            "section_title": chunk.source.section_title,
            "file_type": chunk.source.file_type,
        });

        DataPoint {
            datapoint_id: chunk.id.to_string(),
            feature_vector: chunk.embedding.clone(),
            restricts: Some(restricts),
            crowding_tag: Some(CrowdingTag {
                crowding_attribute: metadata.to_string(),
            }),
        }
    }
}

#[derive(serde::Serialize, Clone)]
struct DataPoint {
    datapoint_id: String,
    feature_vector: Vec<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restricts: Option<Vec<Restrict>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    crowding_tag: Option<CrowdingTag>,
}

#[derive(serde::Serialize, Clone)]
struct Restrict {
    namespace: String,
    #[serde(rename = "allowList", skip_serializing_if = "Vec::is_empty")]
    allow: Vec<String>,
    #[serde(rename = "denyList", skip_serializing_if = "Vec::is_empty")]
    deny: Vec<String>,
}

#[derive(serde::Serialize, Clone)]
struct CrowdingTag {
    crowding_attribute: String,
}

#[derive(serde::Serialize)]
struct FindNeighborsRequest {
    deployed_index_id: String,
    queries: Vec<QueryItem>,
}

#[derive(serde::Serialize)]
struct QueryItem {
    datapoint: QueryDatapoint,
    neighbor_count: u32,
}

#[derive(serde::Serialize)]
struct QueryDatapoint {
    datapoint_id: String,
    feature_vector: Vec<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restricts: Option<Vec<Restrict>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindNeighborsResponse {
    nearest_neighbors: Vec<NearestNeighbors>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NearestNeighbors {
    neighbors: Vec<Neighbor>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Neighbor {
    datapoint: NeighborDatapoint,
    distance: f64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct NeighborDatapoint {
    datapoint_id: String,
    crowding_tag: Option<NeighborCrowdingTag>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NeighborCrowdingTag {
    crowding_attribute: String,
}

#[derive(serde::Serialize)]
struct UpsertRequest {
    datapoints: Vec<DataPoint>,
}

#[derive(serde::Serialize)]
#[allow(dead_code)]
struct RemoveRequest {
    datapoint_ids: Vec<String>,
}

#[async_trait]
impl VectorStoreProvider for VertexVectorSearch {
    async fn insert_chunk(&self, chunk: &Chunk) -> Result<()> {
        self.insert_chunks(std::slice::from_ref(chunk)).await
    }

    async fn insert_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        // Store chunk content in database for FTS and document mapping
        self.store_chunks_content(chunks).await?;

        let client = self.auth.authorized_client().await?;

        // Use data endpoint if available, otherwise use Index resource for upserts
        let endpoint = self.data_endpoint.clone().unwrap_or_else(|| {
            format!(
                "https://{}-aiplatform.googleapis.com/v1/{}:upsertDatapoints",
                self.location, self.index
            )
        });

        // Convert chunks to datapoints (organization_id will be added during ingestion)
        // TODO: Pass organization_id through insert_chunks when available
        let datapoints: Vec<DataPoint> = chunks.iter().map(|c| Self::chunk_to_datapoint(c, None)).collect();

        // Batch upserts with rate limiting and retry logic
        // Use smaller batches (50) and add delays to avoid quota exhaustion
        let batch_size = 50;
        let base_delay_ms = 200; // Delay between batches
        let max_retries = 5;

        for (batch_idx, batch) in datapoints.chunks(batch_size).enumerate() {
            let request = UpsertRequest {
                datapoints: batch.to_vec(),
            };

            // Retry loop with exponential backoff for rate limiting
            let mut retry_count = 0;
            loop {
                let response = client
                    .post(&endpoint)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| Error::VectorDb(format!("Vertex upsert failed: {}", e)))?;

                if response.status().is_success() {
                    break; // Success, move to next batch
                }

                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                // Handle rate limiting (429) with exponential backoff
                if status.as_u16() == 429 && retry_count < max_retries {
                    retry_count += 1;
                    let backoff_ms = base_delay_ms * (2_u64.pow(retry_count as u32));
                    tracing::warn!(
                        "Vertex AI rate limited (429), retry {}/{} after {}ms",
                        retry_count, max_retries, backoff_ms
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                    continue;
                }

                return Err(Error::VectorDb(format!(
                    "Vertex upsert failed ({}): {}",
                    status, body
                )));
            }

            // Add delay between batches to avoid quota exhaustion
            if batch_idx < datapoints.chunks(batch_size).len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(base_delay_ms)).await;
            }
        }

        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        filter: &SearchFilter,
    ) -> Result<Vec<VectorSearchResult>> {
        let client = self.auth.authorized_client().await?;

        // Build restricts from filter — always include organization_id for tenant isolation
        let mut restricts_vec = Vec::new();

        // Organization filter is always applied — no cross-org leakage
        restricts_vec.push(Restrict {
            namespace: "organization_id".to_string(),
            allow: vec![filter.organization_id.clone()],
            deny: vec![],
        });

        // Add document_ids restrict if provided
        if let Some(ref doc_ids) = filter.document_ids {
            if !doc_ids.is_empty() {
                restricts_vec.push(Restrict {
                    namespace: "document_id".to_string(),
                    allow: doc_ids.iter().map(|id| id.to_string()).collect(),
                    deny: vec![],
                });
            }
        }

        let restricts = Some(restricts_vec);

        let request = FindNeighborsRequest {
            deployed_index_id: self.deployed_index_id.clone(),
            queries: vec![QueryItem {
                datapoint: QueryDatapoint {
                    datapoint_id: "query".to_string(),
                    feature_vector: query_embedding.to_vec(),
                    restricts,
                },
                neighbor_count: top_k as u32,
            }],
        };

        let response = client
            .post(self.search_endpoint())
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::VectorDb(format!("Vertex search failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::VectorDb(format!(
                "Vertex search failed ({}): {}",
                status, body
            )));
        }

        let search_response: FindNeighborsResponse = response
            .json()
            .await
            .map_err(|e| Error::VectorDb(format!("Failed to parse Vertex response: {}", e)))?;

        let mut results = Vec::new();

        for neighbors in search_response.nearest_neighbors {
            for neighbor in neighbors.neighbors {
                let datapoint_id = &neighbor.datapoint.datapoint_id;
                let similarity = 1.0 - neighbor.distance as f32;

                // Try to parse chunk_id from datapoint_id
                let chunk_id = match Uuid::parse_str(datapoint_id) {
                    Ok(id) => id,
                    Err(_) => {
                        tracing::warn!("Invalid UUID datapoint_id: {}", datapoint_id);
                        continue;
                    }
                };

                // Try to parse metadata from crowding tag
                let chunk = if let Some(crowding) = &neighbor.datapoint.crowding_tag {
                    match serde_json::from_str::<HashMap<String, serde_json::Value>>(
                        &crowding.crowding_attribute,
                    ) {
                        Ok(metadata) => self.metadata_to_chunk(&metadata)?,
                        Err(_) => {
                            // Crowding attribute is hashed - create minimal chunk with just ID
                            // The caller should look up full chunk data from local store
                            tracing::debug!(
                                "Crowding attribute not JSON for {}, returning minimal chunk",
                                datapoint_id
                            );
                            self.create_minimal_chunk(chunk_id)
                        }
                    }
                } else {
                    // No crowding tag - create minimal chunk
                    self.create_minimal_chunk(chunk_id)
                };

                results.push(VectorSearchResult { chunk, similarity });
            }
        }

        Ok(results)
    }

    async fn string_search(
        &self,
        query: &str,
        limit: usize,
        organization_id: &str,
    ) -> Result<Vec<StringSearchResult>> {
        // Use database FTS for string search with organization filter
        #[cfg(feature = "postgres")]
        let results = self.database.string_search_chunks_filtered(query, limit, Some(organization_id)).await?;
        #[cfg(not(feature = "postgres"))]
        let results = self.database.string_search_chunks_filtered(query, limit, Some(organization_id))?;

        // Convert to StringSearchResult format
        let search_results: Vec<StringSearchResult> = results.into_iter().map(|r| {
            // Find match positions
            let query_lower = query.to_lowercase();
            let content_lower = r.content.to_lowercase();
            let mut positions = Vec::new();
            let mut search_start = 0;
            while let Some(pos) = content_lower[search_start..].find(&query_lower) {
                positions.push(search_start + pos);
                search_start = search_start + pos + 1;
                if search_start >= content_lower.len() {
                    break;
                }
            }

            // Create highlighted snippet
            let highlighted = self.highlight_matches(&r.content, query);

            // Create preview (using char-safe slicing to handle UTF-8)
            let preview = if let Some(&first_pos) = positions.first() {
                // Find character boundaries safely
                let chars: Vec<char> = r.content.chars().collect();
                let char_count = chars.len();

                // Convert byte position to approximate char position
                let approx_char_pos = r.content[..first_pos.min(r.content.len())]
                    .chars()
                    .count();

                let start_char = approx_char_pos.saturating_sub(50);
                let end_char = (approx_char_pos + 50).min(char_count);

                let mut preview: String = chars[start_char..end_char].iter().collect();
                if start_char > 0 {
                    preview = format!("...{}", preview);
                }
                if end_char < char_count {
                    preview = format!("{}...", preview);
                }
                preview
            } else {
                r.content.chars().take(100).collect()
            };

            StringSearchResult {
                chunk_id: r.chunk_id,
                document_id: r.document_id,
                filename: r.filename,
                file_type: r.file_type,
                page_number: r.page_number,
                match_count: positions.len(),
                match_positions: positions,
                highlighted_snippet: highlighted,
                preview,
            }
        }).collect();

        Ok(search_results)
    }

    async fn delete_by_document(&self, document_id: &Uuid) -> Result<usize> {
        // Get chunk count from database
        #[cfg(feature = "postgres")]
        let chunk_count = self.database.get_chunks_count_for_document(document_id).await?;
        #[cfg(not(feature = "postgres"))]
        let chunk_count = self.database.get_chunks_count_for_document(document_id)?;

        if chunk_count == 0 {
            return Ok(0);
        }

        // Delete chunks from database
        #[cfg(feature = "postgres")]
        let deleted = self.database.delete_chunks_by_document(document_id).await?;
        #[cfg(not(feature = "postgres"))]
        let deleted = self.database.delete_chunks_by_document(document_id)?;

        // Note: We can't easily delete from Vertex without knowing the exact datapoint IDs
        // The datapoint_id is the chunk.id (UUID), which we just deleted from SQLite
        // In a full implementation, we would:
        // 1. First query SQLite for chunk IDs before deleting
        // 2. Call Vertex AI delete API with those IDs
        // 3. Then delete from SQLite

        tracing::info!(
            "Deleted {} chunks from database for document {}. Vertex cleanup pending.",
            deleted, document_id
        );

        Ok(deleted)
    }

    async fn len(&self) -> Result<usize> {
        // Use database count since Vertex doesn't have a direct count API
        #[cfg(feature = "postgres")]
        { self.database.get_total_chunks_count().await }
        #[cfg(not(feature = "postgres"))]
        { self.database.get_total_chunks_count() }
    }

    async fn health_check(&self) -> Result<bool> {
        self.auth.get_token().await.map(|_| true)
    }

    fn name(&self) -> &str {
        "vertex-vector-search"
    }
}

impl VertexVectorSearch {
    /// Highlight search query in text using <mark> tags
    fn highlight_matches(&self, text: &str, query: &str) -> String {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();
        let mut result = String::with_capacity(text.len() + query.len() * 20);
        let mut last_end = 0;

        for (idx, matched) in text_lower.match_indices(&query_lower) {
            // Validate byte boundaries before slicing
            if !text.is_char_boundary(idx) || !text.is_char_boundary(idx + matched.len()) {
                continue; // Skip matches that cross character boundaries
            }
            if !text.is_char_boundary(last_end) {
                last_end = text.ceil_char_boundary(last_end);
            }

            // Add text before the match
            if last_end <= idx {
                result.push_str(&text[last_end..idx]);
            }
            // Add highlighted match (preserving original case)
            result.push_str("<mark>");
            result.push_str(&text[idx..idx + matched.len()]);
            result.push_str("</mark>");
            last_end = idx + matched.len();
        }

        // Add remaining text
        if last_end < text.len() {
            if !text.is_char_boundary(last_end) {
                last_end = text.ceil_char_boundary(last_end);
            }
            result.push_str(&text[last_end..]);
        }
        result
    }
}

impl VertexVectorSearch {
    /// Create a minimal chunk with just the ID
    /// The caller should look up full chunk data from local store
    fn create_minimal_chunk(&self, chunk_id: Uuid) -> Chunk {
        Chunk {
            id: chunk_id,
            document_id: Uuid::nil(),
            content: String::new(),
            embedding: Vec::new(),
            source: crate::types::ChunkSource {
                filename: String::new(),
                internal_filename: None,
                file_type: crate::types::FileType::Unknown,
                page_number: None,
                page_count: None,
                section_title: None,
                heading_hierarchy: Vec::new(),
                sheet_name: None,
                row_range: None,
                line_start: None,
                line_end: None,
                code_context: None,
            },
            char_start: 0,
            char_end: 0,
            chunk_index: 0,
            metadata: HashMap::new(),
        }
    }

    /// Convert metadata back to Chunk
    fn metadata_to_chunk(&self, metadata: &HashMap<String, serde_json::Value>) -> Result<Chunk> {
        let chunk_id = metadata
            .get("chunk_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::new_v4);

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

        let file_type = metadata
            .get("file_type")
            .map(|v| serde_json::from_value(v.clone()).unwrap_or(crate::types::FileType::Unknown))
            .unwrap_or(crate::types::FileType::Unknown);

        let source = crate::types::ChunkSource {
            filename,
            internal_filename: None,
            file_type,
            page_number,
            page_count: None,
            section_title,
            heading_hierarchy: Vec::new(),
            sheet_name: None,
            row_range: None,
            line_start: None,
            line_end: None,
            code_context: None,
        };

        Ok(Chunk {
            id: chunk_id,
            document_id,
            content,
            embedding: Vec::new(),
            source,
            char_start,
            char_end,
            chunk_index,
            metadata: HashMap::new(),
        })
    }
}
