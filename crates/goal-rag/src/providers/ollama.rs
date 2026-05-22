//! Ollama-based providers for embeddings and LLM
//!
//! Wraps the existing OllamaClient to implement the provider traits.

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use std::sync::Arc;

use crate::config::LlmConfig;
use crate::error::Result;
use crate::generation::OllamaClient;
use crate::types::response::Citation;

use super::embedding::EmbeddingProvider;
use super::llm::LlmProvider;

/// Ollama embedding provider using nomic-embed-text or similar models
pub struct OllamaEmbedder {
    client: Arc<OllamaClient>,
    dimensions: usize,
    #[allow(dead_code)]
    model: String,
}

impl OllamaEmbedder {
    /// Create a new Ollama embedder
    pub fn new(config: &LlmConfig, dimensions: usize) -> Self {
        Self {
            client: Arc::new(OllamaClient::new(config)),
            dimensions,
            model: config.embed_model.clone(),
        }
    }

    /// Create from existing OllamaClient
    pub fn from_client(client: Arc<OllamaClient>, dimensions: usize, model: String) -> Self {
        Self {
            client,
            dimensions,
            model,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.client.embed(text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let results: Vec<Result<Vec<f32>>> = stream::iter(texts.iter().cloned())
            .map(|text| {
                let client = Arc::clone(&self.client);
                async move { client.embed(&text).await }
            })
            .buffer_unordered(8)
            .collect()
            .await;
        results.into_iter().collect()
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    async fn health_check(&self) -> Result<bool> {
        self.client.health_check().await
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

/// Ollama LLM provider for answer generation
pub struct OllamaLlm {
    client: Arc<OllamaClient>,
    model: String,
}

impl OllamaLlm {
    /// Create a new Ollama LLM provider
    pub fn new(config: &LlmConfig) -> Self {
        Self {
            client: Arc::new(OllamaClient::new(config)),
            model: config.generate_model.clone(),
        }
    }

    /// Create from existing OllamaClient
    pub fn from_client(client: Arc<OllamaClient>, model: String) -> Self {
        Self { client, model }
    }

    /// Get the underlying client for streaming support
    pub fn client(&self) -> &Arc<OllamaClient> {
        &self.client
    }
}

#[async_trait]
impl LlmProvider for OllamaLlm {
    async fn generate_answer(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<String> {
        self.client.generate_answer(question, context, citations).await
    }

    async fn generate_with_learning(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
        past_qa: &[(String, String)],
    ) -> Result<String> {
        self.client
            .generate_with_learning(question, context, citations, past_qa)
            .await
    }

    async fn health_check(&self) -> Result<bool> {
        self.client.health_check().await
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// Combined Ollama provider that shares a single client for both embeddings and LLM
pub struct OllamaProvider {
    embedder: OllamaEmbedder,
    llm: OllamaLlm,
}

impl OllamaProvider {
    /// Create a new combined Ollama provider
    pub fn new(config: &LlmConfig, dimensions: usize) -> Self {
        let client = Arc::new(OllamaClient::new(config));
        Self {
            embedder: OllamaEmbedder::from_client(
                Arc::clone(&client),
                dimensions,
                config.embed_model.clone(),
            ),
            llm: OllamaLlm::from_client(client, config.generate_model.clone()),
        }
    }

    /// Get the embedding provider
    pub fn embedder(&self) -> &OllamaEmbedder {
        &self.embedder
    }

    /// Get the LLM provider
    pub fn llm(&self) -> &OllamaLlm {
        &self.llm
    }

    /// Split into separate providers
    pub fn split(self) -> (OllamaEmbedder, OllamaLlm) {
        (self.embedder, self.llm)
    }
}
