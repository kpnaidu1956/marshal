//! File tier classification for intelligent processing
//!
//! Classifies files into tiers based on size and complexity for
//! adaptive timeout and parser selection.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// File processing tier based on size and complexity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileTier {
    /// Small files (<10MB): 120s timeout, native parsing
    Fast,
    /// Medium files (10-100MB): 300s timeout, local tools first
    Medium,
    /// Large files (100MB-1GB): 900s timeout, cloud-first
    Heavy,
    /// Problematic files (scanned PDFs, encrypted, complex fonts): 1200s timeout
    Complex,
}

impl FileTier {
    /// Classify file tier based on size
    pub fn from_size(size_bytes: u64) -> Self {
        const MB: u64 = 1024 * 1024;

        if size_bytes < 10 * MB {
            FileTier::Fast
        } else if size_bytes < 100 * MB {
            FileTier::Medium
        } else {
            FileTier::Heavy
        }
    }

    /// Get default timeout for this tier
    pub fn default_timeout(&self) -> Duration {
        match self {
            FileTier::Fast => Duration::from_secs(120),    // 2 minutes
            FileTier::Medium => Duration::from_secs(300),  // 5 minutes
            FileTier::Heavy => Duration::from_secs(900),   // 15 minutes
            FileTier::Complex => Duration::from_secs(1200), // 20 minutes
        }
    }

    /// Get default worker count for this tier
    pub fn default_workers(&self) -> usize {
        match self {
            FileTier::Fast => num_cpus::get().min(8),
            FileTier::Medium => 4,
            FileTier::Heavy => 2,
            FileTier::Complex => 2,
        }
    }
}

impl std::fmt::Display for FileTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileTier::Fast => write!(f, "fast"),
            FileTier::Medium => write!(f, "medium"),
            FileTier::Heavy => write!(f, "heavy"),
            FileTier::Complex => write!(f, "complex"),
        }
    }
}

/// Parser strategy recommendation based on file characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserStrategy {
    /// Use native Rust libraries only (fastest)
    #[default]
    NativeOnly,
    /// Try local tools first (pdftotext, pandoc, tesseract)
    LocalToolsFirst,
    /// Try cloud services first (Document AI, Unstructured)
    CloudFirst,
    /// Try multiple parsers in parallel (for very complex files)
    ParallelAttempt,
}

/// File characteristics for intelligent routing and processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCharacteristics {
    /// File size in bytes
    pub size_bytes: u64,
    /// Assigned processing tier
    pub tier: FileTier,
    /// Calculated timeout in seconds
    pub timeout_secs: u64,
    /// Recommended parser strategy
    pub recommended_parser: ParserStrategy,
    /// Complexity score (0.0-1.0)
    pub complexity_score: f32,
    /// Detected as scanned PDF (image-based)
    pub is_scanned_pdf: bool,
    /// Has complex font encoding
    pub has_complex_fonts: bool,
    /// PDF is encrypted
    pub is_encrypted: bool,
    /// Estimated page count (for PDFs)
    pub estimated_pages: Option<u32>,
    /// File extension
    pub extension: String,
}

impl FileCharacteristics {
    /// Create characteristics for a non-PDF file (simpler analysis)
    pub fn for_file(filename: &str, size_bytes: u64) -> Self {
        let extension = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        let tier = FileTier::from_size(size_bytes);
        let timeout = tier.default_timeout();

        // Determine parser strategy based on extension
        let recommended_parser = match extension.as_str() {
            // Native support
            "pdf" | "docx" | "pptx" | "xlsx" | "txt" | "md" | "csv" | "html" => {
                if size_bytes > 50 * 1024 * 1024 {
                    ParserStrategy::LocalToolsFirst
                } else {
                    ParserStrategy::NativeOnly
                }
            }
            // Legacy formats - need conversion
            "doc" | "ppt" | "xls" => ParserStrategy::LocalToolsFirst,
            // Complex formats
            "rtf" | "odt" | "odp" | "ods" | "epub" => ParserStrategy::LocalToolsFirst,
            // Images - need OCR
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tiff" => ParserStrategy::CloudFirst,
            // Unknown
            _ => ParserStrategy::LocalToolsFirst,
        };

        Self {
            size_bytes,
            tier,
            timeout_secs: timeout.as_secs(),
            recommended_parser,
            complexity_score: 0.0,
            is_scanned_pdf: false,
            has_complex_fonts: false,
            is_encrypted: false,
            estimated_pages: None,
            extension,
        }
    }

    /// Create characteristics for a PDF with detailed analysis
    pub fn for_pdf(filename: &str, size_bytes: u64, analysis: &PdfAnalysis) -> Self {
        let base_tier = FileTier::from_size(size_bytes);

        // Upgrade tier if file is complex
        let tier = if analysis.is_encrypted || analysis.is_scanned {
            FileTier::Complex
        } else if analysis.has_complex_fonts && base_tier == FileTier::Fast {
            FileTier::Medium
        } else {
            base_tier
        };

        // Calculate complexity score
        let complexity_score = calculate_complexity_score(analysis, size_bytes);

        // Calculate adaptive timeout
        let timeout = calculate_timeout(size_bytes, &tier, complexity_score, analysis);

        // Determine parser strategy
        let recommended_parser = if analysis.is_encrypted || analysis.is_scanned {
            ParserStrategy::CloudFirst // Need cloud for decryption or OCR
        } else if analysis.has_complex_fonts && size_bytes > 10 * 1024 * 1024 {
            ParserStrategy::LocalToolsFirst // pdftotext handles fonts well
        } else if complexity_score > 0.7 {
            ParserStrategy::ParallelAttempt // Try multiple parsers
        } else if size_bytes > 100 * 1024 * 1024 {
            ParserStrategy::CloudFirst
        } else {
            ParserStrategy::NativeOnly
        };

        Self {
            size_bytes,
            tier,
            timeout_secs: timeout.as_secs(),
            recommended_parser,
            complexity_score,
            is_scanned_pdf: analysis.is_scanned,
            has_complex_fonts: analysis.has_complex_fonts,
            is_encrypted: analysis.is_encrypted,
            estimated_pages: Some(analysis.estimated_pages),
            extension: filename
                .rsplit('.')
                .next()
                .unwrap_or("pdf")
                .to_lowercase(),
        }
    }

    /// Get the timeout as a Duration
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

/// PDF-specific analysis results
#[derive(Debug, Clone, Default)]
pub struct PdfAnalysis {
    /// PDF is encrypted
    pub is_encrypted: bool,
    /// Has complex font encoding (ToUnicode CMaps)
    pub has_complex_fonts: bool,
    /// Likely a scanned document (image-based)
    pub is_scanned: bool,
    /// Estimated page count
    pub estimated_pages: u32,
    /// Number of image references found
    pub image_count: usize,
    /// Number of text stream markers found
    pub text_stream_count: usize,
}

impl PdfAnalysis {
    /// Perform quick analysis of PDF data without full parsing
    pub fn analyze(data: &[u8]) -> Self {
        // Quick header validation
        if !data.starts_with(b"%PDF-") {
            return Self::default();
        }

        // Check for encryption marker
        let is_encrypted = data.windows(8).any(|w| w == b"/Encrypt");

        // Check for complex font encoding (ToUnicode CMaps)
        let has_complex_fonts = data.windows(11).any(|w| w == b"/ToUnicode ");

        // Count image references vs text streams
        let image_count = data.windows(7).filter(|w| w == b"/Image ").count()
            + data.windows(8).filter(|w| w == b"/XObject ").count();

        // Count text stream markers (BT = Begin Text)
        let text_stream_count = data.windows(3).filter(|w| {
            w == b"BT " || w == b"BT\n" || w == b"BT\r"
        }).count();

        // Estimate if scanned: many images, few text streams
        let is_scanned = image_count > 0 && (text_stream_count == 0 || image_count > text_stream_count * 3);

        // Estimate page count
        let estimated_pages = data.windows(6)
            .filter(|w| w == b"/Page " || w == b"/Page\n" || w == b"/Page\r")
            .count() as u32;

        Self {
            is_encrypted,
            has_complex_fonts,
            is_scanned,
            estimated_pages: estimated_pages.max(1), // At least 1 page
            image_count,
            text_stream_count,
        }
    }
}

/// Calculate complexity score (0.0-1.0)
fn calculate_complexity_score(analysis: &PdfAnalysis, size_bytes: u64) -> f32 {
    let mut score = 0.0f32;

    // Encrypted = high complexity
    if analysis.is_encrypted {
        score += 0.4;
    }

    // Scanned = high complexity (needs OCR)
    if analysis.is_scanned {
        score += 0.3;
    }

    // Complex fonts add complexity
    if analysis.has_complex_fonts {
        score += 0.1;
    }

    // Large file size adds complexity
    let size_factor = (size_bytes as f32 / (100.0 * 1024.0 * 1024.0)).min(0.2);
    score += size_factor;

    // Many pages add complexity
    if analysis.estimated_pages > 100 {
        score += 0.1;
    }

    score.min(1.0)
}

/// Calculate adaptive timeout based on file characteristics
fn calculate_timeout(
    size_bytes: u64,
    tier: &FileTier,
    complexity_score: f32,
    analysis: &PdfAnalysis,
) -> Duration {
    // Base: 1 second per 100KB, minimum 60s
    let size_timeout = (size_bytes / (100 * 1024)).max(60);

    // Complexity multiplier
    let multiplier = if analysis.is_scanned {
        3.0 // OCR takes 3x longer
    } else if analysis.has_complex_fonts {
        2.0 // Font processing slower
    } else if analysis.is_encrypted {
        1.5 // Decryption overhead
    } else {
        1.0 + complexity_score as f64 // Scale with complexity
    };

    // Tier caps
    let max_timeout = match tier {
        FileTier::Fast => 120,
        FileTier::Medium => 300,
        FileTier::Heavy => 900,
        FileTier::Complex => 1200,
    };

    let calculated = (size_timeout as f64 * multiplier) as u64;
    Duration::from_secs(calculated.min(max_timeout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_from_size() {
        const MB: u64 = 1024 * 1024;
        assert_eq!(FileTier::from_size(1 * MB), FileTier::Fast);          // 1 MiB
        assert_eq!(FileTier::from_size(9 * MB), FileTier::Fast);          // 9 MiB
        assert_eq!(FileTier::from_size(10 * MB), FileTier::Medium);       // 10 MiB (boundary)
        assert_eq!(FileTier::from_size(50 * MB), FileTier::Medium);       // 50 MiB
        assert_eq!(FileTier::from_size(100 * MB), FileTier::Heavy);       // 100 MiB (boundary)
        assert_eq!(FileTier::from_size(500 * MB), FileTier::Heavy);       // 500 MiB
    }

    #[test]
    fn test_pdf_analysis() {
        // Simple PDF header
        let simple_pdf = b"%PDF-1.4\nsome content BT text ET";
        let analysis = PdfAnalysis::analyze(simple_pdf);
        assert!(!analysis.is_encrypted);
        assert!(!analysis.is_scanned);

        // Encrypted PDF
        let encrypted_pdf = b"%PDF-1.4\n/Encrypt dictionary here";
        let analysis = PdfAnalysis::analyze(encrypted_pdf);
        assert!(analysis.is_encrypted);
    }

    #[test]
    fn test_characteristics_for_file() {
        const MB: u64 = 1024 * 1024;
        let chars = FileCharacteristics::for_file("document.pdf", 5 * MB);
        assert_eq!(chars.tier, FileTier::Fast);
        assert_eq!(chars.extension, "pdf");

        // Files > 50 MiB use LocalToolsFirst strategy
        let chars = FileCharacteristics::for_file("large.xlsx", 51 * MB);
        assert_eq!(chars.tier, FileTier::Medium);
        assert_eq!(chars.recommended_parser, ParserStrategy::LocalToolsFirst);
    }
}
