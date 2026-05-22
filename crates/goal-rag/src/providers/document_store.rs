//! Document store provider trait for storing raw document files

use async_trait::async_trait;
use uuid::Uuid;
use crate::error::Result;

/// Metadata about a stored document
#[derive(Debug, Clone)]
pub struct StoredDocumentInfo {
    /// Document ID
    pub id: Uuid,
    /// Original filename
    pub filename: String,
    /// Storage URI (file path or cloud URI)
    pub uri: String,
    /// Size in bytes
    pub size: u64,
    /// Organization ID (for multi-tenancy)
    pub organization_id: Option<String>,
}

/// Trait for document storage
///
/// Implementations:
/// - `LocalDocumentStore`: Local filesystem
/// - `GcsDocumentStore`: Google Cloud Storage
#[async_trait]
pub trait DocumentStoreProvider: Send + Sync {
    /// Store a document with optional organization isolation
    ///
    /// Returns the storage URI
    async fn store_document(
        &self,
        doc_id: &Uuid,
        filename: &str,
        data: &[u8],
        organization_id: Option<&str>,
    ) -> Result<String>;

    /// Retrieve document data
    async fn get_document(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<Vec<u8>>;

    /// Check if document exists
    async fn exists(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<bool>;

    /// Delete a document
    async fn delete_document(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<()>;

    /// List all stored document IDs (optionally filtered by organization)
    async fn list_documents(&self, organization_id: Option<&str>) -> Result<Vec<StoredDocumentInfo>>;

    /// Get storage URI for a document
    async fn get_uri(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<Option<String>>;

    /// Check if the provider is healthy
    async fn health_check(&self) -> Result<bool>;

    /// Get provider name for logging
    fn name(&self) -> &str;
}
