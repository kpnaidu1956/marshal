//! Provider abstractions for embeddings, LLM, vector storage, and document storage
//!
//! This module provides trait-based abstractions that allow switching between
//! local (Ollama) and cloud (GCP) backends.

pub mod embedding;
pub mod llm;
pub mod vector_store;
pub mod document_store;
pub mod ollama;
pub mod openai;
pub mod anthropic;
pub mod local;
pub mod interaction_classifier;
pub mod reranker;

#[cfg(feature = "gcp")]
pub mod gcp;

#[cfg(feature = "postgres")]
pub mod pgvector;

#[cfg(feature = "postgres")]
pub mod entity_embeddings;

pub use embedding::EmbeddingProvider;
pub use llm::LlmProvider;
pub use vector_store::VectorStoreProvider;
pub use document_store::DocumentStoreProvider;
pub use interaction_classifier::InteractionClassifier;
