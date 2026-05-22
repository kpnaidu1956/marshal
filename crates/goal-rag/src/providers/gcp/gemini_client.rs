//! Gemini 2.5 Pro client for answer generation via Vertex AI
//!
//! High-performance LLM for RAG answer generation with citations.

use async_trait::async_trait;
use std::sync::Arc;

use super::auth::GcpAuth;
use crate::error::{Error, Result};
use crate::providers::llm::LlmProvider;
use crate::types::response::Citation;

/// Gemini client via Vertex AI
pub struct GeminiClient {
    auth: Arc<GcpAuth>,
    model: String,
    location: String,
}

impl GeminiClient {
    /// Create a new Gemini client
    ///
    /// # Arguments
    /// * `auth` - GCP authentication
    /// * `location` - GCP region (e.g., "us-central1")
    /// * `model` - Model name (default: "gemini-2.5-pro")
    pub fn new(auth: Arc<GcpAuth>, location: String, model: Option<String>) -> Self {
        Self {
            auth,
            model: model.unwrap_or_else(|| "gemini-2.5-pro".to_string()),
            location,
        }
    }

    /// Get the API endpoint URL
    fn endpoint(&self) -> String {
        format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
            self.location,
            self.auth.project_id(),
            self.location,
            self.model
        )
    }

    /// Build the RAG prompt — delegates to PromptBuilder for single source of truth
    fn build_prompt(&self, question: &str, context: &str, citations: &[Citation]) -> String {
        crate::generation::PromptBuilder::build_rag_prompt(question, context, citations)
    }
}

#[derive(serde::Serialize)]
struct GenerateRequest {
    contents: Vec<Content>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(serde::Serialize)]
struct Content {
    role: String,
    parts: Vec<Part>,
}

#[derive(serde::Serialize)]
struct Part {
    text: String,
}

#[derive(serde::Serialize)]
struct GenerationConfig {
    temperature: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
    #[serde(rename = "topP")]
    top_p: f32,
}

#[derive(serde::Deserialize)]
struct GenerateResponse {
    candidates: Vec<Candidate>,
}

#[derive(serde::Deserialize)]
struct Candidate {
    content: ResponseContent,
}

#[derive(serde::Deserialize)]
struct ResponseContent {
    parts: Vec<ResponsePart>,
}

#[derive(serde::Deserialize)]
struct ResponsePart {
    text: String,
}

#[async_trait]
impl LlmProvider for GeminiClient {
    async fn generate_answer(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<String> {
        let client = self.auth.authorized_client().await?;
        let prompt = self.build_prompt(question, context, citations);

        let request = GenerateRequest {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part { text: prompt }],
            }],
            generation_config: GenerationConfig {
                temperature: 0.0, // Deterministic for consistent, factual responses
                max_output_tokens: 8192, // Increased for longer, complete answers
                top_p: 0.85, // Tighter for more deterministic output
            },
        };

        let response = client
            .post(self.endpoint())
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Llm(format!("Gemini request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Llm(format!(
                "Gemini generation failed ({}): {}",
                status, body
            )));
        }

        let gen_response: GenerateResponse = response
            .json()
            .await
            .map_err(|e| Error::Llm(format!("Failed to parse Gemini response: {}", e)))?;

        gen_response
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| Error::Llm("No text in Gemini response".to_string()))
    }

    async fn generate_with_learning(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
        past_qa: &[(String, String)],
    ) -> Result<String> {
        let client = self.auth.authorized_client().await?;

        // Build multi-turn conversation with learning examples
        let mut contents = Vec::new();

        // Add past Q&A as examples
        for (q, a) in past_qa.iter().take(3) {
            // Limit to 3 examples
            contents.push(Content {
                role: "user".to_string(),
                parts: vec![Part { text: q.clone() }],
            });
            contents.push(Content {
                role: "model".to_string(),
                parts: vec![Part { text: a.clone() }],
            });
        }

        // Add current question
        let prompt = self.build_prompt(question, context, citations);
        contents.push(Content {
            role: "user".to_string(),
            parts: vec![Part { text: prompt }],
        });

        let request = GenerateRequest {
            contents,
            generation_config: GenerationConfig {
                temperature: 0.0, // Deterministic for consistent, factual responses
                max_output_tokens: 8192, // Increased for longer, complete answers
                top_p: 0.85, // Tighter for more deterministic output
            },
        };

        let response = client
            .post(self.endpoint())
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Llm(format!("Gemini request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Llm(format!(
                "Gemini generation with learning failed ({}): {}",
                status, body
            )));
        }

        let gen_response: GenerateResponse = response
            .json()
            .await
            .map_err(|e| Error::Llm(format!("Failed to parse Gemini response: {}", e)))?;

        gen_response
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| Error::Llm("No text in Gemini response".to_string()))
    }

    async fn health_check(&self) -> Result<bool> {
        self.auth.get_token().await.map(|_| true)
    }

    fn name(&self) -> &str {
        "gemini"
    }

    fn model(&self) -> &str {
        &self.model
    }
}
