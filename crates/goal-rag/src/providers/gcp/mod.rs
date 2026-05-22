//! Google Cloud Platform provider implementations
//!
//! Provides high-performance RAG using:
//! - Vertex AI text-embedding-005 for embeddings
//! - Gemini 2.5 Pro for answer generation
//! - Vertex AI Vector Search for similarity search
//! - Google Cloud Storage for document storage
//! - Document AI for advanced PDF text extraction

mod auth;
mod document_ai;
mod gemini_client;
mod gcs_store;
mod vertex_embedder;
mod vertex_vector;

pub use auth::GcpAuth;
pub use document_ai::{DocumentAiClient, DocumentAiPage, DocumentAiResult};
pub use gemini_client::GeminiClient;
pub use gcs_store::{DocumentWithInfo, FileMetadataByName, GcsDocumentStore, GcsFileInfo, GcsMigrationResult};
pub use vertex_embedder::VertexAiEmbedder;
pub use vertex_vector::VertexVectorSearch;
