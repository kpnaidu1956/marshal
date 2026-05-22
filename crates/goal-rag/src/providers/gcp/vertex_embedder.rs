//! Vertex AI embedding provider using text-embedding-005
//!
//! Provides fast, high-quality embeddings with 768 dimensions.
//! Includes retry logic with exponential backoff for rate limiting.
//! Uses a global rate limiter to prevent overwhelming the API.

use async_trait::async_trait;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;

use super::auth::GcpAuth;
use crate::error::{Error, Result};
use crate::providers::embedding::EmbeddingProvider;

/// Maximum number of retries for rate-limited requests
const MAX_RETRIES: u32 = 5;
/// Initial backoff delay in milliseconds
const INITIAL_BACKOFF_MS: u64 = 2000;
/// Maximum concurrent embedding requests (global limit)
const MAX_CONCURRENT_REQUESTS: usize = 2;

/// Global semaphore to limit concurrent Vertex AI requests
static RATE_LIMITER: OnceLock<Semaphore> = OnceLock::new();

fn get_rate_limiter() -> &'static Semaphore {
    RATE_LIMITER.get_or_init(|| Semaphore::new(MAX_CONCURRENT_REQUESTS))
}

/// Generate a simple jitter value based on current time
fn jitter_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 1000) as u64
}

/// Vertex AI embedding provider
pub struct VertexAiEmbedder {
    auth: Arc<GcpAuth>,
    model: String,
    location: String,
    dimensions: usize,
}

impl VertexAiEmbedder {
    /// Create a new Vertex AI embedder
    ///
    /// # Arguments
    /// * `auth` - GCP authentication
    /// * `location` - GCP region (e.g., "us-central1")
    /// * `model` - Model name (default: "text-embedding-005")
    pub fn new(auth: Arc<GcpAuth>, location: String, model: Option<String>) -> Self {
        Self {
            auth,
            model: model.unwrap_or_else(|| "text-embedding-005".to_string()),
            location,
            dimensions: 768, // text-embedding-005 produces 768-dim vectors
        }
    }

    /// Get the API endpoint URL
    fn endpoint(&self) -> String {
        format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:predict",
            self.location,
            self.auth.project_id(),
            self.location,
            self.model
        )
    }
}

#[derive(serde::Serialize)]
struct EmbedRequest {
    instances: Vec<EmbedInstance>,
}

#[derive(serde::Serialize)]
struct EmbedInstance {
    content: String,
}

#[derive(serde::Deserialize)]
struct EmbedResponse {
    predictions: Vec<EmbedPrediction>,
}

#[derive(serde::Deserialize)]
struct EmbedPrediction {
    embeddings: EmbeddingValues,
}

#[derive(serde::Deserialize)]
struct EmbeddingValues {
    values: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for VertexAiEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Acquire rate limiter permit (waits if too many concurrent requests)
        let _permit = get_rate_limiter().acquire().await.map_err(|e| {
            Error::Embedding(format!("Rate limiter error: {}", e))
        })?;

        let client = self.auth.authorized_client().await?;

        let request = EmbedRequest {
            instances: vec![EmbedInstance {
                content: text.to_string(),
            }],
        };

        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                // Add jitter to prevent synchronized retries
                let backoff_ms = INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1) + jitter_ms();
                tracing::warn!(
                    "Vertex AI rate limited, retrying in {}ms (attempt {}/{})",
                    backoff_ms, attempt, MAX_RETRIES
                );
                sleep(Duration::from_millis(backoff_ms)).await;
            }

            let response = match client
                .post(self.endpoint())
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_error = Some(format!("Vertex AI request failed: {}", e));
                    continue;
                }
            };

            let status = response.status();

            // Retry on 429 (Too Many Requests) or 503 (Service Unavailable)
            if status.as_u16() == 429 || status.as_u16() == 503 {
                let body = response.text().await.unwrap_or_default();
                last_error = Some(format!("Vertex AI rate limited ({}): {}", status, body));
                continue;
            }

            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(Error::Embedding(format!(
                    "Vertex AI embedding failed ({}): {}",
                    status, body
                )));
            }

            let embed_response: EmbedResponse = response
                .json()
                .await
                .map_err(|e| Error::Embedding(format!("Failed to parse Vertex AI response: {}", e)))?;

            return embed_response
                .predictions
                .into_iter()
                .next()
                .map(|p| p.embeddings.values)
                .ok_or_else(|| Error::Embedding("No embedding in response".to_string()));
        }

        Err(Error::Embedding(last_error.unwrap_or_else(|| "Max retries exceeded".to_string())))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let client = self.auth.authorized_client().await?;

        // Use smaller batches (20) to reduce rate limiting
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(20) {
            // Acquire rate limiter permit for each batch
            let _permit = get_rate_limiter().acquire().await.map_err(|e| {
                Error::Embedding(format!("Rate limiter error: {}", e))
            })?;

            let request = EmbedRequest {
                instances: chunk
                    .iter()
                    .map(|t| EmbedInstance {
                        content: t.clone(),
                    })
                    .collect(),
            };

            let mut last_error = None;

            for attempt in 0..=MAX_RETRIES {
                if attempt > 0 {
                    // Add jitter to prevent synchronized retries
                    let backoff_ms = INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1) + jitter_ms();
                    tracing::warn!(
                        "Vertex AI batch rate limited, retrying in {}ms (attempt {}/{}, batch size: {})",
                        backoff_ms, attempt, MAX_RETRIES, chunk.len()
                    );
                    sleep(Duration::from_millis(backoff_ms)).await;
                }

                let response = match client
                    .post(self.endpoint())
                    .json(&request)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        last_error = Some(format!("Vertex AI batch request failed: {}", e));
                        continue;
                    }
                };

                let status = response.status();

                // Retry on 429 (Too Many Requests) or 503 (Service Unavailable)
                if status.as_u16() == 429 || status.as_u16() == 503 {
                    let body = response.text().await.unwrap_or_default();
                    last_error = Some(format!("Vertex AI batch rate limited ({}): {}", status, body));
                    continue;
                }

                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    return Err(Error::Embedding(format!(
                        "Vertex AI batch embedding failed ({}): {}",
                        status, body
                    )));
                }

                let embed_response: EmbedResponse = response.json().await.map_err(|e| {
                    Error::Embedding(format!("Failed to parse Vertex AI batch response: {}", e))
                })?;

                all_embeddings.extend(
                    embed_response
                        .predictions
                        .into_iter()
                        .map(|p| p.embeddings.values),
                );

                // Success - break out of retry loop
                last_error = None;
                break;
            }

            // If we exhausted retries, return error
            if let Some(error) = last_error {
                return Err(Error::Embedding(error));
            }

            // Delay between batches to avoid rate limiting (500ms)
            sleep(Duration::from_millis(500)).await;
        }

        Ok(all_embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    async fn health_check(&self) -> Result<bool> {
        // Try to get a token - if auth works, we're healthy
        self.auth.get_token().await.map(|_| true)
    }

    fn name(&self) -> &str {
        "vertex-ai"
    }
}
