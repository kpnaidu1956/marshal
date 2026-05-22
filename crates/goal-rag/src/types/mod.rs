//! Core types for the RAG system

pub mod document;
pub mod file_record;
pub mod query;
pub mod response;

pub use document::{Chunk, ChunkSource, Document, FileType};
pub use file_record::{
    FileCheckItem, FileCheckRequest, FileCheckResponse, FileCheckResult, FileCheckSummary,
    FileRecord, FileRecordParams, FileRecordStatus, FileRecordSummary, FileUploadAdvice, SkipReason,
};
pub use query::QueryRequest;
pub use response::{
    BatchUploadResponse, Citation, FileUploadInfo, FileUploadResponse, QueryResponse, UploadAction,
};
