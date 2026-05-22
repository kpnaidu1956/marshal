//! LLM provider trait for generating answers

use async_trait::async_trait;
use futures::stream::{self, Stream};
use std::pin::Pin;

use crate::error::Result;
use crate::types::response::Citation;

/// Trait for LLM-based answer generation
///
/// Implementations:
/// - `OllamaLlm`: Local Ollama server (phi3, llama2, etc.)
/// - `GeminiClient`: Google Vertex AI (gemini-2.5-pro)
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate an answer given a question, context, and citations
    async fn generate_answer(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<String>;

    /// Generate answer with learning context (past Q&A examples)
    async fn generate_with_learning(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
        past_qa: &[(String, String)],
    ) -> Result<String>;

    /// Generate an answer as a stream of text chunks.
    ///
    /// The default implementation calls `generate_answer` and yields
    /// the full answer as a single chunk. Providers that support native
    /// streaming (e.g. Gemini `streamGenerateContent`) can override this
    /// for true incremental delivery.
    async fn generate_stream(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let answer = self.generate_answer(question, context, citations).await?;
        Ok(Box::pin(stream::once(async { Ok(answer) })))
    }

    /// Check if the provider is healthy and available
    async fn health_check(&self) -> Result<bool>;

    /// Get provider name for logging
    fn name(&self) -> &str;

    /// Get the model being used
    fn model(&self) -> &str;
}
