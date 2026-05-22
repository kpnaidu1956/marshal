//! Configuration for the RAG system

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ingestion::ExternalParserConfig;

/// Main RAG system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct RagConfig {
    /// Backend provider (local or gcp)
    #[serde(default)]
    pub backend: BackendProvider,
    /// Server configuration
    pub server: ServerConfig,
    /// Embedding configuration
    pub embeddings: EmbeddingConfig,
    /// Chunking configuration
    pub chunking: ChunkingConfig,
    /// Ollama/LLM configuration
    pub llm: LlmConfig,
    /// Vector database configuration
    pub vector_db: VectorDbConfig,
    /// External parser configuration
    pub external_parser: ExternalParserConfig,
    /// Processing configuration
    pub processing: ProcessingConfig,
    /// GCP configuration (required when backend = gcp)
    #[serde(default)]
    pub gcp: Option<GcpConfig>,
    /// PostgreSQL configuration (optional - for learning from database changes)
    #[serde(default)]
    #[cfg(feature = "postgres")]
    pub postgres: Option<crate::postgres::PostgresConfig>,
}


/// Processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    /// Timeout for processing a single file in seconds (default: 300 = 5 minutes)
    /// This is the fallback timeout if tiered processing is disabled
    pub file_timeout_secs: u64,
    /// Number of parallel file workers
    pub parallel_files: Option<usize>,
    /// Number of parallel embeddings per file
    pub parallel_embeddings: Option<usize>,
    /// Tiered processing configuration (size-based routing)
    #[serde(default)]
    pub tiered: TieredProcessingConfig,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            file_timeout_secs: 300, // 5 minutes
            parallel_files: None,   // Auto-detect from CPU count
            parallel_embeddings: None,
            tiered: TieredProcessingConfig::default(),
        }
    }
}

/// Tiered processing configuration for size-based file routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredProcessingConfig {
    /// Enable tiered processing (default: true)
    #[serde(default = "default_tiered_enabled")]
    pub enabled: bool,

    // Size thresholds (bytes)
    /// Files smaller than this are "fast" tier (default: 10MB)
    #[serde(default = "default_fast_threshold")]
    pub fast_threshold: u64,
    /// Files smaller than this are "medium" tier (default: 100MB)
    #[serde(default = "default_medium_threshold")]
    pub medium_threshold: u64,
    /// Files smaller than this are "heavy" tier (default: 1GB)
    #[serde(default = "default_heavy_threshold")]
    pub heavy_threshold: u64,

    // Timeouts (seconds)
    /// Timeout for fast tier files (default: 120s = 2 minutes)
    #[serde(default = "default_fast_timeout")]
    pub fast_timeout_secs: u64,
    /// Timeout for medium tier files (default: 300s = 5 minutes)
    #[serde(default = "default_medium_timeout")]
    pub medium_timeout_secs: u64,
    /// Timeout for heavy tier files (default: 900s = 15 minutes)
    #[serde(default = "default_heavy_timeout")]
    pub heavy_timeout_secs: u64,
    /// Timeout for complex tier files (default: 1200s = 20 minutes)
    #[serde(default = "default_complex_timeout")]
    pub complex_timeout_secs: u64,

    // Worker counts
    /// Workers for fast tier (default: CPU count, max 8)
    pub fast_workers: Option<usize>,
    /// Workers for medium tier (default: 4)
    pub medium_workers: Option<usize>,
    /// Workers for heavy tier (default: 2)
    pub heavy_workers: Option<usize>,

    // Parser preferences
    /// Prefer cloud services for scanned PDFs (default: true)
    #[serde(default = "default_prefer_cloud_scanned")]
    pub prefer_cloud_for_scanned: bool,
    /// Prefer cloud services for encrypted PDFs (default: true)
    #[serde(default = "default_prefer_cloud_encrypted")]
    pub prefer_cloud_for_encrypted: bool,
    /// Enable parallel parsing attempts for complex files (default: false)
    #[serde(default)]
    pub enable_parallel_parsing: bool,
}

fn default_tiered_enabled() -> bool { true }
fn default_fast_threshold() -> u64 { 10 * 1024 * 1024 }      // 10MB
fn default_medium_threshold() -> u64 { 100 * 1024 * 1024 }   // 100MB
fn default_heavy_threshold() -> u64 { 1024 * 1024 * 1024 }   // 1GB
fn default_fast_timeout() -> u64 { 120 }
fn default_medium_timeout() -> u64 { 300 }
fn default_heavy_timeout() -> u64 { 900 }
fn default_complex_timeout() -> u64 { 1200 }
fn default_prefer_cloud_scanned() -> bool { true }
fn default_prefer_cloud_encrypted() -> bool { true }

impl Default for TieredProcessingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fast_threshold: 10 * 1024 * 1024,      // 10MB
            medium_threshold: 100 * 1024 * 1024,   // 100MB
            heavy_threshold: 1024 * 1024 * 1024,   // 1GB
            fast_timeout_secs: 120,
            medium_timeout_secs: 300,
            heavy_timeout_secs: 900,
            complex_timeout_secs: 1200,
            fast_workers: None,
            medium_workers: Some(4),
            heavy_workers: Some(2),
            prefer_cloud_for_scanned: true,
            prefer_cloud_for_encrypted: true,
            enable_parallel_parsing: false,
        }
    }
}

impl TieredProcessingConfig {
    /// Get timeout for a given tier
    pub fn timeout_for_tier(&self, tier: &crate::processing::FileTier) -> std::time::Duration {
        use crate::processing::FileTier;
        let secs = match tier {
            FileTier::Fast => self.fast_timeout_secs,
            FileTier::Medium => self.medium_timeout_secs,
            FileTier::Heavy => self.heavy_timeout_secs,
            FileTier::Complex => self.complex_timeout_secs,
        };
        std::time::Duration::from_secs(secs)
    }

    /// Classify file into tier based on size
    pub fn tier_for_size(&self, size_bytes: u64) -> crate::processing::FileTier {
        use crate::processing::FileTier;
        if size_bytes < self.fast_threshold {
            FileTier::Fast
        } else if size_bytes < self.medium_threshold {
            FileTier::Medium
        } else {
            FileTier::Heavy
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Host address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Enable CORS
    pub enable_cors: bool,
    /// Maximum upload size in bytes (default: 100MB)
    pub max_upload_size: usize,
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            enable_cors: true,
            max_upload_size: 100 * 1024 * 1024, // 100MB
            rate_limit: RateLimitConfig::default(),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting (default: true)
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
    /// Maximum requests per second for query endpoints (default: 10)
    #[serde(default = "default_query_rps")]
    pub query_requests_per_second: u32,
    /// Maximum requests per second for upload endpoints (default: 5)
    #[serde(default = "default_upload_rps")]
    pub upload_requests_per_second: u32,
    /// Maximum concurrent uploads (default: 3)
    #[serde(default = "default_max_concurrent_uploads")]
    pub max_concurrent_uploads: usize,
    /// Maximum concurrent GCS operations (default: 5)
    #[serde(default = "default_max_concurrent_gcs")]
    pub max_concurrent_gcs_operations: usize,
    /// Circuit breaker: max consecutive failures before tripping (default: 5)
    #[serde(default = "default_circuit_breaker_threshold")]
    pub circuit_breaker_threshold: usize,
    /// Circuit breaker: reset time in seconds (default: 30)
    #[serde(default = "default_circuit_breaker_reset")]
    pub circuit_breaker_reset_secs: u64,
    /// Maximum queue depth before rejecting new jobs (default: 100)
    #[serde(default = "default_max_queue_depth")]
    pub max_queue_depth: usize,
}

fn default_rate_limit_enabled() -> bool { true }
fn default_query_rps() -> u32 { 10 }
fn default_upload_rps() -> u32 { 5 }
fn default_max_concurrent_uploads() -> usize { 3 }
fn default_max_concurrent_gcs() -> usize { 5 }
fn default_circuit_breaker_threshold() -> usize { 5 }
fn default_circuit_breaker_reset() -> u64 { 30 }
fn default_max_queue_depth() -> usize { 100 }

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            query_requests_per_second: 10,
            upload_requests_per_second: 5,
            max_concurrent_uploads: 3,
            max_concurrent_gcs_operations: 5,
            circuit_breaker_threshold: 5,
            circuit_breaker_reset_secs: 30,
            max_queue_depth: 100,
        }
    }
}

/// Embedding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Model to use (default: all-MiniLM-L6-v2)
    pub model: String,
    /// Embedding dimensions (384 for MiniLM, 768 for larger models)
    pub dimensions: usize,
    /// Batch size for embedding generation
    pub batch_size: usize,
    /// Maximum sequence length
    pub max_length: usize,
    /// Cache directory for models
    pub cache_dir: PathBuf,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: "nomic-embed-text".to_string(),
            dimensions: 768,
            batch_size: 32,
            max_length: 256,
            cache_dir: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ruvector-rag")
                .join("models"),
        }
    }
}

/// Text chunking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    /// Target chunk size in characters
    pub chunk_size: usize,
    /// Overlap between chunks in characters
    pub chunk_overlap: usize,
    /// Minimum chunk size (skip smaller chunks)
    pub min_chunk_size: usize,
    /// Respect sentence boundaries
    pub respect_sentences: bool,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1024,      // Larger chunks = more context
            chunk_overlap: 200,    // More overlap = better continuity
            min_chunk_size: 100,
            respect_sentences: true,
        }
    }
}

/// LLM (Ollama) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// LLM provider: "ollama" (default), "openai", "anthropic"
    #[serde(default = "default_llm_provider")]
    pub provider: String,
    /// Ollama base URL
    pub base_url: String,
    /// Embedding model name
    pub embed_model: String,
    /// Generation model name
    pub generate_model: String,
    /// Temperature for generation
    pub temperature: f32,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Number of retries for failed requests
    pub max_retries: u32,
    /// Context window size (tokens)
    pub context_size: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_llm_provider(),
            base_url: "http://localhost:11434".to_string(),
            embed_model: "nomic-embed-text".to_string(),
            generate_model: "phi3".to_string(),  // Fast 3.8B model for CPU
            temperature: 0.3,  // Lower for more factual answers
            timeout_secs: 120,  // 2 minutes for phi3
            max_retries: 2,
            context_size: 4096,  // phi3 context size
        }
    }
}

/// Vector database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDbConfig {
    /// Storage path for the vector database
    pub storage_path: PathBuf,
    /// HNSW M parameter (connections per layer)
    pub hnsw_m: usize,
    /// HNSW ef_construction parameter
    pub hnsw_ef_construction: usize,
    /// HNSW ef_search parameter
    pub hnsw_ef_search: usize,
}

impl Default for VectorDbConfig {
    fn default() -> Self {
        // Use absolute path to avoid path traversal detection
        let storage_path = dirs::data_local_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
            .join("ruvector-rag")
            .join("vectors.db");

        Self {
            storage_path,
            hnsw_m: 32,
            hnsw_ef_construction: 200,
            hnsw_ef_search: 100,
        }
    }
}

/// Backend provider selection
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BackendProvider {
    /// Local backend (Ollama + HNSW + filesystem)
    #[default]
    Local,
    /// Google Cloud Platform (Vertex AI + GCS)
    Gcp,
}

/// Hybrid mode configuration for GCP backend
/// Controls which components use local vs cloud services
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HybridMode {
    /// Full GCP: Vertex AI embeddings + Vertex AI vectors + Gemini LLM
    #[default]
    FullGcp,
    /// Hybrid Vertex: Ollama embeddings + Vertex AI vectors + Gemini LLM
    /// Saves on embedding API costs but still uses Vertex for vector storage
    HybridVertex,
    /// Hybrid Local: Ollama embeddings + Local HNSW vectors + GCS storage + Gemini LLM
    /// No rate limits during ingestion, only Gemini calls for answers
    /// Best for avoiding Vertex AI rate limits while keeping cloud LLM quality
    HybridLocal,
}

/// Google Cloud Platform configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcpConfig {
    /// Path to service account JSON key file
    pub service_account_key_path: PathBuf,
    /// GCP project ID
    pub project_id: String,
    /// GCP region (e.g., "us-central1")
    pub location: String,
    /// GCS bucket for document storage
    pub gcs_bucket: String,
    /// GCS prefix for original documents (default: "originals/")
    #[serde(default = "default_gcs_originals_prefix")]
    pub gcs_originals_prefix: String,
    /// GCS prefix for extracted plain text (default: "plaintext/")
    #[serde(default = "default_gcs_plaintext_prefix")]
    pub gcs_plaintext_prefix: String,
    /// Vertex AI Vector Search Index (full resource name for upsert operations)
    /// e.g., "projects/my-project/locations/us-central1/indexes/123456"
    pub vector_search_index: String,
    /// Vertex AI Vector Search endpoint (full resource name for query operations)
    /// e.g., "projects/my-project/locations/us-central1/indexEndpoints/123456"
    pub vector_search_endpoint: String,
    /// Public endpoint domain for Vector Search queries (required for public endpoints)
    /// e.g., "399775135.us-central1-YOUR_PROJECT_NUMBER.vdb.vertexai.goog"
    pub vector_search_public_domain: Option<String>,
    /// Deployed index ID within the endpoint
    pub deployed_index_id: String,
    /// Embedding model (default: "text-embedding-005")
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    /// Generation model (default: "gemini-2.5-pro")
    #[serde(default = "default_generation_model")]
    pub generation_model: String,
    /// Document AI processor ID for PDF extraction (optional)
    /// e.g., "projects/my-project/locations/us/processors/abc123"
    /// If not set, Document AI fallback is disabled
    #[serde(default)]
    pub document_ai_processor: Option<String>,
    /// Enable Document AI as fallback for failed PDF parsing (default: true if processor is set)
    #[serde(default = "default_use_document_ai")]
    pub use_document_ai_fallback: bool,
    /// Use local Ollama for embeddings instead of Vertex AI (avoids rate limits)
    /// DEPRECATED: Use hybrid_mode instead. Kept for backward compatibility.
    /// When true: equivalent to hybrid_mode = "hybrid_vertex"
    #[serde(default)]
    pub use_local_embeddings: bool,

    /// Hybrid mode selection (overrides use_local_embeddings if set)
    /// - full_gcp: Vertex AI embeddings + Vertex AI vectors + Gemini LLM
    /// - hybrid_vertex: Ollama embeddings + Vertex AI vectors + Gemini LLM
    /// - hybrid_local: Ollama embeddings + Local HNSW + GCS storage + Gemini LLM
    #[serde(default)]
    pub hybrid_mode: Option<HybridMode>,
}

impl GcpConfig {
    /// Get the effective hybrid mode, considering both new and legacy config options
    pub fn effective_hybrid_mode(&self) -> HybridMode {
        // New hybrid_mode takes precedence
        if let Some(mode) = self.hybrid_mode {
            return mode;
        }
        // Fall back to legacy use_local_embeddings
        if self.use_local_embeddings {
            HybridMode::HybridVertex
        } else {
            HybridMode::FullGcp
        }
    }
}

fn default_embedding_model() -> String {
    "text-embedding-005".to_string()
}

fn default_generation_model() -> String {
    "gemini-2.5-pro".to_string()
}

fn default_gcs_originals_prefix() -> String {
    "originals/".to_string()
}

fn default_gcs_plaintext_prefix() -> String {
    "plaintext/".to_string()
}

fn default_use_document_ai() -> bool {
    true
}

fn default_llm_provider() -> String {
    "ollama".to_string()
}
