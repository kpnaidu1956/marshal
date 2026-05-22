//! Google Cloud Storage document store
//!
//! Stores raw documents in GCS for scalable, durable storage.

use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use google_cloud_storage::client::Client as GcsClient;
use google_cloud_storage::http::objects::delete::DeleteObjectRequest;
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::list::ListObjectsRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};

use super::auth::GcpAuth;
use crate::error::{Error, Result};
use crate::providers::document_store::{DocumentStoreProvider, StoredDocumentInfo};

/// Google Cloud Storage document store
pub struct GcsDocumentStore {
    #[allow(dead_code)]
    auth: Arc<GcpAuth>,
    client: GcsClient,
    bucket: String,
    /// Prefix for original documents
    originals_prefix: String,
    /// Prefix for extracted plain text
    plaintext_prefix: String,
}

impl GcsDocumentStore {
    /// Create a new GCS document store
    ///
    /// # Arguments
    /// * `auth` - GCP authentication
    /// * `bucket` - GCS bucket name
    /// * `originals_prefix` - Prefix for original documents (e.g., "originals/")
    /// * `plaintext_prefix` - Prefix for extracted plain text (e.g., "plaintext/")
    pub async fn new(
        auth: Arc<GcpAuth>,
        bucket: String,
        originals_prefix: Option<String>,
        plaintext_prefix: Option<String>,
    ) -> Result<Self> {
        // Create GCS client using the service account
        let config = google_cloud_storage::client::ClientConfig::default()
            .with_auth()
            .await
            .map_err(|e| Error::Config(format!("Failed to create GCS client: {}", e)))?;

        let client = GcsClient::new(config);

        Ok(Self {
            auth,
            client,
            bucket,
            originals_prefix: originals_prefix.unwrap_or_else(|| "originals/".to_string()),
            plaintext_prefix: plaintext_prefix.unwrap_or_else(|| "plaintext/".to_string()),
        })
    }

    /// Get organization folder name (uses "_default" for no org)
    fn org_folder(organization_id: Option<&str>) -> &str {
        organization_id.unwrap_or("_default")
    }

    /// Get the full object path for an original document
    fn object_path(&self, doc_id: &Uuid, extension: &str, organization_id: Option<&str>) -> String {
        format!("{}{}/{}.{}", self.originals_prefix, Self::org_folder(organization_id), doc_id, extension)
    }

    /// Get the full object path for plain text
    fn plaintext_object_path(&self, doc_id: &Uuid, organization_id: Option<&str>) -> String {
        format!("{}{}/{}.txt", self.plaintext_prefix, Self::org_folder(organization_id), doc_id)
    }

    /// Get GCS URI for an original document
    fn gcs_uri(&self, doc_id: &Uuid, extension: &str, organization_id: Option<&str>) -> String {
        format!("gs://{}/{}", self.bucket, self.object_path(doc_id, extension, organization_id))
    }

    /// Get GCS URI for plain text
    fn plaintext_gcs_uri(&self, doc_id: &Uuid, organization_id: Option<&str>) -> String {
        format!("gs://{}/{}", self.bucket, self.plaintext_object_path(doc_id, organization_id))
    }

    /// Store extracted plain text for a document
    ///
    /// # Arguments
    /// * `doc_id` - Document UUID
    /// * `filename` - Original filename (for metadata)
    /// * `text` - Extracted plain text content
    /// * `organization_id` - Organization ID for multi-tenancy (stored in org subfolder)
    pub async fn store_plain_text(
        &self,
        doc_id: &Uuid,
        filename: &str,
        text: &str,
        organization_id: Option<&str>,
    ) -> Result<String> {
        let object_path = self.plaintext_object_path(doc_id, organization_id);
        let upload_type = UploadType::Simple(Media::new(object_path.clone()));

        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                text.as_bytes().to_vec(),
                &upload_type,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upload plain text to GCS: {}", e)))?;

        tracing::debug!(
            "Stored plain text for {} ({}) in org {:?} at {}",
            filename,
            doc_id,
            organization_id,
            object_path
        );

        Ok(self.plaintext_gcs_uri(doc_id, organization_id))
    }

    /// Get extracted plain text for a document
    ///
    /// # Arguments
    /// * `doc_id` - Document UUID
    /// * `organization_id` - Organization ID for multi-tenancy
    pub async fn get_plain_text(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<Option<String>> {
        let object_path = self.plaintext_object_path(doc_id, organization_id);

        match self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: object_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
        {
            Ok(data) => {
                let text = String::from_utf8(data).map_err(|e| {
                    Error::Internal(format!("Plain text is not valid UTF-8: {}", e))
                })?;
                Ok(Some(text))
            }
            Err(_) => Ok(None),
        }
    }

    /// Get document info with both original and plain text URIs
    ///
    /// # Arguments
    /// * `doc_id` - Document UUID
    /// * `organization_id` - Organization ID for multi-tenancy
    pub async fn get_document_with_info(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<Option<DocumentWithInfo>> {
        let meta_path = self.object_path(doc_id, "meta.json", organization_id);

        match self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: meta_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
        {
            Ok(meta_data) => {
                if let Ok(metadata) = serde_json::from_slice::<DocumentMetadata>(&meta_data) {
                    let filename = metadata.filename.clone();
                    let extension = std::path::Path::new(&filename)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("bin")
                        .to_string();

                    // Check if plain text exists
                    let plaintext_uri = if self.plaintext_exists(doc_id, organization_id).await {
                        Some(self.plaintext_gcs_uri(doc_id, organization_id))
                    } else {
                        None
                    };

                    Ok(Some(DocumentWithInfo {
                        id: metadata.id,
                        filename,
                        size: metadata.size,
                        content_type: metadata.content_type,
                        original_uri: self.gcs_uri(doc_id, &extension, organization_id),
                        plaintext_uri,
                    }))
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Check if plain text exists for a document
    async fn plaintext_exists(&self, doc_id: &Uuid, organization_id: Option<&str>) -> bool {
        let object_path = self.plaintext_object_path(doc_id, organization_id);

        self.client
            .get_object(&GetObjectRequest {
                bucket: self.bucket.clone(),
                object: object_path,
                ..Default::default()
            })
            .await
            .is_ok()
    }

    /// Delete plain text for a document
    pub async fn delete_plain_text(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<()> {
        let object_path = self.plaintext_object_path(doc_id, organization_id);

        let _ = self
            .client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: object_path,
                ..Default::default()
            })
            .await;

        Ok(())
    }
}

/// Document info with both original and plain text URIs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentWithInfo {
    pub id: Uuid,
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    pub original_uri: String,
    pub plaintext_uri: Option<String>,
}

/// Info about a file in storage (for listing)
#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageFileInfo {
    /// Relative path within the bucket/org folder
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Content type
    pub content_type: String,
    /// Last updated timestamp (RFC3339 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DocumentMetadata {
    id: Uuid,
    filename: String,
    size: u64,
    content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organization_id: Option<String>,
}

#[async_trait]
impl DocumentStoreProvider for GcsDocumentStore {
    async fn store_document(
        &self,
        doc_id: &Uuid,
        filename: &str,
        data: &[u8],
        organization_id: Option<&str>,
    ) -> Result<String> {
        // Determine content type from filename
        let content_type = mime_guess::from_path(filename)
            .first_or_octet_stream()
            .to_string();

        // Upload document data
        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");

        let object_path = self.object_path(doc_id, extension, organization_id);

        let upload_type = UploadType::Simple(Media::new(object_path.clone()));

        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                data.to_vec(),
                &upload_type,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upload to GCS: {}", e)))?;

        // Upload metadata (includes organization_id for retrieval)
        let metadata = DocumentMetadata {
            id: *doc_id,
            filename: filename.to_string(),
            size: data.len() as u64,
            content_type,
            organization_id: organization_id.map(|s| s.to_string()),
        };
        let meta_json = serde_json::to_vec(&metadata)?;
        let meta_path = self.object_path(doc_id, "meta.json", organization_id);

        let meta_upload_type = UploadType::Simple(Media::new(meta_path));

        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                meta_json,
                &meta_upload_type,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upload metadata to GCS: {}", e)))?;

        tracing::debug!(
            "Stored document {} ({}) in org {:?} at {}",
            filename,
            doc_id,
            organization_id,
            object_path
        );

        Ok(self.gcs_uri(doc_id, extension, organization_id))
    }

    async fn get_document(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<Vec<u8>> {
        // First, get metadata to find the extension
        let meta_path = self.object_path(doc_id, "meta.json", organization_id);

        let meta_data = self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: meta_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .map_err(|e| {
                Error::Internal(format!("Failed to download metadata from GCS: {}", e))
            })?;

        let metadata: DocumentMetadata =
            serde_json::from_slice(&meta_data).map_err(|e| {
                Error::Internal(format!("Failed to parse document metadata: {}", e))
            })?;

        // Get extension from original filename
        let extension = std::path::Path::new(&metadata.filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");

        let object_path = self.object_path(doc_id, extension, organization_id);

        self.client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: object_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to download from GCS: {}", e)))
    }

    async fn exists(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<bool> {
        let meta_path = self.object_path(doc_id, "meta.json", organization_id);

        match self
            .client
            .get_object(&GetObjectRequest {
                bucket: self.bucket.clone(),
                object: meta_path,
                ..Default::default()
            })
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn delete_document(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<()> {
        // Get metadata first to find the extension
        let meta_path = self.object_path(doc_id, "meta.json", organization_id);

        if let Ok(meta_data) = self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: meta_path.clone(),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
        {
            if let Ok(metadata) = serde_json::from_slice::<DocumentMetadata>(&meta_data) {
                let extension = std::path::Path::new(&metadata.filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("bin");

                let object_path = self.object_path(doc_id, extension, organization_id);

                // Delete original document
                let _ = self
                    .client
                    .delete_object(&DeleteObjectRequest {
                        bucket: self.bucket.clone(),
                        object: object_path,
                        ..Default::default()
                    })
                    .await;
            }
        }

        // Delete plain text
        let plaintext_path = self.plaintext_object_path(doc_id, organization_id);
        let _ = self
            .client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: plaintext_path,
                ..Default::default()
            })
            .await;

        // Delete metadata
        let _ = self
            .client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: meta_path,
                ..Default::default()
            })
            .await;

        Ok(())
    }

    async fn list_documents(&self, organization_id: Option<&str>) -> Result<Vec<StoredDocumentInfo>> {
        let mut docs = Vec::new();

        // Build prefix for the specific organization (or _default)
        let org_prefix = format!("{}{}/", self.originals_prefix, Self::org_folder(organization_id));

        let list_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(org_prefix),
            ..Default::default()
        };

        let objects = self
            .client
            .list_objects(&list_request)
            .await
            .map_err(|e| Error::Internal(format!("Failed to list GCS objects: {}", e)))?;

        for item in objects.items.unwrap_or_default() {
            // Only process metadata files
            if item.name.ends_with(".meta.json") {
                if let Ok(meta_data) = self
                    .client
                    .download_object(
                        &GetObjectRequest {
                            bucket: self.bucket.clone(),
                            object: item.name.clone(),
                            ..Default::default()
                        },
                        &Range::default(),
                    )
                    .await
                {
                    if let Ok(metadata) = serde_json::from_slice::<DocumentMetadata>(&meta_data) {
                        let filename = metadata.filename;
                        let extension = std::path::Path::new(&filename)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("bin")
                            .to_string();

                        docs.push(StoredDocumentInfo {
                            id: metadata.id,
                            filename,
                            uri: self.gcs_uri(&metadata.id, &extension, organization_id),
                            size: metadata.size,
                            organization_id: metadata.organization_id,
                        });
                    }
                }
            }
        }

        Ok(docs)
    }

    async fn get_uri(&self, doc_id: &Uuid, organization_id: Option<&str>) -> Result<Option<String>> {
        let meta_path = self.object_path(doc_id, "meta.json", organization_id);

        match self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: meta_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
        {
            Ok(meta_data) => {
                if let Ok(metadata) = serde_json::from_slice::<DocumentMetadata>(&meta_data) {
                    let extension = std::path::Path::new(&metadata.filename)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("bin");
                    Ok(Some(self.gcs_uri(doc_id, extension, organization_id)))
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    async fn health_check(&self) -> Result<bool> {
        // Try to list objects (with limit 1) to check bucket access
        let list_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            max_results: Some(1),
            ..Default::default()
        };

        self.client
            .list_objects(&list_request)
            .await
            .map(|_| true)
            .map_err(|e| Error::Internal(format!("GCS health check failed: {}", e)))
    }

    fn name(&self) -> &str {
        "gcs"
    }
}

// ============================================================================
// GCS Sync Methods
// ============================================================================

/// File discovered in GCS during sync
#[derive(Debug, Clone)]
pub struct GcsFileInfo {
    /// Document ID (from metadata)
    pub document_id: Uuid,
    /// Original filename
    pub filename: String,
    /// Content hash (if available)
    pub content_hash: Option<String>,
    /// File size
    pub file_size: u64,
    /// File type (extension)
    pub file_type: String,
    /// Whether plaintext version exists (indicates successful processing)
    pub has_plaintext: bool,
    /// GCS URI for original file
    pub original_uri: String,
    /// GCS URI for plaintext (if exists)
    pub plaintext_uri: Option<String>,
}

impl GcsDocumentStore {
    /// Sync file registry from GCS bucket contents
    ///
    /// This method lists all files in the GCS bucket and determines their state:
    /// - Files with metadata in originals/ prefix are considered uploaded
    /// - Files with corresponding plaintext/ entries are considered successfully processed
    /// - Files with metadata but no plaintext are considered failed/pending
    ///
    /// Returns a list of file info that can be used to populate the database
    pub async fn sync_from_bucket(&self) -> Result<Vec<GcsFileInfo>> {
        tracing::info!("Starting GCS bucket sync...");
        let mut files = Vec::new();

        // List all metadata files in originals prefix
        let list_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(self.originals_prefix.clone()),
            ..Default::default()
        };

        let objects = self
            .client
            .list_objects(&list_request)
            .await
            .map_err(|e| Error::Internal(format!("Failed to list GCS objects: {}", e)))?;

        let items = objects.items.unwrap_or_default();
        let meta_files: Vec<_> = items.iter()
            .filter(|item| item.name.ends_with(".meta.json"))
            .collect();

        tracing::info!("Found {} metadata files in GCS", meta_files.len());

        // Process each metadata file
        for item in meta_files {
            match self.process_meta_file(&item.name).await {
                Ok(Some(file_info)) => {
                    files.push(file_info);
                }
                Ok(None) => {
                    tracing::debug!("Skipping invalid metadata: {}", item.name);
                }
                Err(e) => {
                    tracing::warn!("Error processing {}: {}", item.name, e);
                }
            }
        }

        tracing::info!(
            "GCS sync complete: {} files found ({} with plaintext)",
            files.len(),
            files.iter().filter(|f| f.has_plaintext).count()
        );

        Ok(files)
    }

    /// Sync files from GCS bucket for a specific organization
    ///
    /// Similar to sync_from_bucket but filtered to a specific organization's folder.
    pub async fn sync_from_bucket_for_org(&self, organization_id: &str) -> Result<Vec<GcsFileInfo>> {
        tracing::info!("Starting GCS bucket sync for org '{}'...", organization_id);
        let mut files = Vec::new();

        // List metadata files in organization's originals prefix
        let org_prefix = format!("{}{}/", self.originals_prefix, organization_id);
        let list_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(org_prefix.clone()),
            ..Default::default()
        };

        let objects = self
            .client
            .list_objects(&list_request)
            .await
            .map_err(|e| Error::Internal(format!("Failed to list GCS objects for org '{}': {}", organization_id, e)))?;

        let items = objects.items.unwrap_or_default();
        let meta_files: Vec<_> = items.iter()
            .filter(|item| item.name.ends_with(".meta.json"))
            .collect();

        tracing::info!("Found {} metadata files in GCS for org '{}'", meta_files.len(), organization_id);

        // Process each metadata file
        for item in meta_files {
            match self.process_meta_file(&item.name).await {
                Ok(Some(file_info)) => {
                    files.push(file_info);
                }
                Ok(None) => {
                    tracing::debug!("Skipping invalid metadata: {}", item.name);
                }
                Err(e) => {
                    tracing::warn!("Error processing {}: {}", item.name, e);
                }
            }
        }

        tracing::info!(
            "GCS sync for org '{}' complete: {} files found ({} with plaintext)",
            organization_id,
            files.len(),
            files.iter().filter(|f| f.has_plaintext).count()
        );

        Ok(files)
    }

    /// Process a metadata file and return file info
    async fn process_meta_file(&self, meta_path: &str) -> Result<Option<GcsFileInfo>> {
        // Download metadata
        let meta_data = match self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: meta_path.to_string(),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
        {
            Ok(data) => data,
            Err(_) => return Ok(None),
        };

        // Parse metadata
        let metadata: DocumentMetadata = match serde_json::from_slice(&meta_data) {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };

        // Check if plaintext exists
        let plaintext_path = format!("{}{}.txt", self.plaintext_prefix, metadata.id);
        let has_plaintext = self
            .client
            .get_object(&GetObjectRequest {
                bucket: self.bucket.clone(),
                object: plaintext_path.clone(),
                ..Default::default()
            })
            .await
            .is_ok();

        // Get file extension
        let extension = std::path::Path::new(&metadata.filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
            .to_string();

        Ok(Some(GcsFileInfo {
            document_id: metadata.id,
            filename: metadata.filename.clone(),
            content_hash: None, // Not stored in current metadata
            file_size: metadata.size,
            file_type: extension.clone(),
            has_plaintext,
            original_uri: format!("gs://{}/{}{}.{}", self.bucket, self.originals_prefix, metadata.id, extension),
            plaintext_uri: if has_plaintext {
                Some(format!("gs://{}/{}", self.bucket, plaintext_path))
            } else {
                None
            },
        }))
    }

    /// Get count of files in bucket (quick check)
    pub async fn get_file_counts(&self) -> Result<(usize, usize)> {
        // Count originals
        let orig_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(self.originals_prefix.clone()),
            ..Default::default()
        };
        let orig_objects = self.client.list_objects(&orig_request).await
            .map_err(|e| Error::Internal(format!("Failed to list originals: {}", e)))?;
        let orig_count = orig_objects.items.unwrap_or_default().iter()
            .filter(|i| i.name.ends_with(".meta.json"))
            .count();

        // Count plaintext
        let plain_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(self.plaintext_prefix.clone()),
            ..Default::default()
        };
        let plain_objects = self.client.list_objects(&plain_request).await
            .map_err(|e| Error::Internal(format!("Failed to list plaintext: {}", e)))?;
        let plain_count = plain_objects.items.unwrap_or_default().len();

        Ok((orig_count, plain_count))
    }

    /// Get count of files for a specific organization
    pub async fn get_file_counts_for_org(&self, organization_id: &str) -> Result<(usize, usize)> {
        // Count originals for this organization
        let org_originals_prefix = format!("{}{}/", self.originals_prefix, organization_id);
        let orig_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(org_originals_prefix),
            ..Default::default()
        };
        let orig_objects = self.client.list_objects(&orig_request).await
            .map_err(|e| Error::Internal(format!("Failed to list originals for org {}: {}", organization_id, e)))?;
        let orig_count = orig_objects.items.unwrap_or_default().iter()
            .filter(|i| i.name.ends_with(".meta.json"))
            .count();

        // Count plaintext for this organization
        let org_plaintext_prefix = format!("{}{}/", self.plaintext_prefix, organization_id);
        let plain_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(org_plaintext_prefix),
            ..Default::default()
        };
        let plain_objects = self.client.list_objects(&plain_request).await
            .map_err(|e| Error::Internal(format!("Failed to list plaintext for org {}: {}", organization_id, e)))?;
        let plain_count = plain_objects.items.unwrap_or_default().len();

        Ok((orig_count, plain_count))
    }

    /// Migrate a document from old flat structure to organization-specific folder
    ///
    /// Old structure: originals/{doc_id}.{ext}, plaintext/{doc_id}.txt
    /// New structure: originals/{org_id}/{doc_id}.{ext}, plaintext/{org_id}/{doc_id}.txt
    ///
    /// Returns (original_moved, plaintext_moved, new_original_uri, new_plaintext_uri)
    pub async fn migrate_document_to_org(
        &self,
        doc_id: &Uuid,
        filename: &str,
        organization_id: &str,
    ) -> Result<GcsMigrationResult> {
        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");

        // Old paths (flat structure)
        let old_original_path = format!("{}{}.{}", self.originals_prefix, doc_id, extension);
        let old_meta_path = format!("{}{}.meta.json", self.originals_prefix, doc_id);
        let old_plaintext_path = format!("{}{}.txt", self.plaintext_prefix, doc_id);

        // New paths (org-specific structure)
        let new_original_path = self.object_path(doc_id, extension, Some(organization_id));
        let new_meta_path = self.object_path(doc_id, "meta.json", Some(organization_id));
        let new_plaintext_path = self.plaintext_object_path(doc_id, Some(organization_id));

        let mut result = GcsMigrationResult {
            doc_id: *doc_id,
            original_moved: false,
            plaintext_moved: false,
            metadata_moved: false,
            new_original_uri: None,
            new_plaintext_uri: None,
            error: None,
        };

        // Move original file
        match self.copy_and_delete(&old_original_path, &new_original_path).await {
            Ok(true) => {
                result.original_moved = true;
                result.new_original_uri = Some(self.gcs_uri(doc_id, extension, Some(organization_id)));
                tracing::debug!("Moved original: {} -> {}", old_original_path, new_original_path);
            }
            Ok(false) => {
                // File doesn't exist at old path, check if already at new path
                if self.object_exists(&new_original_path).await {
                    result.new_original_uri = Some(self.gcs_uri(doc_id, extension, Some(organization_id)));
                }
            }
            Err(e) => {
                result.error = Some(format!("Failed to move original: {}", e));
                return Ok(result);
            }
        }

        // Move metadata file
        match self.copy_and_delete(&old_meta_path, &new_meta_path).await {
            Ok(true) => {
                result.metadata_moved = true;
                tracing::debug!("Moved metadata: {} -> {}", old_meta_path, new_meta_path);
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!("Failed to move metadata for {}: {}", doc_id, e);
            }
        }

        // Move plaintext file
        match self.copy_and_delete(&old_plaintext_path, &new_plaintext_path).await {
            Ok(true) => {
                result.plaintext_moved = true;
                result.new_plaintext_uri = Some(self.plaintext_gcs_uri(doc_id, Some(organization_id)));
                tracing::debug!("Moved plaintext: {} -> {}", old_plaintext_path, new_plaintext_path);
            }
            Ok(false) => {
                // File doesn't exist at old path, check if already at new path
                if self.object_exists(&new_plaintext_path).await {
                    result.new_plaintext_uri = Some(self.plaintext_gcs_uri(doc_id, Some(organization_id)));
                }
            }
            Err(e) => {
                tracing::warn!("Failed to move plaintext for {}: {}", doc_id, e);
            }
        }

        Ok(result)
    }

    /// Copy an object and delete the original (move operation)
    /// Returns Ok(true) if moved, Ok(false) if source doesn't exist
    async fn copy_and_delete(&self, from_path: &str, to_path: &str) -> Result<bool> {
        use google_cloud_storage::http::objects::copy::CopyObjectRequest;

        // Check if source exists
        if !self.object_exists(from_path).await {
            return Ok(false);
        }

        // Copy to new location
        let copy_request = CopyObjectRequest {
            source_bucket: self.bucket.clone(),
            source_object: from_path.to_string(),
            destination_bucket: self.bucket.clone(),
            destination_object: to_path.to_string(),
            ..Default::default()
        };

        self.client
            .copy_object(&copy_request)
            .await
            .map_err(|e| Error::Internal(format!("Failed to copy {} to {}: {}", from_path, to_path, e)))?;

        // Delete original
        let _ = self
            .client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: from_path.to_string(),
                ..Default::default()
            })
            .await;

        Ok(true)
    }

    /// Check if an object exists
    async fn object_exists(&self, path: &str) -> bool {
        self.client
            .get_object(&GetObjectRequest {
                bucket: self.bucket.clone(),
                object: path.to_string(),
                ..Default::default()
            })
            .await
            .is_ok()
    }
}

/// Result of migrating a document to organization-specific folder
#[derive(Debug, Clone, serde::Serialize)]
pub struct GcsMigrationResult {
    pub doc_id: Uuid,
    pub original_moved: bool,
    pub plaintext_moved: bool,
    pub metadata_moved: bool,
    pub new_original_uri: Option<String>,
    pub new_plaintext_uri: Option<String>,
    pub error: Option<String>,
}

// ============================================================================
// Filename-Based Storage Methods (New Architecture)
// ============================================================================

/// Metadata stored alongside files using filename-based storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileMetadataByName {
    /// Original filename
    pub filename: String,
    /// Organization ID
    pub organization_id: String,
    /// Content hash (SHA-256)
    pub content_hash: String,
    /// File size in bytes
    pub file_size: u64,
    /// Content type
    pub content_type: String,
    /// Upload timestamp
    pub uploaded_at: chrono::DateTime<chrono::Utc>,
}

impl GcsDocumentStore {
    // ===== Filename-based path helpers =====

    /// Get the full object path for a file using original filename
    /// Format: originals/{org_id}/{filename}
    pub fn object_path_by_name(&self, filename: &str, organization_id: &str) -> String {
        format!("{}{}/{}", self.originals_prefix, organization_id, filename)
    }

    /// Get the full object path for plain text using original filename
    /// Format: plaintext/{org_id}/{filename}.txt
    pub fn plaintext_path_by_name(&self, filename: &str, organization_id: &str) -> String {
        // Remove extension and add .txt
        let base_name = std::path::Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);
        format!("{}{}/{}.txt", self.plaintext_prefix, organization_id, base_name)
    }

    /// Get the metadata path for a file
    /// Format: originals/{org_id}/{filename}.meta.json
    fn metadata_path_by_name(&self, filename: &str, organization_id: &str) -> String {
        format!("{}{}/{}.meta.json", self.originals_prefix, organization_id, filename)
    }

    /// Get GCS URI for a file by name
    pub fn gcs_uri_by_name(&self, filename: &str, organization_id: &str) -> String {
        format!("gs://{}/{}", self.bucket, self.object_path_by_name(filename, organization_id))
    }

    /// Get GCS URI for plain text by name
    pub fn plaintext_gcs_uri_by_name(&self, filename: &str, organization_id: &str) -> String {
        format!("gs://{}/{}", self.bucket, self.plaintext_path_by_name(filename, organization_id))
    }

    // ===== Filename-based operations =====

    /// Check if a file exists in GCS by org + filename
    pub async fn file_exists_by_name(&self, filename: &str, organization_id: &str) -> bool {
        let object_path = self.object_path_by_name(filename, organization_id);
        self.object_exists(&object_path).await
    }

    /// Get metadata for a file by org + filename
    pub async fn get_file_metadata_by_name(
        &self,
        filename: &str,
        organization_id: &str,
    ) -> Result<Option<FileMetadataByName>> {
        let meta_path = self.metadata_path_by_name(filename, organization_id);

        match self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: meta_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
        {
            Ok(data) => {
                let metadata: FileMetadataByName = serde_json::from_slice(&data)
                    .map_err(|e| Error::Internal(format!("Failed to parse metadata: {}", e)))?;
                Ok(Some(metadata))
            }
            Err(_) => Ok(None),
        }
    }

    /// Store a document using original filename (new architecture)
    ///
    /// # Arguments
    /// * `filename` - Original filename (used as identifier)
    /// * `organization_id` - Organization ID (required)
    /// * `data` - File content
    /// * `content_hash` - Pre-calculated content hash (SHA-256)
    ///
    /// # Returns
    /// GCS URI for the stored file
    pub async fn store_document_by_name(
        &self,
        filename: &str,
        organization_id: &str,
        data: &[u8],
        content_hash: &str,
    ) -> Result<String> {
        let content_type = mime_guess::from_path(filename)
            .first_or_octet_stream()
            .to_string();

        let object_path = self.object_path_by_name(filename, organization_id);
        let upload_type = UploadType::Simple(Media::new(object_path.clone()));

        // Upload file
        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                data.to_vec(),
                &upload_type,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upload to GCS: {}", e)))?;

        // Upload metadata
        let metadata = FileMetadataByName {
            filename: filename.to_string(),
            organization_id: organization_id.to_string(),
            content_hash: content_hash.to_string(),
            file_size: data.len() as u64,
            content_type,
            uploaded_at: chrono::Utc::now(),
        };
        let meta_json = serde_json::to_vec(&metadata)?;
        let meta_path = self.metadata_path_by_name(filename, organization_id);
        let meta_upload_type = UploadType::Simple(Media::new(meta_path));

        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                meta_json,
                &meta_upload_type,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upload metadata to GCS: {}", e)))?;

        tracing::info!(
            "Stored document by name: {}/{} (hash: {})",
            organization_id,
            filename,
            &content_hash[..8]
        );

        Ok(self.gcs_uri_by_name(filename, organization_id))
    }

    /// Store extracted plain text for a file by name
    pub async fn store_plain_text_by_name(
        &self,
        filename: &str,
        organization_id: &str,
        text: &str,
    ) -> Result<String> {
        let object_path = self.plaintext_path_by_name(filename, organization_id);
        let upload_type = UploadType::Simple(Media::new(object_path.clone()));

        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                text.as_bytes().to_vec(),
                &upload_type,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upload plain text to GCS: {}", e)))?;

        tracing::debug!(
            "Stored plain text for {}/{} at {}",
            organization_id,
            filename,
            object_path
        );

        Ok(self.plaintext_gcs_uri_by_name(filename, organization_id))
    }

    /// Get document content by org + filename
    pub async fn get_document_by_name(
        &self,
        filename: &str,
        organization_id: &str,
    ) -> Result<Vec<u8>> {
        let object_path = self.object_path_by_name(filename, organization_id);

        self.client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: object_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to download from GCS: {}", e)))
    }

    /// Get plain text content by org + filename
    pub async fn get_plain_text_by_name(
        &self,
        filename: &str,
        organization_id: &str,
    ) -> Result<Option<String>> {
        let object_path = self.plaintext_path_by_name(filename, organization_id);

        match self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: object_path,
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
        {
            Ok(data) => {
                let text = String::from_utf8(data)
                    .map_err(|e| Error::Internal(format!("Plain text is not valid UTF-8: {}", e)))?;
                Ok(Some(text))
            }
            Err(_) => Ok(None),
        }
    }

    /// Delete a file by org + filename (deletes original, plaintext, and metadata)
    pub async fn delete_document_by_name(
        &self,
        filename: &str,
        organization_id: &str,
    ) -> Result<()> {
        // Delete original file
        let object_path = self.object_path_by_name(filename, organization_id);
        let _ = self
            .client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: object_path,
                ..Default::default()
            })
            .await;

        // Delete metadata
        let meta_path = self.metadata_path_by_name(filename, organization_id);
        let _ = self
            .client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: meta_path,
                ..Default::default()
            })
            .await;

        // Delete plain text
        let plaintext_path = self.plaintext_path_by_name(filename, organization_id);
        let _ = self
            .client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: plaintext_path,
                ..Default::default()
            })
            .await;

        tracing::info!("Deleted document by name: {}/{}", organization_id, filename);

        Ok(())
    }

    /// Find the next available version for a filename
    /// e.g., if "document.pdf" exists, returns "document_v2.pdf"
    /// if "document_v2.pdf" exists, returns "document_v3.pdf"
    pub async fn find_next_version(
        &self,
        filename: &str,
        organization_id: &str,
    ) -> Result<String> {
        let path = std::path::Path::new(filename);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(filename);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Check existing versions
        let mut version = 2;
        loop {
            let versioned_name = if extension.is_empty() {
                format!("{}_v{}", stem, version)
            } else {
                format!("{}_v{}.{}", stem, version, extension)
            };

            if !self.file_exists_by_name(&versioned_name, organization_id).await {
                return Ok(versioned_name);
            }

            version += 1;

            // Safety limit to prevent infinite loop
            if version > 1000 {
                return Err(Error::Internal(format!(
                    "Too many versions for file: {}",
                    filename
                )));
            }
        }
    }

    // ===== Generic Storage Operations (for frontend attachments) =====

    /// Store a file in the storage prefix (not for RAG processing)
    /// Path format: storage/{bucket}/{org_id}/{path}
    ///
    /// Takes ownership of data to avoid unnecessary copying.
    pub async fn store_storage_file(
        &self,
        gcs_path: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<()> {
        let data_len = data.len();
        let upload_type = UploadType::Simple(Media {
            name: gcs_path.to_string().into(),
            content_type: content_type.to_string().into(),
            content_length: Some(data_len as u64),
        });

        self.client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket.clone(),
                    ..Default::default()
                },
                data, // No copy needed - we own the data
                &upload_type,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to upload storage file to GCS: {}", e)))?;

        Ok(())
    }

    /// List files in a storage bucket/org path
    ///
    /// Returns a list of (relative_path, size, content_type) tuples for all files
    /// under the given prefix.
    pub async fn list_storage_files(
        &self,
        bucket_name: &str,
        organization_id: &str,
    ) -> Result<Vec<StorageFileInfo>> {
        let prefix = format!("storage/{}/{}/", bucket_name, organization_id);

        let list_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(prefix.clone()),
            ..Default::default()
        };

        let objects = self
            .client
            .list_objects(&list_request)
            .await
            .map_err(|e| Error::Internal(format!("Failed to list storage files: {}", e)))?;

        let files = objects
            .items
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                // Extract relative path (after the prefix)
                let relative_path = item.name.strip_prefix(&prefix)?.to_string();
                if relative_path.is_empty() {
                    return None;
                }

                Some(StorageFileInfo {
                    path: relative_path,
                    size: item.size as u64,
                    content_type: item.content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                    // Convert time::OffsetDateTime to string representation
                    updated_at: item.updated.map(|dt| dt.to_string()),
                })
            })
            .collect();

        Ok(files)
    }

    /// Get a file from storage
    /// Returns (data, content_type)
    pub async fn get_storage_file(&self, gcs_path: &str) -> Result<(Vec<u8>, String)> {
        use google_cloud_storage::http::objects::get::GetObjectRequest;
        use google_cloud_storage::http::objects::download::Range;

        // Get object metadata for content type
        let obj = self.client
            .get_object(&GetObjectRequest {
                bucket: self.bucket.clone(),
                object: gcs_path.to_string(),
                ..Default::default()
            })
            .await
            .map_err(|e| Error::DocumentNotFound(format!("File not found: {}", e)))?;

        let content_type = obj.content_type.unwrap_or_else(|| "application/octet-stream".to_string());

        // Download the file
        let data = self.client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: gcs_path.to_string(),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to download storage file from GCS: {}", e)))?;

        Ok((data, content_type))
    }

    /// Delete a file from storage
    pub async fn delete_storage_file(&self, gcs_path: &str) -> Result<()> {
        self.client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: gcs_path.to_string(),
                ..Default::default()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to delete storage file from GCS: {}", e)))?;

        Ok(())
    }

    /// List all files in an organization folder
    pub async fn list_files_by_org(&self, organization_id: &str) -> Result<Vec<FileMetadataByName>> {
        let mut files = Vec::new();

        let prefix = format!("{}{}/", self.originals_prefix, organization_id);
        let list_request = ListObjectsRequest {
            bucket: self.bucket.clone(),
            prefix: Some(prefix),
            ..Default::default()
        };

        let objects = self
            .client
            .list_objects(&list_request)
            .await
            .map_err(|e| Error::Internal(format!("Failed to list GCS objects: {}", e)))?;

        for item in objects.items.unwrap_or_default() {
            // Only process metadata files
            if item.name.ends_with(".meta.json") {
                if let Ok(meta_data) = self
                    .client
                    .download_object(
                        &GetObjectRequest {
                            bucket: self.bucket.clone(),
                            object: item.name.clone(),
                            ..Default::default()
                        },
                        &Range::default(),
                    )
                    .await
                {
                    if let Ok(metadata) = serde_json::from_slice::<FileMetadataByName>(&meta_data) {
                        files.push(metadata);
                    }
                }
            }
        }

        Ok(files)
    }
}
