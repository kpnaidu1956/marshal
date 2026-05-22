//! Multi-format file parser

use calamine::Reader;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::types::FileType;

/// Common Unicode glyph name mappings for PDF fonts
/// Maps glyph names like "uni2010" to their Unicode characters
fn get_unicode_glyph_map() -> HashMap<&'static str, char> {
    let mut map = HashMap::new();
    // Hyphens and dashes
    map.insert("uni2010", '\u{2010}'); // Hyphen
    map.insert("uni2011", '\u{2011}'); // Non-breaking hyphen
    map.insert("uni2012", '\u{2012}'); // Figure dash
    map.insert("uni2013", '\u{2013}'); // En dash
    map.insert("uni2014", '\u{2014}'); // Em dash
    map.insert("uni2015", '\u{2015}'); // Horizontal bar
    // Quotation marks
    map.insert("uni2018", '\u{2018}'); // Left single quote
    map.insert("uni2019", '\u{2019}'); // Right single quote (apostrophe)
    map.insert("uni201A", '\u{201A}'); // Single low-9 quote
    map.insert("uni201C", '\u{201C}'); // Left double quote
    map.insert("uni201D", '\u{201D}'); // Right double quote
    map.insert("uni201E", '\u{201E}'); // Double low-9 quote
    // Bullets and symbols
    map.insert("uni2022", '\u{2022}'); // Bullet
    map.insert("uni2026", '\u{2026}'); // Ellipsis
    map.insert("uni2030", '\u{2030}'); // Per mille
    // Spaces
    map.insert("uni00A0", '\u{00A0}'); // Non-breaking space
    map.insert("uni2002", '\u{2002}'); // En space
    map.insert("uni2003", '\u{2003}'); // Em space
    map.insert("uni2009", '\u{2009}'); // Thin space
    // Mathematical
    map.insert("uni2212", '\u{2212}'); // Minus sign
    map.insert("uni00D7", '\u{00D7}'); // Multiplication sign
    map.insert("uni00F7", '\u{00F7}'); // Division sign
    // Currency
    map.insert("uni20AC", '\u{20AC}'); // Euro
    map.insert("uni00A3", '\u{00A3}'); // Pound
    map.insert("uni00A5", '\u{00A5}'); // Yen
    // Other common
    map.insert("uni00AE", '\u{00AE}'); // Registered trademark
    map.insert("uni2122", '\u{2122}'); // Trademark
    map.insert("uni00A9", '\u{00A9}'); // Copyright
    // Ligatures - common glyph names
    map.insert("fi", '\u{FB01}'); // fi ligature
    map.insert("fl", '\u{FB02}'); // fl ligature
    map.insert("ff", '\u{FB00}'); // ff ligature
    map.insert("ffi", '\u{FB03}'); // ffi ligature
    map.insert("ffl", '\u{FB04}'); // ffl ligature
    // Ligatures - alternate names
    map.insert("f_i", '\u{FB01}');
    map.insert("f_l", '\u{FB02}');
    map.insert("f_f", '\u{FB00}');
    map.insert("f_f_i", '\u{FB03}');
    map.insert("f_f_l", '\u{FB04}');
    // Latin extended chars that appear in fonts
    map.insert("uni0131", '\u{0131}'); // Dotless i
    map.insert("uni0152", '\u{0152}'); // OE ligature
    map.insert("uni0153", '\u{0153}'); // oe ligature
    map.insert("uni0160", '\u{0160}'); // S caron
    map.insert("uni0161", '\u{0161}'); // s caron
    map.insert("uni0178", '\u{0178}'); // Y diaeresis
    map.insert("uni017D", '\u{017D}'); // Z caron
    map.insert("uni017E", '\u{017E}'); // z caron
    map
}

/// Clean up PDF text by replacing Unicode glyph names with actual characters
fn cleanup_pdf_text(text: &str) -> String {
    let glyph_map = get_unicode_glyph_map();
    let mut result = text.to_string();

    // Replace common problematic characters
    for (glyph_name, char_value) in &glyph_map {
        // Look for glyph references in various formats
        let patterns = [
            format!("({})", glyph_name),
            format!("<{}>", glyph_name),
            glyph_name.to_string(),
        ];
        for pattern in &patterns {
            result = result.replace(pattern, &char_value.to_string());
        }
    }

    // Replace common ASCII approximations
    result = result
        .replace(['\u{2010}', '\u{2011}', '\u{2013}'], "-")  // En dash -> hyphen
        .replace('\u{2014}', "--") // Em dash -> double hyphen
        .replace(['\u{2018}', '\u{2019}'], "'")  // Right single quote -> apostrophe
        .replace(['\u{201C}', '\u{201D}'], "\"") // Right double quote -> quote
        .replace('\u{2022}', "* ") // Bullet -> asterisk
        .replace('\u{2026}', "...") // Ellipsis -> three dots
        .replace('\u{00A0}', " ")  // Non-breaking space -> space
        .replace('\u{FB01}', "fi") // fi ligature -> separate chars
        .replace('\u{FB02}', "fl") // fl ligature -> separate chars
        .replace('\u{FB00}', "ff") // ff ligature -> separate chars
        .replace('\u{FB03}', "ffi") // ffi ligature -> separate chars
        .replace('\u{FB04}', "ffl"); // ffl ligature -> separate chars

    result
}

/// Parsed document with extracted text and metadata
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// File type
    pub file_type: FileType,
    /// Extracted text content
    pub content: String,
    /// Content hash for deduplication
    pub content_hash: String,
    /// Total pages (if applicable)
    pub total_pages: Option<u32>,
    /// Page-level content (for PDFs, DOCX)
    pub pages: Vec<PageContent>,
    /// Document metadata
    pub metadata: HashMap<String, String>,
}

/// Content from a single page
#[derive(Debug, Clone)]
pub struct PageContent {
    /// Page number (1-indexed)
    pub page_number: u32,
    /// Text content of the page
    pub content: String,
    /// Character offset in full document
    pub char_offset: usize,
}

/// Multi-format file parser
pub struct FileParser;

impl FileParser {
    /// Extract PDF text with a sync timeout to prevent hangs on problematic fonts
    fn extract_pdf_with_timeout(data: &[u8]) -> Result<String> {
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        // Clone data for the thread (keep original for fallback)
        let data_for_thread = data.to_vec();
        let (tx, rx) = mpsc::channel();

        // Spawn extraction in a separate thread with timeout
        let handle = thread::spawn(move || {
            let result = pdf_extract::extract_text_from_mem(&data_for_thread);
            let _ = tx.send(result);
        });

        // Wait up to 60 seconds for PDF extraction
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(Ok(text)) => {
                let _ = handle.join();
                Ok(text)
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                let err_msg = e.to_string();
                tracing::warn!("pdf-extract failed: {}, trying fallback", err_msg);
                Self::extract_pdf_text_fallback(data)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Thread is still running - can't kill it, but we can return an error
                tracing::error!("PDF extraction timeout after 60s - PDF may have complex fonts");
                // Try fallback which is often faster
                Self::extract_pdf_text_fallback(data)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                tracing::error!("PDF extraction thread crashed");
                Self::extract_pdf_text_fallback(data)
            }
        }
    }

    /// Parse a file based on its extension, with magic bytes fallback
    pub fn parse(filename: &str, data: &[u8]) -> Result<ParsedDocument> {
        let extension = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        // First try extension-based detection
        let mut file_type = FileType::from_extension(&extension);

        // If extension detection fails or returns Unknown, try magic bytes
        if !file_type.is_supported() {
            tracing::debug!("Extension '{}' not recognized, trying magic bytes detection for '{}'", extension, filename);
            file_type = FileType::from_magic_bytes(data);

            if file_type.is_supported() {
                tracing::info!("Detected file type {:?} from magic bytes for '{}'", file_type, filename);
            }
        }

        if !file_type.is_supported() {
            let reason = file_type.unsupported_reason()
                .unwrap_or("File type not supported");
            return Err(Error::UnsupportedFileType(format!("{} - {}", filename, reason)));
        }

        match file_type {
            FileType::Pdf => Self::parse_pdf(data),
            FileType::Docx => Self::parse_docx(data),
            FileType::Doc => {
                // .doc requires external conversion - return error to trigger fallback
                Err(Error::UnsupportedFileType(
                    "doc - Old Word format requires LibreOffice conversion".to_string()
                ))
            }
            FileType::Pptx => Self::parse_pptx(data),
            FileType::Ppt => {
                // .ppt requires external conversion - return error to trigger fallback
                Err(Error::UnsupportedFileType(
                    "ppt - Old PowerPoint format requires LibreOffice conversion".to_string()
                ))
            }
            FileType::Txt | FileType::Markdown => Self::parse_text(data, file_type),
            FileType::Html => Self::parse_html(data),
            FileType::Csv => Self::parse_csv(data),
            FileType::Xlsx | FileType::Xls => Self::parse_xlsx(data),
            FileType::Rtf | FileType::Odt | FileType::Odp | FileType::Ods | FileType::Epub => {
                // These require external tools (pandoc/LibreOffice) - return error to trigger fallback
                Err(Error::UnsupportedFileType(format!(
                    "{} - Requires pandoc or LibreOffice for conversion",
                    extension
                )))
            }
            FileType::Image => {
                // Images require OCR - return error to trigger fallback
                Err(Error::UnsupportedFileType(
                    "image - Requires tesseract OCR for text extraction".to_string()
                ))
            }
            FileType::Code(ref lang) => Self::parse_code(data, lang.clone()),
            FileType::Unknown => Err(Error::UnsupportedFileType(format!("{} - Unknown file type", extension))),
        }
    }

    /// Parse PDF document
    fn parse_pdf(data: &[u8]) -> Result<ParsedDocument> {
        // Try page-by-page extraction using lopdf first for proper page numbers
        if let Ok(result) = Self::parse_pdf_paged(data) {
            return Ok(result);
        }

        // Fall back to pdf_extract (better text quality, but no page boundaries)
        let content = Self::extract_pdf_with_timeout(data)?;

        let content = cleanup_pdf_text(&content);
        let content = content
            .replace('\0', "")
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        if content.trim().is_empty() {
            return Err(Error::file_parse("document.pdf", "No text content could be extracted from PDF"));
        }

        let pages = vec![PageContent {
            page_number: 1,
            content: content.clone(),
            char_offset: 0,
        }];

        let total_pages = match lopdf::Document::load_mem(data) {
            Ok(doc) => Some(doc.get_pages().len() as u32),
            Err(_) => Some(1),
        };

        Ok(ParsedDocument {
            file_type: FileType::Pdf,
            content_hash: hash_content(&content),
            content,
            total_pages,
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Try page-by-page PDF extraction using lopdf for proper page numbers.
    /// Returns Err if lopdf extraction produces insufficient text.
    fn parse_pdf_paged(data: &[u8]) -> Result<ParsedDocument> {
        let doc = lopdf::Document::load_mem(data)
            .map_err(|e| Error::file_parse("document.pdf", format!("lopdf load failed: {}", e)))?;

        let page_ids = doc.get_pages();
        let total_pages = page_ids.len() as u32;

        let mut pages = Vec::new();
        let mut all_content = String::new();
        let mut char_offset = 0usize;

        for (page_num, page_id) in &page_ids {
            let page_text = match doc.get_page_content(*page_id) {
                Ok(content) => {
                    let raw = Self::extract_text_from_content(&content);
                    let cleaned = raw
                        .replace('\0', "")
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    cleaned
                }
                Err(_) => String::new(),
            };

            if !page_text.is_empty() {
                pages.push(PageContent {
                    page_number: *page_num,
                    content: page_text.clone(),
                    char_offset,
                });
                all_content.push_str(&page_text);
                all_content.push('\n');
                char_offset = all_content.len();
            }
        }

        // Require at least 100 chars per page on average to consider extraction successful
        let avg_chars = if pages.is_empty() { 0 } else { all_content.len() / pages.len() };
        if avg_chars < 100 || pages.is_empty() {
            return Err(Error::file_parse("document.pdf", "lopdf extraction too sparse, falling back"));
        }

        tracing::info!("PDF parsed with lopdf: {} pages, {} chars, avg {}/page", pages.len(), all_content.len(), avg_chars);

        Ok(ParsedDocument {
            file_type: FileType::Pdf,
            content_hash: hash_content(&all_content),
            content: all_content,
            total_pages: Some(total_pages),
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Fallback PDF text extraction using lopdf directly
    fn extract_pdf_text_fallback(data: &[u8]) -> Result<String> {
        use lopdf::Document;

        let doc = Document::load_mem(data)
            .map_err(|e| Error::file_parse("document.pdf", format!("Failed to load PDF: {}", e)))?;

        let mut all_text = String::new();
        let pages = doc.get_pages();

        for (page_num, page_id) in pages {
            // Try to extract text from each page
            match doc.get_page_content(page_id) {
                Ok(content) => {
                    // Extract text from content stream
                    let text = Self::extract_text_from_content(&content);
                    if !text.is_empty() {
                        all_text.push_str(&format!("\n--- Page {} ---\n", page_num));
                        all_text.push_str(&text);
                    }
                }
                Err(e) => {
                    tracing::debug!("Could not get content for page {}: {}", page_num, e);
                }
            }
        }

        // If we still couldn't extract text, try getting text from form XObjects
        if all_text.trim().is_empty() {
            tracing::warn!("Fallback extraction produced no text, PDF may be image-based or encrypted");
            // Return a message indicating the PDF needs OCR
            return Err(Error::file_parse(
                "document.pdf",
                "PDF appears to be image-based or has no extractable text. Consider using OCR or the external parser.",
            ));
        }

        Ok(all_text)
    }

    /// Extract text from PDF content stream bytes
    fn extract_text_from_content(content: &[u8]) -> String {
        // Simple text extraction from PDF content stream
        // Look for text between BT (begin text) and ET (end text) operators
        let content_str = String::from_utf8_lossy(content);
        let mut text = String::new();
        let mut in_text_block = false;
        let mut current_text = String::new();

        for line in content_str.lines() {
            let line = line.trim();

            if line == "BT" {
                in_text_block = true;
                continue;
            }

            if line == "ET" {
                in_text_block = false;
                if !current_text.is_empty() {
                    text.push_str(&current_text);
                    text.push(' ');
                    current_text.clear();
                }
                continue;
            }

            if in_text_block {
                // Look for text show operators: Tj, TJ, ', "
                if line.ends_with("Tj") || line.ends_with("TJ") {
                    // Extract text from parentheses
                    if let Some(start) = line.find('(') {
                        if let Some(end) = line.rfind(')') {
                            let extracted = &line[start + 1..end];
                            // Decode basic PDF string escapes
                            let decoded = extracted
                                .replace("\\n", "\n")
                                .replace("\\r", "\r")
                                .replace("\\t", "\t")
                                .replace("\\(", "(")
                                .replace("\\)", ")")
                                .replace("\\\\", "\\");
                            current_text.push_str(&decoded);
                        }
                    }
                }
            }
        }

        text
    }

    /// Parse DOCX document
    fn parse_docx(data: &[u8]) -> Result<ParsedDocument> {
        let doc = docx_rs::read_docx(data)
            .map_err(|e| Error::file_parse("document.docx", e.to_string()))?;

        let mut content = String::new();
        let mut pages = Vec::new();
        let mut current_page = String::new();
        let page_number = 1u32;

        // Extract text from document
        for child in doc.document.children {
            match child {
                docx_rs::DocumentChild::Paragraph(p) => {
                    for child in p.children {
                        if let docx_rs::ParagraphChild::Run(run) = child {
                            for child in run.children {
                                if let docx_rs::RunChild::Text(t) = child {
                                    current_page.push_str(&t.text);
                                    content.push_str(&t.text);
                                }
                            }
                        }
                    }
                    current_page.push('\n');
                    content.push('\n');
                }
                docx_rs::DocumentChild::Table(_) => {
                    // Skip tables for now
                }
                _ => {}
            }
        }

        // Treat as single page for simplicity
        if !current_page.is_empty() {
            pages.push(PageContent {
                page_number,
                content: current_page,
                char_offset: 0,
            });
        }

        Ok(ParsedDocument {
            file_type: FileType::Docx,
            content_hash: hash_content(&content),
            content,
            total_pages: Some(page_number),
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Parse PowerPoint presentation (.pptx)
    fn parse_pptx(data: &[u8]) -> Result<ParsedDocument> {
        use std::io::Read;

        let cursor = std::io::Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| Error::file_parse("presentation.pptx", e.to_string()))?;

        let mut content = String::new();
        let mut pages = Vec::new();
        let mut slide_number = 0u32;

        // Find all slide files (ppt/slides/slide1.xml, slide2.xml, etc.)
        let mut slide_names: Vec<String> = archive
            .file_names()
            .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
            .map(|s| s.to_string())
            .collect();

        // Sort slides by number
        slide_names.sort_by(|a, b| {
            let num_a = a.trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml")
                .parse::<u32>()
                .unwrap_or(0);
            let num_b = b.trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml")
                .parse::<u32>()
                .unwrap_or(0);
            num_a.cmp(&num_b)
        });

        for slide_name in slide_names {
            slide_number += 1;
            let char_offset = content.len();

            if let Ok(mut file) = archive.by_name(&slide_name) {
                let mut xml_content = String::new();
                if file.read_to_string(&mut xml_content).is_ok() {
                    let slide_text = Self::extract_text_from_pptx_xml(&xml_content);

                    if !slide_text.is_empty() {
                        let slide_content = format!("Slide {}:\n{}\n\n", slide_number, slide_text);
                        content.push_str(&slide_content);

                        pages.push(PageContent {
                            page_number: slide_number,
                            content: slide_text,
                            char_offset,
                        });
                    }
                }
            }
        }

        // If no slides found, try to extract from other XML files
        if content.is_empty() {
            content = "Empty presentation or unable to extract text.".to_string();
        }

        let total_pages = if slide_number > 0 { Some(slide_number) } else { None };

        Ok(ParsedDocument {
            file_type: FileType::Pptx,
            content_hash: hash_content(&content),
            content,
            total_pages,
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Extract text from PowerPoint XML content
    fn extract_text_from_pptx_xml(xml: &str) -> String {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut text_parts = Vec::new();
        let mut in_text_element = false;
        let mut current_text = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    // Look for text elements: <a:t> in PPTX
                    let name = e.local_name();
                    if name.as_ref() == b"t" {
                        in_text_element = true;
                        current_text.clear();
                    }
                }
                Ok(Event::Text(e)) => {
                    if in_text_element {
                        if let Ok(text) = e.unescape() {
                            current_text.push_str(&text);
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let name = e.local_name();
                    if name.as_ref() == b"t" && in_text_element {
                        if !current_text.trim().is_empty() {
                            text_parts.push(current_text.trim().to_string());
                        }
                        in_text_element = false;
                    }
                    // Add line break after paragraphs
                    if name.as_ref() == b"p" && !text_parts.is_empty() {
                        text_parts.push("\n".to_string());
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }

        // Join text parts, cleaning up extra whitespace
        text_parts
            .join(" ")
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Parse plain text or markdown
    fn parse_text(data: &[u8], file_type: FileType) -> Result<ParsedDocument> {
        let content = String::from_utf8_lossy(data).to_string();

        let pages = vec![PageContent {
            page_number: 1,
            content: content.clone(),
            char_offset: 0,
        }];

        Ok(ParsedDocument {
            file_type,
            content_hash: hash_content(&content),
            content,
            total_pages: None,
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Parse HTML document
    fn parse_html(data: &[u8]) -> Result<ParsedDocument> {
        let html = String::from_utf8_lossy(data);
        let document = scraper::Html::parse_document(&html);

        // Extract text from body
        let body_selector = scraper::Selector::parse("body").unwrap();
        let mut content = String::new();

        if let Some(body) = document.select(&body_selector).next() {
            for text in body.text() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if !content.is_empty() {
                        content.push(' ');
                    }
                    content.push_str(trimmed);
                }
            }
        }

        let pages = vec![PageContent {
            page_number: 1,
            content: content.clone(),
            char_offset: 0,
        }];

        Ok(ParsedDocument {
            file_type: FileType::Html,
            content_hash: hash_content(&content),
            content,
            total_pages: None,
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Parse CSV file
    fn parse_csv(data: &[u8]) -> Result<ParsedDocument> {
        let mut reader = csv::Reader::from_reader(data);
        let mut content = String::new();

        // Get headers
        if let Ok(headers) = reader.headers() {
            content.push_str(&headers.iter().collect::<Vec<_>>().join(" | "));
            content.push('\n');
        }

        // Read rows
        for record in reader.records().flatten() {
            content.push_str(&record.iter().collect::<Vec<_>>().join(" | "));
            content.push('\n');
        }

        let pages = vec![PageContent {
            page_number: 1,
            content: content.clone(),
            char_offset: 0,
        }];

        Ok(ParsedDocument {
            file_type: FileType::Csv,
            content_hash: hash_content(&content),
            content,
            total_pages: None,
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Parse Excel spreadsheet
    fn parse_xlsx(data: &[u8]) -> Result<ParsedDocument> {
        let cursor = std::io::Cursor::new(data);
        let mut workbook = calamine::open_workbook_auto_from_rs(cursor)
            .map_err(|e| Error::file_parse("spreadsheet.xlsx", e.to_string()))?;

        let mut content = String::new();
        let mut pages = Vec::new();
        let mut page_number = 0u32;

        for sheet_name in workbook.sheet_names().to_vec() {
            page_number += 1;
            let char_offset = content.len();

            if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                let mut sheet_content = format!("Sheet: {}\n", sheet_name);

                for row in range.rows() {
                    let row_text: Vec<String> = row
                        .iter()
                        .map(|cell| match cell {
                            calamine::Data::Empty => String::new(),
                            calamine::Data::String(s) => s.clone(),
                            calamine::Data::Float(f) => f.to_string(),
                            calamine::Data::Int(i) => i.to_string(),
                            calamine::Data::Bool(b) => b.to_string(),
                            calamine::Data::DateTime(dt) => dt.to_string(),
                            _ => String::new(),
                        })
                        .collect();

                    if !row_text.iter().all(|s| s.is_empty()) {
                        sheet_content.push_str(&row_text.join(" | "));
                        sheet_content.push('\n');
                    }
                }

                content.push_str(&sheet_content);
                content.push('\n');

                pages.push(PageContent {
                    page_number,
                    content: sheet_content,
                    char_offset,
                });
            }
        }

        Ok(ParsedDocument {
            file_type: FileType::Xlsx,
            content_hash: hash_content(&content),
            content,
            total_pages: Some(page_number),
            pages,
            metadata: HashMap::new(),
        })
    }

    /// Parse source code file
    fn parse_code(data: &[u8], language: String) -> Result<ParsedDocument> {
        let content = String::from_utf8_lossy(data).to_string();

        let pages = vec![PageContent {
            page_number: 1,
            content: content.clone(),
            char_offset: 0,
        }];

        Ok(ParsedDocument {
            file_type: FileType::Code(language),
            content_hash: hash_content(&content),
            content,
            total_pages: None,
            pages,
            metadata: HashMap::new(),
        })
    }
}

/// Hash content for deduplication
fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}
