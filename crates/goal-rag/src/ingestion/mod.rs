//! Document ingestion pipeline with multi-format parsing

mod chunker;
pub mod external_parser;
mod parser;
mod processor;

pub use chunker::TextChunker;
pub use external_parser::{ExternalParser, ExternalParserConfig, ParsedExternalDocument, ParserAttempt, EscalationResult};
pub use parser::{FileParser, PageContent, ParsedDocument};
pub use processor::IngestPipeline;
