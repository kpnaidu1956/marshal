//! Anthropic Claude provider for LLM generation
//!
//! Anthropic does not provide an embeddings API, so only `LlmProvider` is
//! implemented here. Pair with an `EmbeddingProvider` from another backend
//! (OpenAI, Ollama, Vertex AI, etc.).
//!
//! Configure via environment variables:
//!   ANTHROPIC_API_KEY — required
//!   ANTHROPIC_MODEL — default: claude-sonnet-4-20250514
//!   ANTHROPIC_MAX_TOKENS — default: 4096

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::types::response::Citation;

use super::llm::LlmProvider;

// ─────────────────────────── request / response types ───────────────────────

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: usize,
    system: &'a str,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize, Clone)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

// ─────────────────────────────── constants ──────────────────────────────────

const API_BASE: &str = "https://api.anthropic.com/v1";
const API_VERSION: &str = "2023-06-01";

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

// ═══════════════════════════ AnthropicLlm ═══════════════════════════════════

/// Anthropic Claude LLM provider
pub struct AnthropicLlm {
    http: reqwest::Client,
    api_key: String,
    model: String,
    max_tokens: usize,
}

impl AnthropicLlm {
    /// Create from environment variables.
    ///
    /// - `ANTHROPIC_API_KEY` — **required**
    /// - `ANTHROPIC_MODEL` — default `claude-sonnet-4-20250514`
    /// - `ANTHROPIC_MAX_TOKENS` — default `4096`
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            Error::Config("ANTHROPIC_API_KEY environment variable is required".into())
        })?;
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-20250514".into());
        let max_tokens: usize = std::env::var("ANTHROPIC_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096);

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to build HTTP client: {e}")))?;

        tracing::info!(
            "Anthropic LLM provider initialized (model: {}, max_tokens: {})",
            model,
            max_tokens
        );

        Ok(Self {
            http,
            api_key,
            model,
            max_tokens,
        })
    }

    /// Send a Messages API request and return the first text block.
    async fn messages(&self, system: &str, messages: Vec<AnthropicMessage>) -> Result<String> {
        let url = format!("{}/messages", API_BASE);

        let body = MessagesRequest {
            model: &self.model,
            max_tokens: self.max_tokens,
            system,
            messages,
        };

        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Llm(format!("Anthropic request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Llm(format!(
                "Anthropic API returned {status}: {text}"
            )));
        }

        let msg_resp: MessagesResponse = resp
            .json()
            .await
            .map_err(|e| Error::Llm(format!("Failed to parse Anthropic response: {e}")))?;

        msg_resp
            .content
            .into_iter()
            .find_map(|b| b.text)
            .ok_or_else(|| Error::Llm("Anthropic returned no text content".into()))
    }
}

#[async_trait]
impl LlmProvider for AnthropicLlm {
    async fn generate_answer(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
    ) -> Result<String> {
        let messages = vec![AnthropicMessage {
            role: "user".into(),
            content: format_user_message(question, context, citations),
        }];

        self.messages(SYSTEM_PROMPT, messages).await
    }

    async fn generate_with_learning(
        &self,
        question: &str,
        context: &str,
        citations: &[Citation],
        past_qa: &[(String, String)],
    ) -> Result<String> {
        let mut messages = Vec::new();

        // Inject past Q&A as conversation history
        for (q, a) in past_qa {
            messages.push(AnthropicMessage {
                role: "user".into(),
                content: q.clone(),
            });
            messages.push(AnthropicMessage {
                role: "assistant".into(),
                content: a.clone(),
            });
        }

        messages.push(AnthropicMessage {
            role: "user".into(),
            content: format_user_message(question, context, citations),
        });

        self.messages(SYSTEM_PROMPT, messages).await
    }

    async fn health_check(&self) -> Result<bool> {
        // Anthropic has no lightweight /models endpoint, so we send a tiny
        // completion request with 1 max token.
        let url = format!("{}/messages", API_BASE);

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "ping"}]
        });

        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }
}
