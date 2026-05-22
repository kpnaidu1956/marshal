//! Storage module for persistent data storage
//!
//! Provides SQLite-based persistence for file registry and documents.
//! With `postgres` feature, provides async PostgreSQL-based persistence via PgFileRegistry.

mod database;

#[cfg(feature = "postgres")]
pub mod pg_registry;

pub use database::{
    FileRegistryDb, FileRegistryDbStats, SyncStatus,
    // Job persistence types
    JobFileRecord, JobFileStatus, JobOptions, JobRecord, PersistedJobStage, PersistedJobStatus,
    // Chunk content types (for FTS)
    ChunkContentRecord, ChunkSearchResult,
};

#[cfg(feature = "postgres")]
pub use pg_registry::PgFileRegistry;
