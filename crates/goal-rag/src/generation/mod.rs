//! Answer generation with LLM and citation handling

pub mod citation;
pub mod ollama;
pub mod prompt;

pub use citation::extract_and_link_citations;
pub use ollama::OllamaClient;
pub use prompt::PromptBuilder;
