//! Response types for RAG queries

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::document::{Chunk, Document, FileType};

/// Citation from a source document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    /// Chunk ID
    pub chunk_id: Uuid,
    /// Document ID
    pub document_id: Uuid,
    /// Source filename
    pub filename: String,
    /// File type
    pub file_type: FileType,
    /// Page number (if applicable)
    pub page_number: Option<u32>,
    /// Section title (if detected)
    pub section_title: Option<String>,
    /// Line numbers for code files
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    /// Exact snippet from the source
    pub snippet: String,
    /// Snippet with highlighted query terms (<mark> tags)
    pub snippet_highlighted: String,
    /// Similarity score (0.0-1.0)
    pub similarity_score: f32,
    /// Rerank score (if reranking was enabled)
    pub rerank_score: Option<f32>,
    /// URL to original document in GCS (authenticated access)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_url: Option<String>,
    /// URL to extracted plain text in GCS (authenticated access)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plaintext_url: Option<String>,
}

impl Citation {
    /// Create a citation from a chunk and similarity score
    pub fn from_chunk(chunk: &Chunk, similarity_score: f32) -> Self {
        Self {
            chunk_id: chunk.id,
            document_id: chunk.document_id,
            filename: chunk.source.filename.clone(),
            file_type: chunk.source.file_type.clone(),
            page_number: chunk.source.page_number,
            section_title: chunk.source.section_title.clone(),
            line_start: chunk.source.line_start,
            line_end: chunk.source.line_end,
            snippet: chunk.content.clone(),
            snippet_highlighted: chunk.content.clone(),
            similarity_score,
            rerank_score: None,
            document_url: None,
            plaintext_url: None,
        }
    }

    /// Enrich citation with document URLs from metadata
    pub fn enrich_with_document(&mut self, document: &Document) {
        if let Some(url) = document.metadata.get("original_uri") {
            if let Some(url_str) = url.as_str() {
                self.document_url = Some(url_str.to_string());
            }
        }
        if let Some(url) = document.metadata.get("plaintext_uri") {
            if let Some(url_str) = url.as_str() {
                self.plaintext_url = Some(url_str.to_string());
            }
        }
    }

    /// Format citation for display in text
    pub fn format_inline(&self) -> String {
        let mut parts = vec![self.filename.clone()];

        if let Some(page) = self.page_number {
            parts.push(format!("Page {}", page));
        }

        if let (Some(start), Some(end)) = (self.line_start, self.line_end) {
            parts.push(format!("Lines {}-{}", start, end));
        }

        format!("[Source: {}]", parts.join(", "))
    }

    /// Highlight query terms in the snippet
    pub fn highlight_terms(&mut self, terms: &[&str]) {
        let mut highlighted = self.snippet.clone();
        for term in terms {
            // Case-insensitive replacement with <mark> tags
            let re = regex::RegexBuilder::new(&regex::escape(term))
                .case_insensitive(true)
                .build();
            if let Ok(re) = re {
                highlighted = re
                    .replace_all(&highlighted, |caps: &regex::Captures| {
                        format!("<mark>{}</mark>", &caps[0])
                    })
                    .to_string();
            }
        }
        self.snippet_highlighted = highlighted;
    }
}

/// Response from a RAG query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// Generated answer in clear language
    pub answer: String,
    /// Citations with source snippets
    pub citations: Vec<Citation>,
    /// Overall confidence score (0.0-1.0)
    pub confidence: f32,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    /// Number of chunks retrieved
    pub chunks_retrieved: usize,
    /// Number of chunks used in answer
    pub chunks_used: usize,
    /// Interaction ID for feedback/learning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<Uuid>,
    /// Raw chunks (if include_chunks was true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_chunks: Option<Vec<Chunk>>,
}

impl QueryResponse {
    /// Create a new query response
    pub fn new(answer: String, citations: Vec<Citation>, processing_time_ms: u64) -> Self {
        let confidence = if citations.is_empty() {
            0.0
        } else {
            // Average similarity score
            citations.iter().map(|c| c.similarity_score).sum::<f32>() / citations.len() as f32
        };

        Self {
            answer,
            confidence,
            chunks_retrieved: citations.len(),
            chunks_used: citations.len(),
            citations,
            processing_time_ms,
            interaction_id: None,
            raw_chunks: None,
        }
    }

    /// Create an error response when no relevant information is found
    pub fn not_found(processing_time_ms: u64) -> Self {
        Self {
            answer: "I couldn't find relevant information in the documents to answer this question.".to_string(),
            citations: Vec::new(),
            confidence: 0.0,
            processing_time_ms,
            chunks_retrieved: 0,
            chunks_used: 0,
            interaction_id: None,
            raw_chunks: None,
        }
    }
}

// ============ File Upload Response Types ============

/// Action taken when uploading a file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UploadAction {
    /// New file created
    Created,
    /// Existing file replaced (same content hash)
    Replaced,
    /// New version created (different content, same base filename)
    Versioned {
        /// The new versioned filename (e.g., "document_v2.pdf")
        new_filename: String,
    },
}

/// Information about an uploaded file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadInfo {
    /// Original or versioned filename
    pub filename: String,
    /// Organization ID
    pub organization_id: String,
    /// GCS path where file is stored
    pub gcs_path: String,
    /// Content hash (SHA-256)
    pub content_hash: String,
    /// File size in bytes
    pub file_size: u64,
    /// Action taken (created, replaced, versioned)
    pub action: UploadAction,
}

/// Response from file upload (Phase 1 - immediate after GCS upload)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadResponse {
    /// Whether upload was successful
    pub success: bool,
    /// Whether file was uploaded to GCS
    pub gcs_uploaded: bool,
    /// Information about the uploaded file
    pub file: FileUploadInfo,
    /// Job ID for tracking processing progress (Phase 2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<Uuid>,
    /// URL to poll for processing status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_status_url: Option<String>,
    /// Error message if upload failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl FileUploadResponse {
    /// Create a successful upload response
    pub fn success(file: FileUploadInfo, job_id: Uuid) -> Self {
        let status_url = format!("/api/jobs/{}", job_id);
        Self {
            success: true,
            gcs_uploaded: true,
            file,
            job_id: Some(job_id),
            processing_status_url: Some(status_url),
            error: None,
        }
    }

    /// Create a failed upload response
    pub fn error(filename: String, organization_id: String, error: String) -> Self {
        Self {
            success: false,
            gcs_uploaded: false,
            file: FileUploadInfo {
                filename,
                organization_id,
                gcs_path: String::new(),
                content_hash: String::new(),
                file_size: 0,
                action: UploadAction::Created,
            },
            job_id: None,
            processing_status_url: None,
            error: Some(error),
        }
    }
}

/// Response from batch file upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchUploadResponse {
    /// Whether all uploads were successful
    pub success: bool,
    /// Number of files uploaded successfully
    pub files_uploaded: usize,
    /// Number of files that failed
    pub files_failed: usize,
    /// Individual file responses
    pub files: Vec<FileUploadResponse>,
    /// Total upload time in milliseconds
    pub upload_time_ms: u64,
}

// ============ End File Upload Response Types ============

/// Response from document ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    /// Whether ingestion was successful
    pub success: bool,
    /// Ingested documents
    pub documents: Vec<DocumentSummary>,
    /// Total chunks created across all documents
    pub total_chunks_created: u32,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    /// Any errors encountered (partial success)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<IngestError>,
}

/// Summary of an ingested document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummary {
    /// Document ID
    pub id: Uuid,
    /// Filename
    pub filename: String,
    /// File type
    pub file_type: FileType,
    /// Number of pages (if applicable)
    pub total_pages: Option<u32>,
    /// Number of chunks created
    pub total_chunks: u32,
    /// File size in bytes
    pub file_size: u64,
    /// Ingestion timestamp
    pub ingested_at: chrono::DateTime<chrono::Utc>,
    /// Whether the document is archived (chunks excluded from search)
    #[serde(default)]
    pub archived: bool,
}

impl From<&Document> for DocumentSummary {
    fn from(doc: &Document) -> Self {
        Self {
            id: doc.id,
            filename: doc.filename.clone(),
            file_type: doc.file_type.clone(),
            total_pages: doc.total_pages,
            total_chunks: doc.total_chunks,
            file_size: doc.file_size,
            ingested_at: doc.ingested_at,
            archived: false,
        }
    }
}

/// Error during ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestError {
    /// Filename that failed
    pub filename: String,
    /// Error message
    pub error: String,
}

/// Response for listing documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentListResponse {
    /// List of documents
    pub documents: Vec<DocumentSummary>,
    /// Total count
    pub total_count: usize,
}

// ============ V2 API Response Types ============

/// Status of a single file during ingestion (for V2 API)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FileIngestStatus {
    /// New file successfully processed
    New {
        document: DocumentSummary,
        chunks_created: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        original_uri: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        plaintext_uri: Option<String>,
    },
    /// File was modified, old version replaced
    Updated {
        document: DocumentSummary,
        chunks_created: u32,
        old_chunks_deleted: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        original_uri: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        plaintext_uri: Option<String>,
    },
    /// File unchanged (same content hash)
    Unchanged {
        existing_document_id: Uuid,
        filename: String,
        message: String,
    },
    /// Duplicate content exists under different filename
    Duplicate {
        existing_document_id: Uuid,
        existing_filename: String,
        message: String,
    },
    /// Processing failed
    Failed {
        filename: String,
        error: String,
    },
}

impl FileIngestStatus {
    /// Get the filename from any status variant
    pub fn filename(&self) -> &str {
        match self {
            Self::New { document, .. } => &document.filename,
            Self::Updated { document, .. } => &document.filename,
            Self::Unchanged { filename, .. } => filename,
            Self::Duplicate { existing_filename, .. } => existing_filename,
            Self::Failed { filename, .. } => filename,
        }
    }

    /// Check if this was a successful processing
    pub fn is_success(&self) -> bool {
        matches!(self, Self::New { .. } | Self::Updated { .. })
    }

    /// Check if file was skipped (unchanged or duplicate)
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Unchanged { .. } | Self::Duplicate { .. })
    }
}

/// Summary statistics for ingestion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct IngestSummary {
    /// Number of new files processed
    pub new_files: usize,
    /// Number of updated files
    pub updated_files: usize,
    /// Number of unchanged files (skipped)
    pub unchanged_files: usize,
    /// Number of duplicate files (skipped)
    pub duplicate_files: usize,
    /// Number of failed files
    pub failed_files: usize,
    /// Total chunks created
    pub total_chunks_created: u32,
    /// Total chunks deleted (from updates)
    pub total_chunks_deleted: usize,
}


impl IngestSummary {
    /// Create summary from list of file statuses
    pub fn from_statuses(statuses: &[FileIngestStatus]) -> Self {
        let mut summary = Self::default();

        for status in statuses {
            match status {
                FileIngestStatus::New { chunks_created, .. } => {
                    summary.new_files += 1;
                    summary.total_chunks_created += chunks_created;
                }
                FileIngestStatus::Updated { chunks_created, old_chunks_deleted, .. } => {
                    summary.updated_files += 1;
                    summary.total_chunks_created += chunks_created;
                    summary.total_chunks_deleted += old_chunks_deleted;
                }
                FileIngestStatus::Unchanged { .. } => {
                    summary.unchanged_files += 1;
                }
                FileIngestStatus::Duplicate { .. } => {
                    summary.duplicate_files += 1;
                }
                FileIngestStatus::Failed { .. } => {
                    summary.failed_files += 1;
                }
            }
        }

        summary
    }
}

/// V2 Response from document ingestion with detailed status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponseV2 {
    /// Whether any files were successfully processed
    pub success: bool,
    /// Status of each file
    pub files: Vec<FileIngestStatus>,
    /// Summary statistics
    pub summary: IngestSummary,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

impl IngestResponseV2 {
    /// Create a new V2 ingest response
    pub fn new(files: Vec<FileIngestStatus>, processing_time_ms: u64) -> Self {
        let summary = IngestSummary::from_statuses(&files);
        let success = summary.new_files > 0 || summary.updated_files > 0;

        Self {
            success,
            files,
            summary,
            processing_time_ms,
        }
    }
}

// ============ String Search Types ============

/// Result from string search (literal text matching)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringSearchResult {
    /// Chunk containing the match
    pub chunk_id: Uuid,
    /// Document ID
    pub document_id: Uuid,
    /// Source filename
    pub filename: String,
    /// File type
    pub file_type: FileType,
    /// Page number (if applicable)
    pub page_number: Option<u32>,
    /// Number of times the search term appears
    pub match_count: usize,
    /// Positions of matches in the chunk
    pub match_positions: Vec<usize>,
    /// Snippet with highlighted matches (<mark> tags)
    pub highlighted_snippet: String,
    /// Preview text around first match
    pub preview: String,
}

/// Response from string search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringSearchResponse {
    /// Search query
    pub query: String,
    /// Total number of matches across all documents
    pub total_matches: usize,
    /// Number of documents with matches
    pub documents_matched: usize,
    /// Search results (top 10)
    pub results: Vec<StringSearchResult>,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

impl StringSearchResponse {
    /// Create a new string search response
    pub fn new(query: String, results: Vec<StringSearchResult>, processing_time_ms: u64) -> Self {
        let total_matches: usize = results.iter().map(|r| r.match_count).sum();
        let unique_docs: std::collections::HashSet<Uuid> = results.iter().map(|r| r.document_id).collect();
        let documents_matched = unique_docs.len();

        Self {
            query,
            total_matches,
            documents_matched,
            results,
            processing_time_ms,
        }
    }
}

// ============ V2 API Response Types ============

/// Query response type for V2 API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryResponseType {
    /// Full RAG answer with LLM generation
    RagAnswer {
        chunks_retrieved: usize,
        chunks_used: usize,
    },
    /// Literal string search (no LLM)
    StringSearch {
        total_matches: usize,
        documents_matched: usize,
    },
    /// No relevant information found
    NotFound,
}

/// Relevance indicator for citations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceInfo {
    /// Relevance score (0-100)
    pub score: u32,
    /// Human-readable label
    pub label: String,
}

impl RelevanceInfo {
    /// Create relevance info from similarity score (0.0-1.0)
    pub fn from_similarity(similarity: f32) -> Self {
        let score = (similarity * 100.0).round() as u32;
        let label = match score {
            90..=100 => "Excellent",
            75..=89 => "High",
            50..=74 => "Medium",
            25..=49 => "Low",
            _ => "Weak",
        };
        Self {
            score,
            label: label.to_string(),
        }
    }
}

/// Response metrics for V2 API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetrics {
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    /// Confidence score (0-100)
    pub confidence: u32,
    /// Whether embedding was used
    pub used_embeddings: bool,
}

/// Cache info for V2 API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInfo {
    /// Whether response came from cache
    pub from_cache: bool,
    /// Cache hit count (if cached)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_count: Option<u32>,
}

/// V2 Citation with frontend-friendly structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationV2 {
    /// Citation index (1-based for display)
    pub index: u32,
    /// Source information
    pub source: SourceInfoV2,
    /// Snippet information
    pub snippet: SnippetInfoV2,
    /// Relevance information
    pub relevance: RelevanceInfo,
    /// Document links
    pub links: CitationLinks,
}

/// Source information for V2 citation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfoV2 {
    /// Document ID
    pub document_id: Uuid,
    /// Chunk ID
    pub chunk_id: Uuid,
    /// Display filename
    pub filename: String,
    /// File type
    pub file_type: String,
    /// Page number (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Line range for code files
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<String>,
    /// Section title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

/// Snippet information for V2 citation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetInfoV2 {
    /// Plain text snippet
    pub text: String,
    /// Highlighted snippet with <mark> tags
    pub highlighted: String,
    /// Preview text (shorter, for UI)
    pub preview: String,
}

/// Document links for V2 citation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationLinks {
    /// URL to original document (authenticated GCS)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    /// URL to extracted plain text (authenticated GCS)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plaintext: Option<String>,
}

impl CitationV2 {
    /// Create V2 citation from standard citation
    pub fn from_citation(citation: &Citation, index: u32) -> Self {
        let lines = match (citation.line_start, citation.line_end) {
            (Some(start), Some(end)) => Some(format!("{}-{}", start, end)),
            (Some(start), None) => Some(format!("{}", start)),
            _ => None,
        };

        // Create preview (first 100 chars)
        let preview = if citation.snippet.len() > 100 {
            format!("{}...", &citation.snippet[..100])
        } else {
            citation.snippet.clone()
        };

        Self {
            index,
            source: SourceInfoV2 {
                document_id: citation.document_id,
                chunk_id: citation.chunk_id,
                filename: citation.filename.clone(),
                file_type: format!("{:?}", citation.file_type).to_lowercase(),
                page: citation.page_number,
                lines,
                section: citation.section_title.clone(),
            },
            snippet: SnippetInfoV2 {
                text: citation.snippet.clone(),
                highlighted: citation.snippet_highlighted.clone(),
                preview,
            },
            relevance: RelevanceInfo::from_similarity(citation.similarity_score),
            links: CitationLinks {
                document: citation.document_url.clone(),
                plaintext: citation.plaintext_url.clone(),
            },
        }
    }
}

/// V2 Query Response (frontend-friendly format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponseV2 {
    /// Generated answer
    pub answer: String,
    /// Query type and metadata
    pub query_type: QueryResponseType,
    /// Citations in V2 format
    pub citations: Vec<CitationV2>,
    /// Response metrics
    pub metrics: ResponseMetrics,
    /// Cache information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_info: Option<CacheInfo>,
    /// Interaction ID for feedback
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<Uuid>,
}

impl QueryResponseV2 {
    /// Create V2 response from standard response
    pub fn from_response(response: &QueryResponse, used_embeddings: bool, cache_info: Option<CacheInfo>) -> Self {
        let query_type = if response.citations.is_empty() {
            QueryResponseType::NotFound
        } else {
            QueryResponseType::RagAnswer {
                chunks_retrieved: response.chunks_retrieved,
                chunks_used: response.chunks_used,
            }
        };

        let citations: Vec<CitationV2> = response
            .citations
            .iter()
            .enumerate()
            .map(|(i, c)| CitationV2::from_citation(c, (i + 1) as u32))
            .collect();

        Self {
            answer: response.answer.clone(),
            query_type,
            citations,
            metrics: ResponseMetrics {
                processing_time_ms: response.processing_time_ms,
                confidence: (response.confidence * 100.0).round() as u32,
                used_embeddings,
            },
            cache_info,
            interaction_id: response.interaction_id,
        }
    }

    /// Create V2 response for string search
    pub fn from_string_search(
        answer: String,
        results: &[StringSearchResult],
        processing_time_ms: u64,
    ) -> Self {
        let total_matches: usize = results.iter().map(|r| r.match_count).sum();
        let unique_docs: std::collections::HashSet<Uuid> = results.iter().map(|r| r.document_id).collect();

        let citations: Vec<CitationV2> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                CitationV2 {
                    index: (i + 1) as u32,
                    source: SourceInfoV2 {
                        document_id: r.document_id,
                        chunk_id: r.chunk_id,
                        filename: r.filename.clone(),
                        file_type: format!("{:?}", r.file_type).to_lowercase(),
                        page: r.page_number,
                        lines: None,
                        section: None,
                    },
                    snippet: SnippetInfoV2 {
                        text: r.preview.clone(),
                        highlighted: r.highlighted_snippet.clone(),
                        preview: if r.preview.len() > 100 {
                            format!("{}...", &r.preview[..100])
                        } else {
                            r.preview.clone()
                        },
                    },
                    relevance: RelevanceInfo {
                        score: 100, // Exact match
                        label: "Exact Match".to_string(),
                    },
                    links: CitationLinks {
                        document: None,
                        plaintext: None,
                    },
                }
            })
            .collect();

        Self {
            answer,
            query_type: QueryResponseType::StringSearch {
                total_matches,
                documents_matched: unique_docs.len(),
            },
            citations,
            metrics: ResponseMetrics {
                processing_time_ms,
                confidence: if results.is_empty() { 0 } else { 100 },
                used_embeddings: false,
            },
            cache_info: None,
            interaction_id: None,
        }
    }
}
