//! Citation extraction and linking

use regex::Regex;
use crate::types::response::Citation;

/// Extract citations from LLM response and link them to source chunks
pub fn extract_and_link_citations(
    answer: &str,
    available_citations: &mut [Citation],
) -> (String, Vec<Citation>) {
    // Pattern to match [Source: filename, Page X] or similar
    let citation_pattern = Regex::new(
        r"\[Source:\s*([^,\]]+)(?:,\s*(?:Page\s*(\d+)|Lines?\s*(\d+)(?:-(\d+))?))?\]"
    ).expect("Invalid regex");

    let mut linked_citations = Vec::new();
    let mut clean_answer = answer.to_string();

    // Find all citation matches
    for cap in citation_pattern.captures_iter(answer) {
        let _full_match = cap.get(0).map(|m| m.as_str()).unwrap_or("");
        let filename = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let page: Option<u32> = cap.get(2).and_then(|m| m.as_str().parse().ok());
        let line_start: Option<u32> = cap.get(3).and_then(|m| m.as_str().parse().ok());
        let _line_end: Option<u32> = cap.get(4).and_then(|m| m.as_str().parse().ok());

        // Find matching citation
        if let Some(citation) = find_matching_citation(
            available_citations,
            filename,
            page,
            line_start,
        ) {
            // Check if we haven't already linked this citation
            if !linked_citations.iter().any(|c: &Citation| c.chunk_id == citation.chunk_id) {
                linked_citations.push(citation);
            }
        }
    }

    // If no citations were explicitly found in the text, use top citations by similarity
    if linked_citations.is_empty() && !available_citations.is_empty() {
        // Sort by similarity and take top 3
        available_citations.sort_by(|a, b| {
            b.similarity_score.partial_cmp(&a.similarity_score).unwrap()
        });

        for citation in available_citations.iter().take(3) {
            linked_citations.push(citation.clone());
        }

        // Add implicit citation markers
        if !linked_citations.is_empty() {
            clean_answer.push_str("\n\nSources used:");
            for citation in &linked_citations {
                clean_answer.push_str(&format!("\n- {}", citation.format_inline()));
            }
        }
    }

    (clean_answer, linked_citations)
}

/// Find a citation matching the given criteria
fn find_matching_citation(
    citations: &[Citation],
    filename: &str,
    page: Option<u32>,
    line_start: Option<u32>,
) -> Option<Citation> {
    // Try exact match first
    for citation in citations {
        let filename_matches = citation.filename.contains(filename)
            || filename.contains(&citation.filename)
            || filename.to_lowercase() == citation.filename.to_lowercase();

        if filename_matches {
            // Check page match if specified
            if let Some(p) = page {
                if citation.page_number == Some(p) {
                    return Some(citation.clone());
                }
            }

            // Check line match if specified
            if let Some(start) = line_start {
                if citation.line_start == Some(start) {
                    return Some(citation.clone());
                }
            }

            // Filename only match
            if page.is_none() && line_start.is_none() {
                return Some(citation.clone());
            }
        }
    }

    // Fuzzy match - just find by filename
    for citation in citations {
        if citation.filename.contains(filename) || filename.contains(&citation.filename) {
            return Some(citation.clone());
        }
    }

    None
}

/// Highlight query terms in citation snippets
pub fn highlight_snippet(snippet: &str, query_terms: &[&str]) -> String {
    let mut highlighted = snippet.to_string();

    for term in query_terms {
        if term.len() < 3 {
            continue; // Skip very short terms
        }

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

    highlighted
}

/// Truncate snippet to a maximum length while preserving word boundaries
pub fn truncate_snippet(snippet: &str, max_len: usize) -> String {
    if snippet.len() <= max_len {
        return snippet.to_string();
    }

    // Find a word boundary near max_len
    let mut end = max_len;
    while end > 0 && !snippet.is_char_boundary(end) {
        end -= 1;
    }

    // Try to end at a word boundary
    if let Some(pos) = snippet[..end].rfind(' ') {
        return format!("{}...", &snippet[..pos]);
    }

    format!("{}...", &snippet[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_snippet() {
        let snippet = "This is a test about climate change and global warming.";
        let terms = vec!["climate", "warming"];
        let highlighted = highlight_snippet(snippet, &terms);

        assert!(highlighted.contains("<mark>climate</mark>"));
        assert!(highlighted.contains("<mark>warming</mark>"));
    }

    #[test]
    fn test_truncate_snippet() {
        let snippet = "This is a very long snippet that needs to be truncated.";
        let truncated = truncate_snippet(snippet, 20);

        assert!(truncated.len() <= 23); // 20 + "..."
        assert!(truncated.ends_with("..."));
    }
}
