//! OpenAI provider for LLM generation and embeddings
//!
//! Supports OpenAI API, Azure OpenAI, and any compatible API (LocalAI, vLLM, etc.)
//! Configure via environment variables:
//!   OPENAI_API_KEY — required
//!   OPENAI_MODEL — default: gpt-4o
//!   OPENAI_BASE_URL — default: https://api.openai.com/v1
//!   OPENAI_EMBED_MODEL — default: text-embedding-3-small
//!   OPENAI_EMBED_DIMENSIONS — default: 768

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::types::response::Citation;

use super::embedding::EmbeddingProvider;
use super::llm::LlmProvider;

// ─────────────────────────── request / response types ───────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: EmbedInput<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum EmbedInput<'a> {
    Single(&'a str),
    Batch(&'a [String]),
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

// ─────────────────────────────── system prompt ──────────────────────────────

const SYSTEM_PROMPT: &str = r#"You are an assistant that answers questions ONLY using the provided document excerpts below. You must NEVER use your own knowledge or training data.

RULES:
1. Use ONLY facts from the CONTEXT below. Do NOT add information from your own knowledge or training data.
2. If the CONTEXT contains ANY information related to the question — even partial or indirect — present what the documents say. Use quotation marks when quoting exact text from the documents.
3. If the question asks for a specific detail (e.g., a number, size, or threshold) and the CONTEXT does not contain that exact detail, say what the documents DO cover about the topic, then note: "The specific [detail] is not stated in the available documents."
4. Only say "no information available" when the CONTEXT is completely unrelated to the question topic.
5. Do NOT generate document names, filenames, page numbers, or [Source: ...] references — these are handled separately.
6. Do NOT add a "Sources used" section at the end.

RESPONSE FORMAT:
- Use bullet points or numbered lists for requirements, definitions, or multiple items
- Quote directly from documents using quotation marks where appropriate
- Be thorough — include all relevant details from the provided context"#;

// ────────────────────────────── helpers ─────────────────────────────────────

/// Format context + citations into a single user message for the LLM.
fn format_user_message(question: &str, context: &str, citations: &[Citation]) -> String {
    let mut msg = String::with_capacity(context.len() + 512);
    msg.push_str("CONTEXT FROM DOCUMENTS:\n");

    for (i, cite) in citations.iter().enumerate() {
        msg.push_str(&format!(
            "\n[{}] {} (score: {:.2})\n{}\n---\n",
            i + 1,
            cite.filename,
            cite.similarity_score,
            cite.snippet,
        ));
    }

    // If there is additional context not captured by citations, include it
    if citations.is_empty() && !context.is_empty() {
        msg.push_str(context);
        msg.push_str("\n---\n");
    }

    msg.push_str(&format!(
        "\nQUESTION: {}\n\nAnswer using ONLY the document content above. Do NOT add [Source: ...] citations — they are handled separately:",
        question
    ));
    msg
}

// ══════════════════════════════ OpenAiLlm ═══════════════════════════════════

/// OpenAI-compatible LLM provider
pub struct OpenAiLlm {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiLlm {
    /// Create from environment variables.
    ///
    /// - `OPENAI_API_KEY` — **required**
    /// - `OPENAI_MODEL` — default `gpt-4o`
    /// - `OPENAI_BASE_URL` — default `https://api.openai.com/v1`
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
            Error::Config("OPENAI_API_KEY environment variable is required".into())
        })?;
        let model =
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".into());
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into());

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to build HTTP client: {e}")))?;

        tracing::info!(
            "OpenAI LLM provider initialized (model: {}, base_url: {})",
            model,
            base_url
        );

        Ok(Self {
            http,
            api_key,
            base_url,
            model,
        })
    }

    /// Send a chat completion request and return the assistant message content.
    async fn chat_completion(&self, messages: Vec<ChatMessage>) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = ChatRequest {
            model: &self.model,
            messages,
            temperature: 0.3,
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Llm(format!("OpenAI request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Llm(format!(
                "OpenAI API returned {status}: {text}"
            )));
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .map_err(|e| Error::Llm(format!("Failed to parse OpenAI response: {e}")))?;

        chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| Error::Llm("OpenAI returned empty choices".into()))
    }
}

#[async_trait]
impl LlmProvider for OpenAiLlm {
    async fn generate_answer(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<String> {
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: SYSTEM_PROMPT.into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format_user_message(question, context, citations),
            },
        ];

        self.chat_completion(messages).await
    }

    async fn generate_with_learning(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
        past_qa: &[(String, String)],
    ) -> Result<String> {
        let mut messages = vec![ChatMessage {
            role: "system".into(),
            content: SYSTEM_PROMPT.into(),
        }];

        // Inject past Q&A as conversation history so the model can learn
        // the expected answering style.
        for (q, a) in past_qa {
            messages.push(ChatMessage {
                role: "user".into(),
                content: q.clone(),
            });
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: a.clone(),
            });
        }

        messages.push(ChatMessage {
            role: "user".into(),
            content: format_user_message(question, context, citations),
        });

        self.chat_completion(messages).await
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/models", self.base_url);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

// ═══════════════════════════ OpenAiEmbedder ═════════════════════════════════

/// OpenAI-compatible embedding provider
pub struct OpenAiEmbedder {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    dims: usize,
}

impl OpenAiEmbedder {
    /// Create from environment variables.
    ///
    /// - `OPENAI_API_KEY` — **required**
    /// - `OPENAI_EMBED_MODEL` — default `text-embedding-3-small`
    /// - `OPENAI_EMBED_DIMENSIONS` — default `768`
    /// - `OPENAI_BASE_URL` — default `https://api.openai.com/v1`
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
            Error::Config("OPENAI_API_KEY environment variable is required".into())
        })?;
        let model = std::env::var("OPENAI_EMBED_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".into());
        let dims: usize = std::env::var("OPENAI_EMBED_DIMENSIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(768);
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into());

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to build HTTP client: {e}")))?;

        tracing::info!(
            "OpenAI embedding provider initialized (model: {}, dimensions: {}, base_url: {})",
            model,
            dims,
            base_url
        );

        Ok(Self {
            http,
            api_key,
            base_url,
            model,
            dims,
        })
    }

    /// Internal helper to call the embeddings endpoint.
    async fn call_embed(&self, input: EmbedInput<'_>) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.base_url);

        let body = EmbedRequest {
            model: &self.model,
            input,
            dimensions: Some(self.dims),
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Embedding(format!("OpenAI embedding request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Embedding(format!(
                "OpenAI embeddings API returned {status}: {text}"
            )));
        }

        let embed_resp: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| Error::Embedding(format!("Failed to parse OpenAI embedding response: {e}")))?;

        Ok(embed_resp.data.into_iter().map(|d| d.embedding).collect())
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut results = self.call_embed(EmbedInput::Single(text)).await?;
        results
            .pop()
            .ok_or_else(|| Error::Embedding("OpenAI returned empty embedding data".into()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.call_embed(EmbedInput::Batch(texts)).await
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/models", self.base_url);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
    }

    fn name(&self) -> &str {
        "openai"
    }
}
