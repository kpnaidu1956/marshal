//! Embedding provider trait for generating text embeddings

use async_trait::async_trait;
use crate::error::Result;

/// Trait for generating text embeddings
///
/// Implementations:
/// - `OllamaEmbedder`: Local Ollama server (nomic-embed-text)
/// - `VertexAiEmbedder`: Google Vertex AI (text-embedding-005)
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch)
    ///
    /// Default implementation calls `embed` sequentially.
    /// Implementations should override for better performance.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::with_capacity(texts.len());
        for text in texts {
            embeddings.push(self.embed(text).await?);
        }
        Ok(embeddings)
    }

    /// Get embedding dimensions (e.g., 768 for nomic-embed-text and text-embedding-005)
    fn dimensions(&self) -> usize;

    /// Check if the provider is healthy and available
    async fn health_check(&self) -> Result<bool>;

    /// Get provider name for logging
    fn name(&self) -> &str;
}
