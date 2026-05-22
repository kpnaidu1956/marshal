//! ruvector-rag: Full-stack RAG system with document ingestion and citation-aware answers
//!
//! This crate provides a complete RAG (Retrieval-Augmented Generation) system built on ruvector-core.
//! It supports multiple file formats, local ONNX embeddings, and LLM-powered answer generation
//! with precise source citations.

pub mod analytics;
pub mod config;
pub mod embeddings;
pub mod error;
pub mod generation;
pub mod ingestion;
pub mod learning;
pub mod processing;
pub mod providers;
pub mod retrieval;
pub mod server;
pub mod storage;
pub mod types;
pub mod validation;

#[cfg(feature = "postgres")]
pub mod postgres;

pub use config::RagConfig;
pub use error::{Error, Result};
pub use types::{
    document::{Chunk, ChunkSource, Document, FileType},
    query::QueryRequest,
    response::{Citation, QueryResponse},
};

/// Re-export ruvector-core for convenience
pub use ruvector_core;
