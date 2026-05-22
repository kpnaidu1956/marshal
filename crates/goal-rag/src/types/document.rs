//! Document and chunk types with source tracking for citations

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Supported file types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    /// PDF document
    Pdf,
    /// Microsoft Word document (.docx)
    Docx,
    /// Old Microsoft Word document (.doc) - requires LibreOffice
    Doc,
    /// Microsoft PowerPoint presentation (.pptx)
    Pptx,
    /// Old Microsoft PowerPoint (.ppt) - requires LibreOffice
    Ppt,
    /// Plain text file
    Txt,
    /// Markdown file
    Markdown,
    /// Excel spreadsheet (.xlsx)
    Xlsx,
    /// Old Excel spreadsheet (.xls) - requires LibreOffice for complex files
    Xls,
    /// HTML document
    Html,
    /// CSV file
    Csv,
    /// Rich Text Format
    Rtf,
    /// OpenDocument Text
    Odt,
    /// OpenDocument Presentation
    Odp,
    /// OpenDocument Spreadsheet
    Ods,
    /// EPUB ebook
    Epub,
    /// Image (for OCR) - requires tesseract
    Image,
    /// Source code file with language
    Code(String),
    /// Unknown file type
    Unknown,
}

impl FileType {
    /// Detect file type from magic bytes (file signature)
    /// Used as fallback when extension is missing or unreliable
    pub fn from_magic_bytes(data: &[u8]) -> Self {
        if data.len() < 8 {
            return Self::Unknown;
        }

        // PDF: %PDF
        if data.starts_with(b"%PDF") {
            return Self::Pdf;
        }

        // ZIP-based formats (DOCX, XLSX, PPTX, ODT, EPUB)
        if data.starts_with(&[0x50, 0x4B, 0x03, 0x04]) || data.starts_with(&[0x50, 0x4B, 0x05, 0x06]) {
            // Need to check internal structure for specific format
            if let Ok(content) = std::str::from_utf8(&data[..std::cmp::min(data.len(), 2000)]) {
                if content.contains("word/") || content.contains("word\\") {
                    return Self::Docx;
                }
                if content.contains("xl/") || content.contains("xl\\") {
                    return Self::Xlsx;
                }
                if content.contains("ppt/") || content.contains("ppt\\") {
                    return Self::Pptx;
                }
                if content.contains("mimetype") && content.contains("opendocument") {
                    return Self::Odt;
                }
                if content.contains("EPUB") || content.contains("epub") {
                    return Self::Epub;
                }
            }
            // Generic Office Open XML - assume docx
            return Self::Docx;
        }

        // Microsoft Compound File Binary (DOC, XLS, PPT)
        if data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
            // Could be DOC, XLS, or PPT - assume DOC for text content
            return Self::Doc;
        }

        // RTF: {\rtf
        if data.starts_with(b"{\\rtf") {
            return Self::Rtf;
        }

        // HTML: <!DOCTYPE or <html
        if data.starts_with(b"<!DOCTYPE") || data.starts_with(b"<!doctype") ||
           data.starts_with(b"<html") || data.starts_with(b"<HTML") {
            return Self::Html;
        }

        // XML (could be various formats)
        if data.starts_with(b"<?xml") {
            return Self::Html; // Treat as HTML for now
        }

        // PNG
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Self::Image;
        }

        // JPEG
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Self::Image;
        }

        // GIF
        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            return Self::Image;
        }

        // Check if it looks like plain text (UTF-8 or ASCII)
        if Self::is_likely_text(data) {
            // Check for markdown indicators
            if let Ok(text) = std::str::from_utf8(&data[..std::cmp::min(data.len(), 1000)]) {
                if text.contains("# ") || text.contains("## ") || text.contains("```") ||
                   text.contains("**") || text.contains("- [") {
                    return Self::Markdown;
                }
                // Check for CSV (has commas and newlines in structured way)
                let lines: Vec<&str> = text.lines().take(5).collect();
                if lines.len() > 1 {
                    let comma_counts: Vec<usize> = lines.iter().map(|l| l.matches(',').count()).collect();
                    if comma_counts.iter().all(|&c| c > 0) && comma_counts.windows(2).all(|w| w[0] == w[1]) {
                        return Self::Csv;
                    }
                }
            }
            return Self::Txt;
        }

        Self::Unknown
    }

    /// Check if data looks like plain text
    fn is_likely_text(data: &[u8]) -> bool {
        let sample = &data[..std::cmp::min(data.len(), 512)];
        let printable = sample.iter().filter(|&&b| {
            b == 0x09 || b == 0x0A || b == 0x0D || (0x20..=0x7E).contains(&b) || b >= 0x80
        }).count();
        // If >90% of bytes are printable/text-like, it's probably text
        printable > sample.len() * 9 / 10
    }

    /// Detect file type from extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "pdf" => Self::Pdf,
            "docx" => Self::Docx,
            "doc" => Self::Doc,
            "pptx" => Self::Pptx,
            "ppt" => Self::Ppt,
            "txt" | "text" => Self::Txt,
            "md" | "markdown" => Self::Markdown,
            "xlsx" => Self::Xlsx,
            "xls" => Self::Xls,
            "html" | "htm" => Self::Html,
            "csv" => Self::Csv,
            "rtf" => Self::Rtf,
            "odt" => Self::Odt,
            "odp" => Self::Odp,
            "ods" => Self::Ods,
            "epub" => Self::Epub,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif" => Self::Image,
            // Code files
            "rs" => Self::Code("rust".to_string()),
            "py" => Self::Code("python".to_string()),
            "js" => Self::Code("javascript".to_string()),
            "ts" => Self::Code("typescript".to_string()),
            "tsx" | "jsx" => Self::Code("react".to_string()),
            "go" => Self::Code("go".to_string()),
            "java" => Self::Code("java".to_string()),
            "cpp" | "cc" | "cxx" => Self::Code("cpp".to_string()),
            "c" | "h" => Self::Code("c".to_string()),
            "cs" => Self::Code("csharp".to_string()),
            "rb" => Self::Code("ruby".to_string()),
            "php" => Self::Code("php".to_string()),
            "swift" => Self::Code("swift".to_string()),
            "kt" | "kts" => Self::Code("kotlin".to_string()),
            "sql" => Self::Code("sql".to_string()),
            "sh" | "bash" => Self::Code("bash".to_string()),
            "yaml" | "yml" => Self::Code("yaml".to_string()),
            "json" => Self::Code("json".to_string()),
            "xml" => Self::Code("xml".to_string()),
            "toml" => Self::Code("toml".to_string()),
            _ => Self::Unknown,
        }
    }

    /// Check if this is a supported file type
    /// Note: Some formats require external tools (LibreOffice, tesseract)
    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// Check if this format requires external tools
    pub fn requires_external_tools(&self) -> bool {
        matches!(self, Self::Doc | Self::Ppt | Self::Xls | Self::Image | Self::Rtf | Self::Odt | Self::Odp | Self::Ods | Self::Epub)
    }

    /// Get display name
    pub fn display_name(&self) -> &str {
        match self {
            Self::Pdf => "PDF",
            Self::Docx => "Word Document (.docx)",
            Self::Doc => "Word Document (.doc)",
            Self::Pptx => "PowerPoint (.pptx)",
            Self::Ppt => "PowerPoint (.ppt)",
            Self::Txt => "Text File",
            Self::Markdown => "Markdown",
            Self::Xlsx => "Excel Spreadsheet (.xlsx)",
            Self::Xls => "Excel Spreadsheet (.xls)",
            Self::Html => "HTML",
            Self::Csv => "CSV",
            Self::Rtf => "Rich Text Format",
            Self::Odt => "OpenDocument Text",
            Self::Odp => "OpenDocument Presentation",
            Self::Ods => "OpenDocument Spreadsheet",
            Self::Epub => "EPUB eBook",
            Self::Image => "Image",
            Self::Code(lang) => lang.as_str(),
            Self::Unknown => "Unknown",
        }
    }

    /// Get reason why file type is not supported
    pub fn unsupported_reason(&self) -> Option<&str> {
        match self {
            Self::Unknown => Some("Unknown file type."),
            _ => None,
        }
    }

    /// Get required tools for this file type
    pub fn required_tools(&self) -> Option<&str> {
        match self {
            Self::Doc => Some("LibreOffice (libreoffice --headless)"),
            Self::Ppt => Some("LibreOffice (libreoffice --headless)"),
            Self::Xls => Some("LibreOffice (libreoffice --headless) or calamine fallback"),
            Self::Rtf => Some("pandoc or LibreOffice"),
            Self::Odt | Self::Odp | Self::Ods => Some("pandoc or LibreOffice"),
            Self::Epub => Some("pandoc"),
            Self::Image => Some("tesseract OCR (apt install tesseract-ocr)"),
            Self::Pdf => Some("poppler-utils (pdftotext) for complex PDFs, tesseract for scanned PDFs"),
            _ => None,
        }
    }
}

/// A document that has been ingested
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique document ID
    pub id: Uuid,
    /// Organization ID for multi-tenancy (optional for backwards compatibility)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    /// Original filename as uploaded by user
    pub filename: String,
    /// Internal filename (may differ if file was converted)
    #[serde(default)]
    pub internal_filename: Option<String>,
    /// File type
    pub file_type: FileType,
    /// Content hash for deduplication
    pub content_hash: String,
    /// Total number of pages (if applicable)
    pub total_pages: Option<u32>,
    /// Total number of chunks created
    pub total_chunks: u32,
    /// File size in bytes
    pub file_size: u64,
    /// Ingestion timestamp
    pub ingested_at: chrono::DateTime<chrono::Utc>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Document {
    /// Create a new document with original filename
    pub fn new(original_filename: String, file_type: FileType, content_hash: String, file_size: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id: None,
            filename: original_filename,
            internal_filename: None,
            file_type,
            content_hash,
            total_pages: None,
            total_chunks: 0,
            file_size,
            ingested_at: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create a new document with organization for multi-tenancy
    pub fn new_with_organization(
        original_filename: String,
        file_type: FileType,
        content_hash: String,
        file_size: u64,
        organization_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id,
            filename: original_filename,
            internal_filename: None,
            file_type,
            content_hash,
            total_pages: None,
            total_chunks: 0,
            file_size,
            ingested_at: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create a new document with both original and internal filename
    pub fn new_with_internal(
        original_filename: String,
        internal_filename: String,
        file_type: FileType,
        content_hash: String,
        file_size: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id: None,
            filename: original_filename,
            internal_filename: Some(internal_filename),
            file_type,
            content_hash,
            total_pages: None,
            total_chunks: 0,
            file_size,
            ingested_at: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Source information for a chunk (used for citations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSource {
    /// Original filename as uploaded (used in citations)
    pub filename: String,
    /// Internal filename if converted (for debugging)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_filename: Option<String>,
    /// File type
    pub file_type: FileType,
    /// Page number (1-indexed, for PDF/DOCX)
    pub page_number: Option<u32>,
    /// Total pages in document
    pub page_count: Option<u32>,
    /// Section or heading title
    pub section_title: Option<String>,
    /// Heading hierarchy (e.g., ["Chapter 1", "Section 1.2"])
    pub heading_hierarchy: Vec<String>,
    /// Sheet name (for Excel)
    pub sheet_name: Option<String>,
    /// Row range (for spreadsheets)
    pub row_range: Option<(u32, u32)>,
    /// Line numbers (for code files)
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    /// Function or class name (for code files)
    pub code_context: Option<String>,
}

impl ChunkSource {
    /// Create source info for a text file
    pub fn text(filename: String) -> Self {
        Self {
            filename,
            internal_filename: None,
            file_type: FileType::Txt,
            page_number: None,
            page_count: None,
            section_title: None,
            heading_hierarchy: Vec::new(),
            sheet_name: None,
            row_range: None,
            line_start: None,
            line_end: None,
            code_context: None,
        }
    }

    /// Create source info for a PDF
    pub fn pdf(filename: String, page: u32, total_pages: u32) -> Self {
        Self {
            filename,
            internal_filename: None,
            file_type: FileType::Pdf,
            page_number: Some(page),
            page_count: Some(total_pages),
            section_title: None,
            heading_hierarchy: Vec::new(),
            sheet_name: None,
            row_range: None,
            line_start: None,
            line_end: None,
            code_context: None,
        }
    }

    /// Create source info for code
    pub fn code(filename: String, language: String, line_start: u32, line_end: u32) -> Self {
        Self {
            filename,
            internal_filename: None,
            file_type: FileType::Code(language),
            page_number: None,
            page_count: None,
            section_title: None,
            heading_hierarchy: Vec::new(),
            sheet_name: None,
            row_range: None,
            line_start: Some(line_start),
            line_end: Some(line_end),
            code_context: None,
        }
    }

    /// Format source for display
    pub fn format_citation(&self) -> String {
        let mut parts = vec![self.filename.clone()];

        if let Some(page) = self.page_number {
            parts.push(format!("Page {}", page));
        }

        if let Some(sheet) = &self.sheet_name {
            parts.push(format!("Sheet: {}", sheet));
        }

        if let (Some(start), Some(end)) = (self.line_start, self.line_end) {
            parts.push(format!("Lines {}-{}", start, end));
        }

        if let Some(section) = &self.section_title {
            parts.push(format!("Section: {}", section));
        }

        parts.join(", ")
    }
}

/// A chunk of text from a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Unique chunk ID
    pub id: Uuid,
    /// Parent document ID
    pub document_id: Uuid,
    /// Text content
    pub content: String,
    /// Embedding vector (384 or 768 dimensions)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub embedding: Vec<f32>,
    /// Source information for citations
    pub source: ChunkSource,
    /// Character position in original document
    pub char_start: usize,
    pub char_end: usize,
    /// Chunk index within document
    pub chunk_index: u32,
    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Chunk {
    /// Create a new chunk
    pub fn new(
        document_id: Uuid,
        content: String,
        source: ChunkSource,
        char_start: usize,
        char_end: usize,
        chunk_index: u32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            document_id,
            content,
            embedding: Vec::new(),
            source,
            char_start,
            char_end,
            chunk_index,
            metadata: HashMap::new(),
        }
    }

    /// Convert to vector metadata for storage
    pub fn to_vector_metadata(&self) -> HashMap<String, serde_json::Value> {
        let mut meta = HashMap::new();
        meta.insert("chunk_id".to_string(), serde_json::json!(self.id.to_string()));
        meta.insert("document_id".to_string(), serde_json::json!(self.document_id.to_string()));
        meta.insert("filename".to_string(), serde_json::json!(self.source.filename));
        meta.insert("file_type".to_string(), serde_json::json!(self.source.file_type));
        meta.insert("chunk_index".to_string(), serde_json::json!(self.chunk_index));
        meta.insert("char_start".to_string(), serde_json::json!(self.char_start));
        meta.insert("char_end".to_string(), serde_json::json!(self.char_end));
        meta.insert("content".to_string(), serde_json::json!(self.content));

        if let Some(page) = self.source.page_number {
            meta.insert("page_number".to_string(), serde_json::json!(page));
        }

        if let Some(section) = &self.source.section_title {
            meta.insert("section_title".to_string(), serde_json::json!(section));
        }

        if let (Some(start), Some(end)) = (self.source.line_start, self.source.line_end) {
            meta.insert("line_start".to_string(), serde_json::json!(start));
            meta.insert("line_end".to_string(), serde_json::json!(end));
        }

        meta
    }
}
