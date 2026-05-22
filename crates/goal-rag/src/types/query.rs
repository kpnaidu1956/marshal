//! Query request types

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of query for routing between RAG and string search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryType {
    /// Full RAG query with semantic search and LLM answer generation
    Question,
    /// Literal string search (word or phrase lookup)
    StringSearch,
}

impl QueryType {
    /// Detect query type from input string
    ///
    /// Heuristics:
    /// - Question if: ends with ?, starts with question words (what/how/why/etc), or 5+ words
    /// - StringSearch otherwise (short phrases, single words)
    pub fn detect(input: &str) -> Self {
        let input = input.trim();

        // Ends with question mark -> Question
        if input.ends_with('?') {
            return Self::Question;
        }

        let lower = input.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        // Question words at start -> Question
        const QUESTION_WORDS: &[&str] = &[
            "what", "how", "why", "when", "where", "who", "which",
            "can", "could", "would", "should", "is", "are", "do", "does",
            "explain", "describe", "tell", "show", "find", "list",
        ];

        if let Some(first_word) = words.first() {
            if QUESTION_WORDS.contains(first_word) {
                return Self::Question;
            }
        }

        // 5+ words -> likely a question or complex query
        if words.len() >= 5 {
            return Self::Question;
        }

        // Short phrase or single word -> string search
        Self::StringSearch
    }
}

/// Query request for RAG search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// The question to answer
    pub question: String,

    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,

    /// Number of chunks to retrieve (default: 5)
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// Minimum similarity threshold (0.0-1.0, default: 0.3)
    #[serde(default = "default_threshold")]
    pub similarity_threshold: f32,

    /// Whether to rerank results (default: true)
    #[serde(default = "default_rerank")]
    pub rerank: bool,

    /// Filter by specific document IDs (optional)
    #[serde(default)]
    pub document_filter: Option<Vec<Uuid>>,

    /// Include raw chunks in response (default: false)
    #[serde(default)]
    pub include_chunks: bool,

    /// Stream the response (default: false)
    #[serde(default)]
    pub stream: bool,
}

fn default_top_k() -> usize {
    15  // More chunks for comprehensive context (GPU can handle larger context)
}

fn default_threshold() -> f32 {
    0.35  // Quality threshold — filters out marginally relevant noise
}

fn default_rerank() -> bool {
    true
}

impl Default for QueryRequest {
    fn default() -> Self {
        Self {
            question: String::new(),
            organization_id: String::new(),  // Must be set by caller
            top_k: 10,  // Focused: fewer chunks = less noise for LLM
            similarity_threshold: 0.35,  // Quality threshold — rejects weak matches
            rerank: true,
            document_filter: None,
            include_chunks: false,
            stream: false,
        }
    }
}

impl QueryRequest {
    /// Create a new query
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            ..Default::default()
        }
    }

    /// Set the number of results to retrieve
    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }

    /// Set the similarity threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    /// Filter by document IDs
    pub fn with_documents(mut self, doc_ids: Vec<Uuid>) -> Self {
        self.document_filter = Some(doc_ids);
        self
    }

    /// Include raw chunks in response
    pub fn with_chunks(mut self) -> Self {
        self.include_chunks = true;
        self
    }
}

/// Ingest request options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestOptions {
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,

    /// Custom chunk size (overrides config)
    pub chunk_size: Option<usize>,

    /// Custom chunk overlap (overrides config)
    pub chunk_overlap: Option<usize>,

    /// Extract images and run OCR
    #[serde(default)]
    pub extract_images: bool,

    /// Custom metadata to attach to documents
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

