//! Ollama LLM client for answer generation with retry logic

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

use crate::config::LlmConfig;
use crate::error::{Error, Result};
use crate::types::response::Citation;

use super::prompt::PromptBuilder;

/// Ollama API client with automatic retry
pub struct OllamaClient {
    /// HTTP client
    client: Client,
    /// Configuration
    config: LlmConfig,
    /// Maximum retries
    max_retries: u32,
}

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
    /// Maximum number of tokens to generate (prevents truncated responses)
    num_predict: u32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

impl OllamaClient {
    /// Create a new Ollama client with retry support
    pub fn new(config: &LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .pool_max_idle_per_host(5)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            max_retries: config.max_retries,
            config: config.clone(),
        }
    }

    /// Retry a request with exponential backoff
    async fn retry_request<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.max_retries {
                        let delay = Duration::from_secs(2u64.pow(attempt));
                        tracing::warn!(
                            "Request failed (attempt {}/{}), retrying in {:?}",
                            attempt + 1,
                            self.max_retries + 1,
                            delay
                        );
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Llm("Unknown error".to_string())))
    }

    /// Check if Ollama is available
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.config.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Generate an embedding using Ollama with retry
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.config.base_url);
        let text = text.to_string();
        let model = self.config.embed_model.clone();
        let client = self.client.clone();

        self.retry_request(|| {
            let url = url.clone();
            let text = text.clone();
            let model = model.clone();
            let client = client.clone();

            async move {
                let request = EmbedRequest {
                    model,
                    prompt: text,
                };

                let response = client
                    .post(&url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| Error::Llm(format!("Embedding request failed: {}", e)))?;

                if !response.status().is_success() {
                    return Err(Error::Llm(format!(
                        "Embedding failed: HTTP {}",
                        response.status()
                    )));
                }

                let embed_response: EmbedResponse = response
                    .json()
                    .await
                    .map_err(|e| Error::Llm(format!("Failed to parse embedding response: {}", e)))?;

                Ok(embed_response.embedding)
            }
        }).await
    }

    /// Generate an answer with citations and retry logic
    pub async fn generate_answer(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<String> {
        let url = format!("{}/api/generate", self.config.base_url);
        let prompt = PromptBuilder::build_rag_prompt(question, context, citations);
        let model = self.config.generate_model.clone();
        let temperature = self.config.temperature;
        let context_size = self.config.context_size as u32;
        let client = self.client.clone();

        tracing::info!("Generating answer with model: {} (max tokens: {})", model, context_size);

        self.retry_request(|| {
            let url = url.clone();
            let prompt = prompt.clone();
            let model = model.clone();
            let client = client.clone();

            async move {
                let request = GenerateRequest {
                    model,
                    prompt,
                    stream: false,
                    options: GenerateOptions {
                        temperature,
                        num_predict: context_size,
                    },
                };

                let response = client
                    .post(&url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| Error::Llm(format!("Generation request failed: {}", e)))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(Error::Llm(format!(
                        "Generation failed: HTTP {} - {}",
                        status, body
                    )));
                }

                let generate_response: GenerateResponse = response
                    .json()
                    .await
                    .map_err(|e| Error::Llm(format!("Failed to parse generation response: {}", e)))?;

                Ok(generate_response.response)
            }
        }).await
    }

    /// Generate answer with learned context from past Q&A
    pub async fn generate_with_learning(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
        past_qa: &[(String, String)],  // (question, answer) pairs from learning
    ) -> Result<String> {
        let url = format!("{}/api/generate", self.config.base_url);
        let prompt = PromptBuilder::build_rag_prompt_with_learning(question, context, citations, past_qa);
        let model = self.config.generate_model.clone();
        let temperature = self.config.temperature;
        let context_size = self.config.context_size as u32;
        let client = self.client.clone();

        tracing::info!("Generating answer with {} past Q&A examples (max tokens: {})", past_qa.len(), context_size);

        self.retry_request(|| {
            let url = url.clone();
            let prompt = prompt.clone();
            let model = model.clone();
            let client = client.clone();

            async move {
                let request = GenerateRequest {
                    model,
                    prompt,
                    stream: false,
                    options: GenerateOptions {
                        temperature,
                        num_predict: context_size,
                    },
                };

                let response = client
                    .post(&url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| Error::Llm(format!("Generation request failed: {}", e)))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(Error::Llm(format!(
                        "Generation failed: HTTP {} - {}",
                        status, body
                    )));
                }

                let generate_response: GenerateResponse = response
                    .json()
                    .await
                    .map_err(|e| Error::Llm(format!("Failed to parse generation response: {}", e)))?;

                Ok(generate_response.response)
            }
        }).await
    }

    /// Generate a streaming response (returns chunks)
    pub async fn generate_stream(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<impl futures_util::Stream<Item = Result<String>>> {
        use futures_util::StreamExt;

        let url = format!("{}/api/generate", self.config.base_url);
        let prompt = PromptBuilder::build_rag_prompt(question, context, citations);

        #[derive(Serialize)]
        struct StreamRequest {
            model: String,
            prompt: String,
            stream: bool,
        }

        let request = StreamRequest {
            model: self.config.generate_model.clone(),
            prompt,
            stream: true,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Llm(format!("Stream request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Llm(format!(
                "Stream failed: HTTP {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct StreamChunk {
            response: String,
            #[allow(dead_code)]
            done: bool,
        }

        let stream = response.bytes_stream().map(move |chunk| {
            let bytes = chunk.map_err(|e| Error::Llm(format!("Stream error: {}", e)))?;
            let text = String::from_utf8_lossy(&bytes);

            // Parse NDJSON
            let mut output = String::new();
            for line in text.lines() {
                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(line) {
                    output.push_str(&chunk.response);
                }
            }

            Ok(output)
        });

        Ok(stream)
    }
}
