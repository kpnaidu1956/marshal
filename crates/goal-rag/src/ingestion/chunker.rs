//! Text chunking with page and position tracking

use unicode_segmentation::UnicodeSegmentation;

use crate::types::{Chunk, ChunkSource, Document, FileType};
use super::parser::ParsedDocument;

/// Text chunker with configurable size and overlap
pub struct TextChunker {
    /// Target chunk size in characters
    chunk_size: usize,
    /// Overlap between chunks
    overlap: usize,
    /// Minimum chunk size
    min_size: usize,
}

impl TextChunker {
    /// Create a new chunker
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        Self {
            chunk_size,
            overlap,
            min_size: 50,
        }
    }

    /// Chunk a parsed document
    pub fn chunk_document(&self, doc: &Document, parsed: &ParsedDocument) -> Vec<Chunk> {
        let mut chunks = Vec::new();

        // For page-aware documents
        if !parsed.pages.is_empty() && parsed.pages.len() > 1 {
            for page in &parsed.pages {
                let page_chunks = self.chunk_text_with_source(
                    &page.content,
                    doc,
                    Some(page.page_number),
                    parsed.total_pages,
                    page.char_offset,
                    chunks.len() as u32,
                );
                chunks.extend(page_chunks);
            }
        } else {
            // Single-page or non-paginated documents
            chunks = self.chunk_text_with_source(
                &parsed.content,
                doc,
                parsed.pages.first().map(|p| p.page_number),
                parsed.total_pages,
                0,
                0,
            );
        }

        chunks
    }

    /// Chunk text with source information
    fn chunk_text_with_source(
        &self,
        text: &str,
        doc: &Document,
        page_number: Option<u32>,
        page_count: Option<u32>,
        base_offset: usize,
        start_index: u32,
    ) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let sentences = self.split_into_sentences(text);

        let mut current_chunk = String::new();
        let mut current_start = 0usize;
        let mut chunk_index = start_index;
        let mut char_pos = 0usize;

        for sentence in sentences {
            let sentence_len = sentence.len();

            // If adding this sentence exceeds chunk size, save current chunk
            if !current_chunk.is_empty()
                && current_chunk.len() + sentence_len > self.chunk_size
            {
                if current_chunk.len() >= self.min_size {
                    let source = self.create_source(
                        doc,
                        page_number,
                        page_count,
                        current_start,
                        char_pos,
                    );

                    chunks.push(Chunk::new(
                        doc.id,
                        current_chunk.trim().to_string(),
                        source,
                        base_offset + current_start,
                        base_offset + char_pos,
                        chunk_index,
                    ));

                    chunk_index += 1;
                }

                // Start new chunk with overlap
                let overlap_text = self.get_overlap_text(&current_chunk);
                current_chunk = overlap_text;
                current_start = char_pos.saturating_sub(self.overlap);
            }

            current_chunk.push_str(sentence);
            char_pos += sentence_len;
        }

        // Save final chunk
        if current_chunk.len() >= self.min_size {
            let source = self.create_source(
                doc,
                page_number,
                page_count,
                current_start,
                char_pos,
            );

            chunks.push(Chunk::new(
                doc.id,
                current_chunk.trim().to_string(),
                source,
                base_offset + current_start,
                base_offset + char_pos,
                chunk_index,
            ));
        }

        chunks
    }

    /// Split text into sentences
    fn split_into_sentences<'a>(&self, text: &'a str) -> Vec<&'a str> {
        // Use unicode segmentation for proper sentence boundaries
        text.split_sentence_bounds().collect()
    }

    /// Get overlap text from the end of a chunk
    fn get_overlap_text(&self, text: &str) -> String {
        if text.len() <= self.overlap {
            return text.to_string();
        }

        // Find a good break point (sentence or word boundary)
        let mut start = text.len().saturating_sub(self.overlap);

        // Ensure we're at a valid UTF-8 character boundary
        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }

        let overlap_text = &text[start..];

        // Try to start at a sentence boundary
        if let Some(pos) = overlap_text.find(". ") {
            return overlap_text[pos + 2..].to_string();
        }

        // Fall back to word boundary
        if let Some(pos) = overlap_text.find(' ') {
            return overlap_text[pos + 1..].to_string();
        }

        overlap_text.to_string()
    }

    /// Create source information for a chunk
    fn create_source(
        &self,
        doc: &Document,
        page_number: Option<u32>,
        page_count: Option<u32>,
        _char_start: usize,
        _char_end: usize,
    ) -> ChunkSource {
        let mut source = ChunkSource {
            filename: doc.filename.clone(),  // Original filename for citations
            internal_filename: doc.internal_filename.clone(),  // Internal filename for debugging
            file_type: doc.file_type.clone(),
            page_number,
            page_count,
            section_title: None,
            heading_hierarchy: Vec::new(),
            sheet_name: None,
            row_range: None,
            line_start: None,
            line_end: None,
            code_context: None,
        };

        // For code files, calculate line numbers
        if let FileType::Code(_) = &doc.file_type {
            // This is a simplified calculation
            // In production, track line numbers during chunking
            source.line_start = Some(1);
            source.line_end = Some(1);
        }

        source
    }
}

/// Chunk code files with function/class awareness
pub struct CodeChunker {
    base: TextChunker,
}

impl CodeChunker {
    /// Create a new code chunker
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        Self {
            base: TextChunker::new(chunk_size, overlap),
        }
    }

    /// Chunk code by functions/methods when possible
    pub fn chunk_code(&self, doc: &Document, content: &str, language: &str) -> Vec<Chunk> {
        // For now, use simple line-based chunking
        // In production, use tree-sitter for syntax-aware chunking

        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_start_line = 1u32;
        let mut chunk_index = 0u32;
        let mut char_offset = 0usize;

        for (line_num, line) in lines.iter().enumerate() {
            let line_with_newline = format!("{}\n", line);

            if !current_chunk.is_empty()
                && current_chunk.len() + line_with_newline.len() > self.base.chunk_size
            {
                let char_start = char_offset - current_chunk.len();

                let mut source = ChunkSource::code(
                    doc.filename.clone(),  // Original filename for citations
                    language.to_string(),
                    current_start_line,
                    line_num as u32,
                );
                source.internal_filename = doc.internal_filename.clone();

                chunks.push(Chunk::new(
                    doc.id,
                    current_chunk.trim().to_string(),
                    source,
                    char_start,
                    char_offset,
                    chunk_index,
                ));

                chunk_index += 1;
                current_chunk = String::new();
                current_start_line = line_num as u32 + 1;
            }

            current_chunk.push_str(&line_with_newline);
            char_offset += line_with_newline.len();
        }

        // Final chunk
        if !current_chunk.trim().is_empty() {
            let char_start = char_offset - current_chunk.len();

            let mut source = ChunkSource::code(
                doc.filename.clone(),  // Original filename for citations
                language.to_string(),
                current_start_line,
                lines.len() as u32,
            );
            source.internal_filename = doc.internal_filename.clone();

            chunks.push(Chunk::new(
                doc.id,
                current_chunk.trim().to_string(),
                source,
                char_start,
                char_offset,
                chunk_index,
            ));
        }

        chunks
    }
}
