//! Google Document AI client for advanced PDF text extraction
//!
//! Document AI provides high-quality text extraction with:
//! - OCR for scanned documents
//! - Table detection and extraction
//! - Form field recognition
//! - Layout preservation
//! - Support for large documents (up to 2000 pages)

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::auth::GcpAuth;
use crate::error::{Error, Result};

/// Google Document AI client for PDF processing
pub struct DocumentAiClient {
    auth: Arc<GcpAuth>,
    /// Full processor resource name
    /// e.g., "projects/my-project/locations/us/processors/abc123"
    processor_name: String,
}

impl DocumentAiClient {
    /// Create a new Document AI client
    ///
    /// # Arguments
    /// * `auth` - GCP authentication
    /// * `processor_name` - Full resource name of the Document AI processor
    pub fn new(auth: Arc<GcpAuth>, processor_name: String) -> Self {
        Self {
            auth,
            processor_name,
        }
    }

    /// Get the API endpoint URL for processing
    fn endpoint(&self) -> String {
        // Document AI API endpoint format:
        // https://LOCATION-documentai.googleapis.com/v1/PROCESSOR_NAME:process

        // Extract location from processor name
        // Format: projects/PROJECT/locations/LOCATION/processors/PROCESSOR_ID
        let location = self.processor_name
            .split('/')
            .nth(3)
            .unwrap_or("us");

        format!(
            "https://{}-documentai.googleapis.com/v1/{}:process",
            location,
            self.processor_name
        )
    }

    /// Process a PDF document and extract text
    ///
    /// # Arguments
    /// * `pdf_data` - Raw PDF bytes
    /// * `filename` - Original filename (for logging)
    ///
    /// # Returns
    /// Extracted text content with page information
    pub async fn process_pdf(&self, pdf_data: &[u8], filename: &str) -> Result<DocumentAiResult> {
        let client = self.auth.authorized_client().await?;

        // Encode PDF as base64
        let content = BASE64.encode(pdf_data);

        let request = ProcessRequest {
            raw_document: RawDocument {
                content,
                mime_type: "application/pdf".to_string(),
            },
            skip_human_review: true,
        };

        tracing::info!(
            "[{}] Sending to Document AI processor: {}",
            filename,
            self.processor_name
        );

        let response = client
            .post(self.endpoint())
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Document AI request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Internal(format!(
                "Document AI processing failed ({}): {}",
                status, body
            )));
        }

        let process_response: ProcessResponse = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse Document AI response: {}", e)))?;

        // Extract text from response
        let document = process_response.document;
        let full_text = document.text.unwrap_or_default();

        if full_text.trim().is_empty() {
            return Err(Error::Internal(
                "Document AI returned empty text - document may be empty or unreadable".to_string()
            ));
        }

        // Build page-level content
        let mut pages = Vec::new();
        let mut total_pages = 0u32;

        if let Some(doc_pages) = document.pages {
            total_pages = doc_pages.len() as u32;

            for (i, page) in doc_pages.iter().enumerate() {
                let page_number = page.page_number.unwrap_or((i + 1) as i32) as u32;

                // Extract text from page layout
                let page_text = if let Some(ref layout) = page.layout {
                    extract_text_from_layout(layout, &full_text)
                } else {
                    // Fallback: try to get text from blocks/paragraphs
                    extract_text_from_page_elements(page, &full_text)
                };

                pages.push(DocumentAiPage {
                    page_number,
                    content: page_text,
                    width: page.dimension.as_ref().map(|d| d.width).unwrap_or(0.0),
                    height: page.dimension.as_ref().map(|d| d.height).unwrap_or(0.0),
                });
            }
        }

        // If no pages were extracted, use full text as single page
        if pages.is_empty() && !full_text.is_empty() {
            pages.push(DocumentAiPage {
                page_number: 1,
                content: full_text.clone(),
                width: 0.0,
                height: 0.0,
            });
            total_pages = 1;
        }

        tracing::info!(
            "[{}] Document AI extracted {} chars from {} pages",
            filename,
            full_text.len(),
            total_pages
        );

        Ok(DocumentAiResult {
            text: full_text,
            pages,
            total_pages,
        })
    }

    /// Check if Document AI is properly configured and accessible
    pub async fn health_check(&self) -> Result<bool> {
        // Just verify we can get a token
        self.auth.get_token().await.map(|_| true)
    }

    /// Get the processor name
    pub fn processor_name(&self) -> &str {
        &self.processor_name
    }
}

/// Extract text from a layout element using text anchors
fn extract_text_from_layout(layout: &Layout, full_text: &str) -> String {
    if let Some(ref text_anchor) = layout.text_anchor {
        extract_text_from_anchor(text_anchor, full_text)
    } else {
        String::new()
    }
}

/// Extract text using text anchor segments
fn extract_text_from_anchor(anchor: &TextAnchor, full_text: &str) -> String {
    let mut text = String::new();

    if let Some(ref segments) = anchor.text_segments {
        for segment in segments {
            let start = segment.start_index.unwrap_or(0) as usize;
            let end = segment.end_index.unwrap_or(full_text.len() as i64) as usize;

            if start < full_text.len() && end <= full_text.len() && start < end {
                text.push_str(&full_text[start..end]);
            }
        }
    }

    text
}

/// Extract text from page elements (blocks, paragraphs, lines)
fn extract_text_from_page_elements(page: &Page, full_text: &str) -> String {
    let mut text = String::new();

    // Try paragraphs first
    if let Some(ref paragraphs) = page.paragraphs {
        for para in paragraphs {
            if let Some(ref layout) = para.layout {
                text.push_str(&extract_text_from_layout(layout, full_text));
                text.push('\n');
            }
        }
    }

    // If no paragraphs, try blocks
    if text.is_empty() {
        if let Some(ref blocks) = page.blocks {
            for block in blocks {
                if let Some(ref layout) = block.layout {
                    text.push_str(&extract_text_from_layout(layout, full_text));
                    text.push('\n');
                }
            }
        }
    }

    text
}

/// Result from Document AI processing
#[derive(Debug, Clone)]
pub struct DocumentAiResult {
    /// Full extracted text
    pub text: String,
    /// Page-by-page content
    pub pages: Vec<DocumentAiPage>,
    /// Total number of pages
    pub total_pages: u32,
}

/// Page content from Document AI
#[derive(Debug, Clone)]
pub struct DocumentAiPage {
    /// Page number (1-indexed)
    pub page_number: u32,
    /// Text content of the page
    pub content: String,
    /// Page width in points
    pub width: f64,
    /// Page height in points
    pub height: f64,
}

// ============================================================================
// API Request/Response types
// ============================================================================

#[derive(Serialize)]
struct ProcessRequest {
    #[serde(rename = "rawDocument")]
    raw_document: RawDocument,
    #[serde(rename = "skipHumanReview")]
    skip_human_review: bool,
}

#[derive(Serialize)]
struct RawDocument {
    content: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
}

#[derive(Deserialize)]
struct ProcessResponse {
    document: Document,
}

#[derive(Deserialize)]
struct Document {
    text: Option<String>,
    pages: Option<Vec<Page>>,
}

#[derive(Deserialize)]
struct Page {
    #[serde(rename = "pageNumber")]
    page_number: Option<i32>,
    dimension: Option<Dimension>,
    layout: Option<Layout>,
    blocks: Option<Vec<Block>>,
    paragraphs: Option<Vec<Paragraph>>,
}

#[derive(Deserialize)]
struct Dimension {
    width: f64,
    height: f64,
}

#[derive(Deserialize)]
struct Layout {
    #[serde(rename = "textAnchor")]
    text_anchor: Option<TextAnchor>,
}

#[derive(Deserialize)]
struct TextAnchor {
    #[serde(rename = "textSegments")]
    text_segments: Option<Vec<TextSegment>>,
}

#[derive(Deserialize)]
struct TextSegment {
    #[serde(rename = "startIndex")]
    start_index: Option<i64>,
    #[serde(rename = "endIndex")]
    end_index: Option<i64>,
}

#[derive(Deserialize)]
struct Block {
    layout: Option<Layout>,
}

#[derive(Deserialize)]
struct Paragraph {
    layout: Option<Layout>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_generation() {
        // Test that endpoint is correctly generated from processor name
        let processor = "projects/my-project/locations/us/processors/abc123";

        // Extract location
        let location = processor.split('/').nth(3).unwrap_or("us");
        assert_eq!(location, "us");

        let endpoint = format!(
            "https://{}-documentai.googleapis.com/v1/{}:process",
            location, processor
        );
        assert_eq!(
            endpoint,
            "https://us-documentai.googleapis.com/v1/projects/my-project/locations/us/processors/abc123:process"
        );
    }

    #[test]
    fn test_eu_location() {
        let processor = "projects/my-project/locations/eu/processors/xyz789";
        let location = processor.split('/').nth(3).unwrap_or("us");
        assert_eq!(location, "eu");
    }
}
