//! Cross-encoder reranker using the existing LLM provider
//!
//! After vector retrieval returns over-fetched candidates, the reranker scores
//! each chunk's relevance to the query using the LLM, then re-sorts by that
//! score. This significantly improves precision without adding new dependencies.

use std::sync::Arc;

use async_trait::async_trait;
use futures::{stream, StreamExt};

use crate::error::Result;
use crate::providers::llm::LlmProvider;

/// Trait for reranking retrieved documents against a query.
#[async_trait]
pub trait Reranker: Send + Sync {
    /// Rerank documents by relevance to the query.
    ///
    /// Each tuple is `(chunk_content, original_similarity_score)`.
    /// Returns the same tuples re-sorted by the reranked score.
    async fn rerank(
        &self,
        query: &str,
        documents: &[(String, f32)],
    ) -> Result<Vec<(String, f32)>>;
}

/// LLM-based reranker that asks the model to rate relevance 0-10.
pub struct LlmReranker {
    llm: Arc<dyn LlmProvider>,
    /// Maximum number of concurrent LLM scoring requests.
    concurrency: usize,
}

impl LlmReranker {
    /// Create a new LLM reranker with the given provider and concurrency limit.
    pub fn new(llm: Arc<dyn LlmProvider>, concurrency: usize) -> Self {
        Self { llm, concurrency }
    }
}

/// Score a single chunk independently (free function to avoid lifetime issues).
async fn score_chunk_with_llm(
    llm: &Arc<dyn LlmProvider>,
    query: &str,
    chunk: &str,
) -> Option<f32> {
    // Truncate chunk to ~500 chars to save tokens
    let truncated = if chunk.len() > 500 {
        &chunk[..500]
    } else {
        chunk
    };

    let prompt = format!(
        "Rate the relevance of this passage to the query on a scale of 0-10.\n\
         Query: {}\n\
         Passage: {}\n\
         Relevance score (just the number):",
        query, truncated
    );

    let response = llm
        .generate_answer(
            &prompt,
            "You are a relevance scorer. Reply ONLY with a single integer from 0 to 10.",
            &[],
        )
        .await
        .ok()?;

    let score: f32 = response
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>()
        .parse()
        .ok()?;

    Some((score.clamp(0.0, 10.0)) / 10.0)
}

#[async_trait]
impl Reranker for LlmReranker {
    async fn rerank(
        &self,
        query: &str,
        documents: &[(String, f32)],
    ) -> Result<Vec<(String, f32)>> {
        let query_owned = query.to_string();
        let llm = Arc::clone(&self.llm);
        let concurrency = self.concurrency;

        // Collect into owned vec to avoid lifetime issues with buffer_unordered
        let owned_docs: Vec<(usize, String, f32)> = documents
            .iter()
            .enumerate()
            .map(|(i, (c, s))| (i, c.clone(), *s))
            .collect();

        // Score all chunks concurrently with bounded parallelism
        let scored: Vec<(String, f32)> = stream::iter(owned_docs)
            .map(|(idx, content, original)| {
                let q = query_owned.clone();
                let llm_clone = Arc::clone(&llm);
                async move {
                    match score_chunk_with_llm(&llm_clone, &q, &content).await {
                        Some(reranked_score) => {
                            tracing::debug!(
                                "Reranker: chunk {} scored {:.2} (was {:.2})",
                                idx,
                                reranked_score,
                                original
                            );
                            (content, reranked_score)
                        }
                        None => {
                            tracing::debug!(
                                "Reranker: chunk {} parse failed, keeping original {:.2}",
                                idx,
                                original
                            );
                            (content, original)
                        }
                    }
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        // Sort descending by reranked score
        let mut results = scored;
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }
}
