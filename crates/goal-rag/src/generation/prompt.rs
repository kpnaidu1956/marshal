//! Prompt templates for RAG generation

use crate::providers::vector_store::VectorSearchResult;
use crate::types::response::Citation;

/// Prompt builder for RAG queries
pub struct PromptBuilder;

impl PromptBuilder {
    /// Build context from search results
    pub fn build_context(results: &[VectorSearchResult]) -> String {
        let mut context = String::new();

        for (i, result) in results.iter().enumerate() {
            let source = &result.chunk.source;

            // Build source reference
            let source_ref = Self::format_source_ref(source, i + 1);

            context.push_str(&format!(
                "[{}] {}\n\nContent:\n{}\n\n---\n\n",
                i + 1,
                source_ref,
                result.chunk.content
            ));
        }

        context
    }

    /// Format source reference for context
    fn format_source_ref(source: &crate::types::ChunkSource, _index: usize) -> String {
        let mut parts = vec![source.filename.clone()];

        // Only show page numbers for PDFs (Word docs don't have reliable page numbers)
        if let Some(page) = source.page_number {
            let is_pdf = matches!(source.file_type, crate::types::FileType::Pdf);
            if is_pdf {
                parts.push(format!("Page {}", page));
            }
        }

        if let (Some(start), Some(end)) = (source.line_start, source.line_end) {
            parts.push(format!("Lines {}-{}", start, end));
        }

        if let Some(section) = &source.section_title {
            parts.push(format!("Section: {}", section));
        }

        parts.join(", ")
    }

    /// Build the full RAG prompt with strict grounding
    pub fn build_rag_prompt(question: &str, context: &str, _citations: &[Citation]) -> String {
        format!(
            r#"You are an assistant that answers questions ONLY using the provided document excerpts below. You must NEVER use your own knowledge or training data.

RULES:
1. Use ONLY facts from the CONTEXT below. Do NOT add information from your own knowledge or training data.
2. If the CONTEXT contains ANY information related to the question — even partial or indirect — present what the documents say. Use quotation marks when quoting exact text from the documents.
3. If the question asks for a specific detail (e.g., a number, size, or threshold) and the CONTEXT does not contain that exact detail, say what the documents DO cover about the topic, then note: "The specific [detail] is not stated in the available documents."
4. Only say "no information available" when the CONTEXT is completely unrelated to the question topic.
5. Do NOT generate document names, filenames, page numbers, or [Source: ...] references — these are handled separately.
6. Do NOT add a "Sources used" section at the end.

RESPONSE FORMAT:
- Use bullet points or numbered lists for requirements, definitions, or multiple items
- Quote directly from documents using quotation marks where appropriate
- Be thorough — include all relevant details from the provided context

CONTEXT FROM DOCUMENTS:
{context}

QUESTION: {question}

Answer using ONLY the document content above. Do NOT add [Source: ...] citations — they are handled separately:"#,
            context = context,
            question = question
        )
    }

    // format_sources_list removed — sources no longer included in prompt to prevent hallucinated citations

    /// Build RAG prompt with learning from past Q&A
    pub fn build_rag_prompt_with_learning(
        question: &str,
        context: &str,
        _citations: &[Citation],
        past_qa: &[(String, String)],
    ) -> String {
        let past_examples = if past_qa.is_empty() {
            String::new()
        } else {
            let examples: Vec<String> = past_qa
                .iter()
                .take(3)  // Limit to 3 examples to avoid context overflow
                .map(|(q, a)| format!("Q: {}\nA: {}", q, a))
                .collect();
            format!(
                "\nHERE ARE EXAMPLES OF WELL-ANSWERED SIMILAR QUESTIONS:\n{}\n\nNow answer the new question following the same comprehensive style:\n",
                examples.join("\n\n---\n\n")
            )
        };

        format!(
            r#"You are an assistant that answers questions ONLY using the provided document excerpts below. You must NEVER use your own knowledge or training data.

ABSOLUTE RULES — VIOLATION MEANS FAILURE:
1. Your answer must contain ONLY facts found in the CONTEXT below. For every claim you make, the exact or near-exact wording MUST exist in the provided text.
2. If the CONTEXT does not contain relevant information, respond ONLY with: "The available documents do not contain specific information about this topic. Please try rephrasing your question."
3. Do NOT generate document names, filenames, page numbers, or section references. Sources are handled separately.
4. Do NOT use your training data, world knowledge, or any information not present in the CONTEXT.
5. When quoting requirements or regulations, use the EXACT wording from the documents. Use quotation marks for direct quotes.
6. If the context contains partial information, clearly state what the documents say and explicitly note: "The documents do not address [specific aspect]."
{past_examples}
RESPONSE FORMAT:
- Provide a clear, well-organized answer using bullet points or numbered lists
- Do NOT include [Source: ...] citations inline — sources are shown separately
- Be thorough but ONLY include details from the provided context
- If only partial information is available, state what the documents say and note what is not covered

CONTEXT FROM DOCUMENTS:
{context}

QUESTION: {question}

Answer using ONLY the document content above. Do NOT add [Source: ...] citations — they are handled separately:"#,
            past_examples = past_examples,
            context = context,
            question = question
        )
    }

    /// Build a simple question-answering prompt
    pub fn build_qa_prompt(question: &str, context: &str) -> String {
        format!(
            r#"Based on the following context, answer the question. Only use information from the context.

Context:
{context}

Question: {question}

Answer:"#,
            context = context,
            question = question
        )
    }

    /// Build a summarization prompt
    pub fn build_summary_prompt(text: &str) -> String {
        format!(
            r#"Summarize the following text in clear, concise language:

{text}

Summary:"#,
            text = text
        )
    }
}
