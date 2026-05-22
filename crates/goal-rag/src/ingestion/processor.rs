//! Ingestion pipeline orchestration

use crate::error::Result;
use crate::types::{Chunk, Document, FileType};

use super::chunker::{CodeChunker, TextChunker};
use super::parser::{FileParser, ParsedDocument};

/// Main ingestion pipeline
pub struct IngestPipeline {
    /// Text chunker
    chunker: TextChunker,
    /// Code chunker
    code_chunker: CodeChunker,
}

impl IngestPipeline {
    /// Create a new ingestion pipeline
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self {
            chunker: TextChunker::new(chunk_size, chunk_overlap),
            code_chunker: CodeChunker::new(chunk_size, chunk_overlap),
        }
    }

    /// Parse a file
    pub fn parse_file(&self, filename: &str, data: &[u8]) -> Result<ParsedDocument> {
        FileParser::parse(filename, data)
    }

    /// Create chunks from a parsed document
    pub fn create_chunks(&self, doc: &Document, parsed: &ParsedDocument) -> Result<Vec<Chunk>> {
        let chunks = match &doc.file_type {
            FileType::Code(language) => {
                self.code_chunker.chunk_code(doc, &parsed.content, language)
            }
            _ => self.chunker.chunk_document(doc, parsed),
        };

        Ok(chunks)
    }

    /// Full ingestion: parse + chunk
    pub fn ingest(&self, filename: &str, data: &[u8]) -> Result<(Document, Vec<Chunk>)> {
        let parsed = self.parse_file(filename, data)?;

        let mut doc = Document::new(
            filename.to_string(),
            parsed.file_type.clone(),
            parsed.content_hash.clone(),
            data.len() as u64,
        );
        doc.total_pages = parsed.total_pages;

        let chunks = self.create_chunks(&doc, &parsed)?;
        doc.total_chunks = chunks.len() as u32;

        Ok((doc, chunks))
    }
}

impl Default for IngestPipeline {
    fn default() -> Self {
        Self::new(512, 50)
    }
}
