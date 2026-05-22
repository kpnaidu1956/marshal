//! Vector store provider trait for storing and searching embeddings

use async_trait::async_trait;
use uuid::Uuid;
use crate::error::Result;
use crate::types::Chunk;
use crate::types::response::StringSearchResult;

/// Search result from vector store
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    /// The matched chunk
    pub chunk: Chunk,
    /// Similarity score (0.0 to 1.0, higher is more similar)
    pub similarity: f32,
}

/// Search filter options for multi-tenancy
#[derive(Debug, Clone)]
pub struct SearchFilter {
    /// Organization ID for tenant isolation (REQUIRED — no cross-org leakage)
    pub organization_id: String,
    /// Filter by specific document IDs
    pub document_ids: Option<Vec<Uuid>>,
}

impl SearchFilter {
    /// Create a new filter with organization ID (required for tenant isolation)
    pub fn new(organization_id: String) -> Self {
        Self {
            organization_id,
            document_ids: None,
        }
    }

    /// Add document filter
    pub fn with_documents(mut self, document_ids: Option<Vec<Uuid>>) -> Self {
        self.document_ids = document_ids;
        self
    }
}

/// Trait for vector storage and similarity search
///
/// Implementations:
/// - `LocalVectorStore`: Local HNSW index (ruvector-core)
/// - `VertexVectorSearch`: Google Vertex AI Vector Search
#[async_trait]
pub trait VectorStoreProvider: Send + Sync {
    /// Insert a chunk with its embedding
    async fn insert_chunk(&self, chunk: &Chunk) -> Result<()>;

    /// Insert multiple chunks (batch)
    async fn insert_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        for chunk in chunks {
            self.insert_chunk(chunk).await?;
        }
        Ok(())
    }

    /// Search for similar chunks by embedding similarity
    /// Organization ID in filter is REQUIRED for tenant isolation
    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        filter: &SearchFilter,
    ) -> Result<Vec<VectorSearchResult>>;

    /// Perform literal string search across all chunks
    /// Organization ID is REQUIRED for tenant isolation
    async fn string_search(
        &self,
        query: &str,
        limit: usize,
        organization_id: &str,
    ) -> Result<Vec<StringSearchResult>>;

    /// Delete all chunks for a document
    async fn delete_by_document(&self, document_id: &Uuid) -> Result<usize>;

    /// Get total number of vectors stored
    async fn len(&self) -> Result<usize>;

    /// Check if store is empty
    async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    /// Check if the provider is healthy
    async fn health_check(&self) -> Result<bool>;

    /// Get provider name for logging
    fn name(&self) -> &str;
}
