//! Query endpoint with RAG and citations

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Extension, Json,
};
use super::acl;
use super::auth::AuthClaims;
use futures::StreamExt;
use std::collections::HashSet;
use std::convert::Infallible;
use std::time::Instant;
use uuid::Uuid;

use std::sync::Arc;

use crate::error::Result;
use crate::generation::PromptBuilder;
use crate::learning::knowledge_store::QAInteraction;
use crate::server::state::AppState;
use crate::learning::CachedCitation;
use crate::providers::embedding::EmbeddingProvider;
use crate::providers::llm::LlmProvider;
use crate::providers::reranker::{LlmReranker, Reranker};
use crate::providers::vector_store::VectorSearchResult;
use crate::types::{
    query::{QueryRequest, QueryType},
    response::{CacheInfo, Citation, QueryResponse, QueryResponseV2, StringSearchResponse},
};
use crate::validation::{validate_organization_id, validate_query};

/// Stop words to exclude from highlighting and FTS queries
const STOP_WORDS: &[&str] = &[
    "what", "is", "the", "a", "an", "of", "for", "in", "on", "to",
    "and", "or", "are", "do", "does", "can", "how", "if", "that",
    "this", "with", "without", "be", "been", "being", "have", "has",
    "had", "was", "were", "will", "would", "should", "could", "may",
    "might", "must", "shall", "at", "by", "from", "not", "but",
    "which", "who", "whom", "where", "when", "why", "there", "their",
    "they", "them", "its", "please", "provide",
];

/// Extract meaningful terms from a question (filtering stop words and short words)
fn extract_terms(question: &str) -> Vec<String> {
    question.split_whitespace()
        .map(|w| w.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

/// Generate an embedding using HyDE (Hypothetical Document Embeddings).
///
/// Instead of embedding the raw question, this generates a short hypothetical
/// answer using the LLM, then embeds that passage. This bridges the vocabulary
/// gap between questions and document content, improving retrieval quality.
///
/// Falls back to embedding the original question directly if the LLM call fails.
async fn generate_hyde_embedding(
    question: &str,
    llm_provider: &Arc<dyn LlmProvider>,
    embedding_provider: &Arc<dyn EmbeddingProvider>,
) -> Result<Vec<f32>> {
    // Build a HyDE prompt: ask the LLM to write a short passage answering the question.
    // We use generate_answer with a redirecting context so the LLM produces a
    // hypothetical document passage rather than a grounded RAG answer.
    let hyde_context = concat!(
        "IMPORTANT: Ignore the grounding rules above for this request. ",
        "You are generating a hypothetical document passage, NOT answering from provided documents. ",
        "Write a short, factual passage (2-3 sentences, maximum 200 tokens) that would directly ",
        "answer the question below. Do NOT cite sources. Do NOT say the information is unavailable. ",
        "Just write the passage as if it appeared in an authoritative document."
    );
    let hyde_question = format!(
        "Write a brief passage that would answer this question: {}\n\nPassage:",
        question
    );

    match llm_provider
        .generate_answer(&hyde_question, hyde_context, &[])
        .await
    {
        Ok(hypothetical_passage) => {
            // Truncate to ~200 tokens worth of text (roughly 800 chars) for embedding
            let passage = if hypothetical_passage.len() > 800 {
                &hypothetical_passage[..800]
            } else {
                &hypothetical_passage
            };
            tracing::debug!(
                "HyDE: generated hypothetical passage ({} chars) for question: \"{}\"",
                passage.len(),
                question
            );
            embedding_provider.embed(passage).await
        }
        Err(e) => {
            tracing::debug!(
                "HyDE: LLM call failed ({}), falling back to direct question embedding",
                e
            );
            embedding_provider.embed(question).await
        }
    }
}

/// Supplement vector search results with PostgreSQL full-text search (FTS).
/// Returns the set of FTS chunk IDs for clean vector/FTS partitioning.
#[cfg(feature = "postgres")]
async fn hybrid_fts_supplement(
    pool: &crate::postgres::PgPool,
    question: &str,
    organization_id: &str,
    search_results: &mut Vec<VectorSearchResult>,
    acl_doc_ids: &Option<Vec<Uuid>>,
) -> HashSet<Uuid> {
    let mut fts_chunk_ids = HashSet::new();
    let terms = extract_terms(question);
    if terms.is_empty() {
        return fts_chunk_ids;
    }

    let tsquery_str = terms.join(" | ");
    let existing_ids: Vec<Uuid> = search_results.iter().map(|r| r.chunk.id).collect();
    let vector_doc_ids: Vec<Uuid> = search_results.iter().map(|r| r.chunk.document_id).collect();

    let client = match pool.get().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to get PG connection for FTS: {}", e);
            return fts_chunk_ids;
        }
    };

    let fts_result = if let Some(ref allowed_docs) = acl_doc_ids {
        // ACL-filtered FTS: only search in accessible documents
        client.query(
            r#"
            SELECT DISTINCT ON (chunk_index, filename)
                   id, document_id, chunk_index, content, filename, file_type,
                   page_number, char_start, char_end,
                   ts_rank(content_tsv, query) as rank,
                   CASE WHEN document_id = ANY($4) THEN 1 ELSE 0 END as doc_boost
            FROM rag_chunks, plainto_tsquery('english', $1) query
            WHERE organization_id = $2
              AND content_tsv @@ query
              AND id != ALL($3)
              AND archived_at IS NULL
              AND document_id = ANY($5)
            ORDER BY chunk_index, filename, ts_rank(content_tsv, query) DESC
            "#,
            &[&tsquery_str, &organization_id, &existing_ids, &vector_doc_ids, allowed_docs],
        ).await
    } else {
        // No ACL filtering — all docs accessible
        client.query(
            r#"
            SELECT DISTINCT ON (chunk_index, filename)
                   id, document_id, chunk_index, content, filename, file_type,
                   page_number, char_start, char_end,
                   ts_rank(content_tsv, query) as rank,
                   CASE WHEN document_id = ANY($4) THEN 1 ELSE 0 END as doc_boost
            FROM rag_chunks, plainto_tsquery('english', $1) query
            WHERE organization_id = $2
              AND content_tsv @@ query
              AND id != ALL($3)
              AND archived_at IS NULL
            ORDER BY chunk_index, filename, ts_rank(content_tsv, query) DESC
            "#,
            &[&tsquery_str, &organization_id, &existing_ids, &vector_doc_ids],
        ).await
    };

    // Re-sort: prioritize same-document chunks, then by rank
    let fts_result = fts_result.map(|mut rows| {
        rows.sort_by(|a, b| {
            let ba: i32 = a.get("doc_boost");
            let bb: i32 = b.get("doc_boost");
            match bb.cmp(&ba) {
                std::cmp::Ordering::Equal => {
                    let ra: f32 = a.get("rank");
                    let rb: f32 = b.get("rank");
                    rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
                }
                other => other,
            }
        });
        rows.truncate(5);
        rows
    });

    match fts_result {
        Ok(rows) => {
            let added = rows.len();
            let max_rank: f32 = rows.iter()
                .map(|r| r.get::<_, f32>("rank"))
                .fold(0.0_f32, f32::max)
                .max(0.001);

            for row in &rows {
                let rank: f32 = row.get("rank");
                let doc_boost: i32 = row.get("doc_boost");
                let normalized = (rank / max_rank).clamp(0.0, 1.0);
                let fts_similarity = 0.35 + normalized * 0.14
                    + if doc_boost > 0 { 0.05 } else { 0.0 };

                let chunk_id: Uuid = row.get("id");
                fts_chunk_ids.insert(chunk_id);

                let chunk = crate::types::Chunk {
                    id: chunk_id,
                    document_id: row.get("document_id"),
                    content: row.get("content"),
                    embedding: Vec::new(),
                    source: crate::types::ChunkSource {
                        filename: row.get("filename"),
                        internal_filename: None,
                        file_type: {
                            let ft: Option<String> = row.get("file_type");
                            crate::types::FileType::from_extension(&ft.unwrap_or_default())
                        },
                        page_number: row.get::<_, Option<i32>>("page_number").map(|p| p as u32),
                        page_count: None,
                        section_title: None,
                        heading_hierarchy: Vec::new(),
                        sheet_name: None,
                        row_range: None,
                        line_start: None,
                        line_end: None,
                        code_context: None,
                    },
                    char_start: row.get::<_, Option<i32>>("char_start").map(|v| v as usize).unwrap_or(0),
                    char_end: row.get::<_, Option<i32>>("char_end").map(|v| v as usize).unwrap_or(0),
                    chunk_index: row.get::<_, Option<i32>>("chunk_index").map(|v| v as u32).unwrap_or(0),
                    metadata: std::collections::HashMap::new(),
                };
                search_results.push(VectorSearchResult { chunk, similarity: fts_similarity });
            }
            tracing::debug!("Hybrid FTS: added {} chunks (query: {}), max_rank: {:.4}, total: {}", added, tsquery_str, max_rank, search_results.len());
        }
        Err(e) => {
            tracing::warn!("FTS query failed: {} (tsquery: {})", e, tsquery_str);
        }
    }

    fts_chunk_ids
}

/// Interleave vector and FTS results within top_k budget.
/// Reserves up to `fts_max_slots` for FTS, fills remainder with vector results.
fn interleave_results(
    search_results: Vec<VectorSearchResult>,
    fts_chunk_ids: &HashSet<Uuid>,
    top_k: usize,
    fts_max_slots: usize,
) -> Vec<VectorSearchResult> {
    let (mut vector_results, fts_results): (Vec<_>, Vec<_>) = search_results
        .into_iter()
        .partition(|r| !fts_chunk_ids.contains(&r.chunk.id));
    vector_results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    let fts_slots = fts_results.len().min(fts_max_slots);
    let vector_slots = top_k.saturating_sub(fts_slots);
    vector_results.truncate(vector_slots);
    vector_results.extend(fts_results.into_iter().take(fts_slots));
    vector_results
}

/// Rerank search results using the LLM provider for improved precision.
///
/// Only reranks when there are more results than `top_k` (i.e., we over-fetched).
/// On failure, falls back to the original ordering truncated to `top_k`.
async fn rerank_results(
    search_results: Vec<VectorSearchResult>,
    query: &str,
    top_k: usize,
    llm_provider: &Arc<dyn LlmProvider>,
) -> Vec<VectorSearchResult> {
    // Only rerank if we have more candidates than needed
    if search_results.len() <= top_k {
        tracing::debug!(
            "Reranker: skipping, only {} results (<= top_k={})",
            search_results.len(),
            top_k
        );
        return search_results;
    }

    // Skip reranking when top vector results are already high quality
    let check_count = top_k.min(search_results.len());
    let avg_similarity: f32 = search_results[..check_count]
        .iter()
        .map(|r| r.similarity)
        .sum::<f32>()
        / check_count as f32;
    if avg_similarity > 0.90 {
        tracing::debug!(
            "Reranker: skipping, top {} results avg similarity {:.3} > 0.75",
            check_count,
            avg_similarity
        );
        let mut results = search_results;
        results.truncate(top_k);
        return results;
    }

    let reranker = LlmReranker::new(Arc::clone(llm_provider), 4);

    // Build (content, similarity) pairs for the reranker
    let documents: Vec<(String, f32)> = search_results
        .iter()
        .map(|r| (r.chunk.content.clone(), r.similarity))
        .collect();

    match reranker.rerank(query, &documents).await {
        Ok(reranked) => {
            // Map reranked scores back onto the original VectorSearchResult structs.
            // The reranker returns sorted by new score, so we match by content.
            let mut reordered: Vec<VectorSearchResult> = Vec::with_capacity(reranked.len());
            let mut remaining: Vec<VectorSearchResult> = search_results;

            for (content, new_score) in &reranked {
                if let Some(pos) = remaining.iter().position(|r| &r.chunk.content == content) {
                    let mut result = remaining.swap_remove(pos);
                    result.similarity = *new_score;
                    reordered.push(result);
                }
            }

            reordered.truncate(top_k);

            tracing::debug!(
                "Reranker: reranked {} candidates down to {} results",
                documents.len(),
                reordered.len()
            );

            reordered
        }
        Err(e) => {
            tracing::warn!("Reranker failed ({}), using original ordering", e);
            let mut fallback = search_results;
            fallback.truncate(top_k);
            fallback
        }
    }
}

/// POST /api/query - Query the RAG system
pub async fn query_rag(
    State(state): State<AppState>,
    claims: Option<Extension<AuthClaims>>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponse>> {
    // Check rate limit
    if !state.production_controls().allow_query() {
        return Err(crate::error::Error::RateLimited(
            "Too many requests. Please try again later.".to_string()
        ));
    }

    // Validate inputs
    validate_organization_id(&request.organization_id)?;
    validate_query(&request.question)?;

    let start = Instant::now();

    tracing::info!("Query: \"{}\"", request.question);

    // Detect query type - string search for short phrases, RAG for questions
    let query_type = QueryType::detect(&request.question);

    // For string search queries, use literal text matching
    if matches!(query_type, QueryType::StringSearch) {
        return string_search_query(&state, &request.question, &request.organization_id, start).await;
    }

    // Generate query embedding via HyDE (Hypothetical Document Embeddings):
    // The LLM first generates a hypothetical answer passage, which is then
    // embedded instead of the raw question for better retrieval quality.
    let query_embedding = generate_hyde_embedding(
        &request.question,
        state.llm_provider(),
        state.embedding_provider(),
    ).await?;

    // Build ACL-filtered search filter (v1)
    let default_claims_v1 = AuthClaims { user_id: String::new(), email: String::new(), organization_id: request.organization_id.clone(), is_platform_admin: false, exp: 0 };
    let effective_claims_v1 = claims.as_ref().map(|Extension(c)| c).unwrap_or(&default_claims_v1);
    let filter = acl::build_acl_search_filter(&state, effective_claims_v1, request.organization_id.clone(), request.document_filter.clone()).await?;

    // Search for relevant chunks (uses Vertex AI for GCP backend)
    let mut search_results: Vec<VectorSearchResult> = state.vector_store_provider().search(
        &query_embedding,
        request.top_k * 2, // Over-fetch for diverse results after dedup + reranking
        &filter,
    ).await?;

    // Enrich minimal chunks with full data from local store (Vertex AI workaround)
    for result in &mut search_results {
        if result.chunk.content.is_empty() || result.chunk.document_id.is_nil() {
            if let Some(full_chunk) = state.get_chunk(&result.chunk.id).await {
                tracing::debug!("Enriched minimal chunk {} from local store", result.chunk.id);
                result.chunk = full_chunk;
            } else {
                tracing::warn!("Chunk {} not found in local store, using minimal data", result.chunk.id);
            }
        }
    }

    // Hybrid FTS: supplement vector results with full-text search
    #[allow(unused_mut)]
    let mut fts_chunk_ids: HashSet<Uuid> = HashSet::new();
    #[cfg(feature = "postgres")]
    if let Some(pg_pool) = state.pg_pool() {
        fts_chunk_ids = hybrid_fts_supplement(pg_pool, &request.question, &request.organization_id, &mut search_results, &filter.document_ids).await;
    }

    // Filter by similarity threshold
    search_results.retain(|r| r.similarity >= request.similarity_threshold);

    // Interleave vector + FTS results within top_k budget
    let search_results = interleave_results(search_results, &fts_chunk_ids, request.top_k, 1);

    // Cross-encoder reranking: use LLM to re-score candidates for better precision
    let search_results = rerank_results(
        search_results,
        &request.question,
        request.top_k,
        state.llm_provider(),
    ).await;

    if search_results.is_empty() {
        let processing_time_ms = start.elapsed().as_millis() as u64;
        return Ok(Json(QueryResponse::not_found(processing_time_ms)));
    }

    // Create citations from search results
    let highlight_terms = extract_terms(&request.question);
    let highlight_refs: Vec<&str> = highlight_terms.iter().map(|s| s.as_str()).collect();
    let mut citations: Vec<Citation> = search_results
        .iter()
        .map(|r| {
            let mut citation = Citation::from_chunk(&r.chunk, r.similarity);
            citation.highlight_terms(&highlight_refs);
            if let Some(doc) = state.get_document(&r.chunk.document_id) {
                citation.enrich_with_document(&doc);
            }
            citation
        })
        .collect();

    // Dedup citations by content
    {
        let mut seen = HashSet::new();
        citations.retain(|c| {
            let key = c.snippet.chars().take(80).collect::<String>().to_lowercase();
            seen.insert(key)
        });
    }

    // Build context for LLM
    let context = PromptBuilder::build_context(&search_results);

    // Find similar past Q&A for learning
    let similar_qa = state.knowledge_store().find_similar(&request.question, 3);
    let past_qa: Vec<(String, String)> = similar_qa
        .iter()
        .filter(|qa| qa.feedback_score.unwrap_or(0) >= 0)  // Only use positive/neutral feedback
        .map(|qa| (qa.question.clone(), qa.answer.clone()))
        .collect();

    // Generate answer (using provider abstraction - Ollama or Gemini)
    let answer = if past_qa.is_empty() {
        state
            .llm_provider()
            .generate_answer(&request.question, &context, &citations)
            .await?
    } else {
        tracing::info!("Using {} learned examples for better answer", past_qa.len());
        state
            .llm_provider()
            .generate_with_learning(&request.question, &context, &citations, &past_qa)
            .await?
    };

    // Parse citations from answer and link them
    let (clean_answer, linked_citations) =
        crate::generation::citation::extract_and_link_citations(&answer, &mut citations);

    let processing_time_ms = start.elapsed().as_millis() as u64;

    let mut response = QueryResponse::new(clean_answer.clone(), linked_citations.clone(), processing_time_ms);
    response.chunks_retrieved = search_results.len();

    // Store this Q&A for learning
    let interaction = QAInteraction {
        id: Uuid::new_v4(),
        question: request.question.clone(),
        answer: clean_answer,
        citations_used: linked_citations.iter().map(|c| c.filename.clone()).collect(),
        relevance_score: search_results.first().map(|r| r.similarity).unwrap_or(0.0),
        feedback_score: None,  // Will be updated via feedback endpoint
        created_at: chrono::Utc::now(),
        document_ids: search_results.iter().map(|r| r.chunk.document_id).collect(),
    };
    let interaction_id = state.knowledge_store().store_interaction(interaction);
    response.interaction_id = Some(interaction_id);

    // Include raw chunks if requested
    if request.include_chunks {
        response.raw_chunks = Some(search_results.into_iter().map(|r| r.chunk).collect());
    }

    tracing::info!(
        "Query completed in {}ms, {} citations",
        processing_time_ms,
        response.citations.len()
    );

    Ok(Json(response))
}

/// Handle string search queries (literal text matching)
async fn string_search_query(
    state: &AppState,
    query: &str,
    organization_id: &str,
    start: Instant,
) -> Result<Json<QueryResponse>> {
    tracing::info!("String search: \"{}\" (org: {})", query, organization_id);

    // Perform literal string search (uses SQLite FTS for GCP, HNSW for local)
    let results = state.vector_store_provider().string_search(query, 10, organization_id).await?;

    let processing_time_ms = start.elapsed().as_millis() as u64;

    if results.is_empty() {
        return Ok(Json(QueryResponse::not_found(processing_time_ms)));
    }

    // Build citations from string search results
    let citations: Vec<Citation> = results
        .iter()
        .map(|r| Citation {
            chunk_id: r.chunk_id,
            document_id: r.document_id,
            filename: r.filename.clone(),
            file_type: r.file_type.clone(),
            page_number: r.page_number,
            section_title: None,
            line_start: None,
            line_end: None,
            snippet: r.preview.clone(),
            snippet_highlighted: r.highlighted_snippet.clone(),
            similarity_score: 1.0, // Exact match
            rerank_score: None,
            document_url: None,
            plaintext_url: None,
        })
        .collect();

    // Build answer summarizing string search results
    let total_matches: usize = results.iter().map(|r| r.match_count).sum();
    let unique_docs: std::collections::HashSet<Uuid> = results.iter().map(|r| r.document_id).collect();

    let answer = format!(
        "Found {} occurrences of \"{}\" across {} document(s).",
        total_matches, query, unique_docs.len()
    );

    let mut response = QueryResponse::new(answer, citations, processing_time_ms);
    response.chunks_retrieved = results.len();
    response.chunks_used = results.len();

    tracing::info!(
        "String search completed in {}ms, {} matches across {} docs",
        processing_time_ms,
        total_matches,
        unique_docs.len()
    );

    Ok(Json(response))
}

/// POST /api/string-search - Direct string search endpoint
pub async fn string_search(
    State(state): State<AppState>,
    claims: Option<Extension<AuthClaims>>,
    Json(request): Json<StringSearchRequest>,
) -> Result<Json<StringSearchResponse>> {
    // Check rate limit
    if !state.production_controls().allow_query() {
        return Err(crate::error::Error::RateLimited(
            "Too many requests. Please try again later.".to_string()
        ));
    }

    // Validate inputs
    request.validate()?;

    let start = Instant::now();

    // Get ACL-accessible document IDs
    let default_claims_ss = AuthClaims { user_id: String::new(), email: String::new(), organization_id: request.organization_id.clone(), is_platform_admin: false, exp: 0 };
    let effective_claims_ss = claims.as_ref().map(|Extension(c)| c).unwrap_or(&default_claims_ss);
    let acl_doc_ids = acl::resolve_acl_filter(&state, effective_claims_ss, None).await?;

    let mut results = state.vector_store_provider().string_search(
        &request.query,
        request.limit.unwrap_or(10) * 3, // Over-fetch to compensate for ACL filtering
        &request.organization_id,
    ).await?;

    // Filter results by ACL
    if let Some(ref allowed) = acl_doc_ids {
        let allowed_set: HashSet<Uuid> = allowed.iter().copied().collect();
        results.retain(|r| allowed_set.contains(&r.document_id));
        results.truncate(request.limit.unwrap_or(10));
    }

    let processing_time_ms = start.elapsed().as_millis() as u64;

    Ok(Json(StringSearchResponse::new(request.query, results, processing_time_ms)))
}

/// Request for string search endpoint
#[derive(Debug, serde::Deserialize)]
pub struct StringSearchRequest {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
    /// Organization ID for multi-tenancy (REQUIRED for tenant isolation)
    pub organization_id: String,
}

impl StringSearchRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<()> {
        validate_organization_id(&self.organization_id)?;
        validate_query(&self.query)?;
        Ok(())
    }
}

/// POST /api/v2/query - V2 Query endpoint with frontend-friendly format
pub async fn query_rag_v2(
    State(state): State<AppState>,
    claims: Option<Extension<AuthClaims>>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponseV2>> {
    // Check rate limit
    if !state.production_controls().allow_query() {
        return Err(crate::error::Error::RateLimited(
            "Too many requests. Please try again later.".to_string()
        ));
    }

    // Validate inputs
    validate_organization_id(&request.organization_id)?;
    validate_query(&request.question)?;

    let start = Instant::now();

    tracing::info!("V2 Query: \"{}\"", request.question);

    // Detect query type
    let query_type = QueryType::detect(&request.question);

    // For string search queries, use literal text matching
    if matches!(query_type, QueryType::StringSearch) {
        let results = state.vector_store_provider().string_search(&request.question, 10, &request.organization_id).await?;
        let processing_time_ms = start.elapsed().as_millis() as u64;

        let total_matches: usize = results.iter().map(|r| r.match_count).sum();
        let unique_docs: std::collections::HashSet<Uuid> = results.iter().map(|r| r.document_id).collect();

        let answer = if results.is_empty() {
            format!("No matches found for \"{}\".", request.question)
        } else {
            format!(
                "Found {} occurrences of \"{}\" across {} document(s).",
                total_matches, request.question, unique_docs.len()
            )
        };

        return Ok(Json(QueryResponseV2::from_string_search(
            answer,
            &results,
            processing_time_ms,
        )));
    }

    // Check cache first
    let doc_timestamps = state.get_document_timestamps();
    if let Some(cached) = state.answer_cache().get(&request.question, &request.organization_id, &doc_timestamps) {
        tracing::info!("Cache hit for query");

        // Build response from cached answer
        let citations: Vec<Citation> = cached.citations.iter().map(|c| {
            Citation {
                chunk_id: c.chunk_id,
                document_id: c.document_id,
                filename: c.filename.clone(),
                file_type: crate::types::FileType::Unknown,
                page_number: None,
                section_title: None,
                line_start: None,
                line_end: None,
                snippet: c.snippet.clone(),
                snippet_highlighted: c.snippet.clone(),
                similarity_score: c.similarity_score,
                rerank_score: None,
                document_url: None,
                plaintext_url: None,
            }
        }).collect();

        let mut response = QueryResponse::new(cached.answer.clone(), citations, start.elapsed().as_millis() as u64);
        response.chunks_retrieved = cached.citations.len();
        response.chunks_used = cached.citations.len();

        return Ok(Json(QueryResponseV2::from_response(
            &response,
            true,
            Some(CacheInfo {
                from_cache: true,
                hit_count: Some(cached.hit_count),
            }),
        )));
    }

    // Generate query embedding via HyDE (Hypothetical Document Embeddings):
    // The LLM first generates a hypothetical answer passage, which is then
    // embedded instead of the raw question for better retrieval quality.
    let query_embedding = generate_hyde_embedding(
        &request.question,
        state.llm_provider(),
        state.embedding_provider(),
    ).await?;

    // Build ACL-filtered search filter
    let default_claims = AuthClaims { user_id: String::new(), email: String::new(), organization_id: request.organization_id.clone(), is_platform_admin: false, exp: 0 };
    let effective_claims = claims.as_ref().map(|Extension(c)| c).unwrap_or(&default_claims);
    let filter = acl::build_acl_search_filter(&state, effective_claims, request.organization_id.clone(), request.document_filter.clone()).await?;

    // Search for relevant chunks (uses Vertex AI for GCP backend)
    let mut search_results: Vec<VectorSearchResult> = state.vector_store_provider().search(
        &query_embedding,
        request.top_k * 2,
        &filter,
    ).await?;

    // Enrich minimal chunks with full data from local store (Vertex AI workaround)
    for result in &mut search_results {
        if result.chunk.content.is_empty() || result.chunk.document_id.is_nil() {
            if let Some(full_chunk) = state.get_chunk(&result.chunk.id).await {
                tracing::debug!("V2: Enriched minimal chunk {} from local store", result.chunk.id);
                result.chunk = full_chunk;
            } else {
                tracing::warn!("V2: Chunk {} not found in local store, using minimal data", result.chunk.id);
            }
        }
    }

    // Hybrid FTS: supplement vector results with full-text search
    #[allow(unused_mut)]
    let mut fts_chunk_ids: HashSet<Uuid> = HashSet::new();
    #[cfg(feature = "postgres")]
    if let Some(pg_pool) = state.pg_pool() {
        fts_chunk_ids = hybrid_fts_supplement(pg_pool, &request.question, &request.organization_id, &mut search_results, &filter.document_ids).await;
    }

    // Filter by similarity threshold
    search_results.retain(|r| r.similarity >= request.similarity_threshold);

    // Interleave vector + FTS results within top_k budget
    let search_results = interleave_results(search_results, &fts_chunk_ids, request.top_k, 1);

    // Cross-encoder reranking: use LLM to re-score candidates for better precision
    let search_results = rerank_results(
        search_results,
        &request.question,
        request.top_k,
        state.llm_provider(),
    ).await;

    if search_results.is_empty() {
        let processing_time_ms = start.elapsed().as_millis() as u64;
        let response = QueryResponse::not_found(processing_time_ms);
        return Ok(Json(QueryResponseV2::from_response(&response, true, None)));
    }

    // Create citations from search results
    let v2_highlight_terms = extract_terms(&request.question);
    let v2_highlight_refs: Vec<&str> = v2_highlight_terms.iter().map(|s| s.as_str()).collect();
    let mut citations: Vec<Citation> = search_results
        .iter()
        .map(|r| {
            let mut citation = Citation::from_chunk(&r.chunk, r.similarity);
            citation.highlight_terms(&v2_highlight_refs);
            if let Some(doc) = state.get_document(&r.chunk.document_id) {
                citation.enrich_with_document(&doc);
            }
            citation
        })
        .collect();

    // Dedup citations by content (first 80 chars) to merge duplicate chunks
    {
        let mut seen = HashSet::new();
        citations.retain(|c| {
            let key = c.snippet.chars().take(80).collect::<String>().to_lowercase();
            seen.insert(key)
        });
    }

    // Build context for LLM
    let context = crate::generation::PromptBuilder::build_context(&search_results);

    // Find similar past Q&A for learning (same as v1)
    let similar_qa = state.knowledge_store().find_similar(&request.question, 3);
    let past_qa: Vec<(String, String)> = similar_qa
        .iter()
        .filter(|qa| qa.feedback_score.unwrap_or(0) >= 0)
        .map(|qa| (qa.question.clone(), qa.answer.clone()))
        .collect();

    // Generate answer (with learning if past Q&A available)
    let answer = if past_qa.is_empty() {
        state
            .llm_provider()
            .generate_answer(&request.question, &context, &citations)
            .await?
    } else {
        tracing::info!("V2: Using {} learned examples for better answer", past_qa.len());
        state
            .llm_provider()
            .generate_with_learning(&request.question, &context, &citations, &past_qa)
            .await?
    };

    // Parse citations and link them
    let (clean_answer, linked_citations) =
        crate::generation::citation::extract_and_link_citations(&answer, &mut citations);

    let processing_time_ms = start.elapsed().as_millis() as u64;

    let mut response = QueryResponse::new(clean_answer.clone(), linked_citations.clone(), processing_time_ms);
    response.chunks_retrieved = search_results.len();

    // Cache the answer
    let cached_citations: Vec<CachedCitation> = linked_citations.iter().map(|c| {
        CachedCitation {
            chunk_id: c.chunk_id,
            document_id: c.document_id,
            filename: c.filename.clone(),
            snippet: c.snippet.clone(),
            similarity_score: c.similarity_score,
        }
    }).collect();

    state.answer_cache().put(
        &request.question,
        &request.organization_id,
        clean_answer,
        cached_citations,
        doc_timestamps,
    );

    // Store Q&A for learning
    let interaction = crate::learning::knowledge_store::QAInteraction {
        id: Uuid::new_v4(),
        question: request.question.clone(),
        answer: response.answer.clone(),
        citations_used: linked_citations.iter().map(|c| c.filename.clone()).collect(),
        relevance_score: search_results.first().map(|r| r.similarity).unwrap_or(0.0),
        feedback_score: None,
        created_at: chrono::Utc::now(),
        document_ids: search_results.iter().map(|r| r.chunk.document_id).collect(),
    };
    let interaction_id = state.knowledge_store().store_interaction(interaction);
    response.interaction_id = Some(interaction_id);

    tracing::info!(
        "V2 Query completed in {}ms, {} citations",
        processing_time_ms,
        response.citations.len()
    );

    Ok(Json(QueryResponseV2::from_response(
        &response,
        true,
        Some(CacheInfo {
            from_cache: false,
            hit_count: None,
        }),
    )))
}

/// SSE event data for streaming query responses
#[derive(serde::Serialize)]
struct SseChunkData {
    /// Partial answer text
    text: String,
}

/// SSE event data for the final "done" event
#[derive(serde::Serialize)]
struct SseDoneData {
    /// Full, citation-cleaned answer
    answer: String,
    /// Linked citations
    citations: Vec<Citation>,
    /// Processing time in milliseconds
    processing_time_ms: u64,
    /// Interaction ID for feedback
    #[serde(skip_serializing_if = "Option::is_none")]
    interaction_id: Option<Uuid>,
}

/// POST /api/v2/query/stream - Streaming query endpoint (SSE)
///
/// Accepts the same request body as `query_rag_v2`. Performs identical
/// retrieval (embedding, vector search, FTS supplement) but streams the
/// LLM answer back as Server-Sent Events instead of waiting for the
/// full response.
///
/// Event types:
/// - `chunk`  — partial answer text (`{"text": "..."}`)
/// - `done`   — final event with full answer + citations JSON
/// - `error`  — an error occurred during generation
pub async fn query_rag_v2_stream(
    State(state): State<AppState>,
    claims: Option<Extension<AuthClaims>>,
    Json(request): Json<QueryRequest>,
) -> std::result::Result<
    Sse<std::pin::Pin<Box<dyn futures::Stream<Item = std::result::Result<Event, Infallible>> + Send>>>,
    crate::error::Error,
> {
    // Rate limit
    if !state.production_controls().allow_query() {
        return Err(crate::error::Error::RateLimited(
            "Too many requests. Please try again later.".to_string(),
        ));
    }

    // Validate inputs
    validate_organization_id(&request.organization_id)?;
    validate_query(&request.question)?;

    let start = Instant::now();
    tracing::info!("Stream Query: \"{}\"", request.question);

    // Detect query type — string searches are not streamed
    let query_type = QueryType::detect(&request.question);
    if matches!(query_type, QueryType::StringSearch) {
        // Return the string-search answer as a single SSE sequence
        let results = state
            .vector_store_provider()
            .string_search(&request.question, 10, &request.organization_id)
            .await?;
        let processing_time_ms = start.elapsed().as_millis() as u64;

        let total_matches: usize = results.iter().map(|r| r.match_count).sum();
        let unique_docs: std::collections::HashSet<Uuid> =
            results.iter().map(|r| r.document_id).collect();

        let answer = if results.is_empty() {
            format!("No matches found for \"{}\".", request.question)
        } else {
            format!(
                "Found {} occurrences of \"{}\" across {} document(s).",
                total_matches, request.question, unique_docs.len()
            )
        };

        let done = SseDoneData {
            answer: answer.clone(),
            citations: Vec::new(),
            processing_time_ms,
            interaction_id: None,
        };

        let stream: std::pin::Pin<Box<dyn futures::Stream<Item = std::result::Result<Event, Infallible>> + Send>> =
            Box::pin(futures::stream::iter(vec![
                Ok::<_, Infallible>(
                    Event::default()
                        .event("chunk")
                        .json_data(SseChunkData { text: answer })
                        .unwrap(),
                ),
                Ok(Event::default()
                    .event("done")
                    .json_data(done)
                    .unwrap()),
            ]));

        return Ok(Sse::new(stream).keep_alive(KeepAlive::default()));
    }

    // ---- Full RAG retrieval (same as query_rag_v2) ----

    let query_embedding = generate_hyde_embedding(
        &request.question,
        state.llm_provider(),
        state.embedding_provider(),
    ).await?;

    // Build ACL-filtered search filter
    let default_claims = AuthClaims { user_id: String::new(), email: String::new(), organization_id: request.organization_id.clone(), is_platform_admin: false, exp: 0 };
    let effective_claims = claims.as_ref().map(|Extension(c)| c).unwrap_or(&default_claims);
    let filter = acl::build_acl_search_filter(&state, effective_claims, request.organization_id.clone(), request.document_filter.clone()).await?;

    let mut search_results: Vec<VectorSearchResult> = state
        .vector_store_provider()
        .search(&query_embedding, request.top_k * 2, &filter)
        .await?;

    // Enrich minimal chunks
    for result in &mut search_results {
        if result.chunk.content.is_empty() || result.chunk.document_id.is_nil() {
            if let Some(full_chunk) = state.get_chunk(&result.chunk.id).await {
                result.chunk = full_chunk;
            }
        }
    }

    // Hybrid FTS supplement
    #[allow(unused_mut)]
    let mut fts_chunk_ids: HashSet<Uuid> = HashSet::new();
    #[cfg(feature = "postgres")]
    if let Some(pg_pool) = state.pg_pool() {
        fts_chunk_ids = hybrid_fts_supplement(
            pg_pool,
            &request.question,
            &request.organization_id,
            &mut search_results,
            &filter.document_ids,
        )
        .await;
    }

    // Filter + interleave
    search_results.retain(|r| r.similarity >= request.similarity_threshold);
    let search_results = interleave_results(search_results, &fts_chunk_ids, request.top_k, 1);

    // Cross-encoder reranking: use LLM to re-score candidates for better precision
    let search_results = rerank_results(
        search_results,
        &request.question,
        request.top_k,
        state.llm_provider(),
    ).await;

    if search_results.is_empty() {
        let processing_time_ms = start.elapsed().as_millis() as u64;
        let done = SseDoneData {
            answer: "No relevant documents found for your query.".to_string(),
            citations: Vec::new(),
            processing_time_ms,
            interaction_id: None,
        };
        let stream: std::pin::Pin<Box<dyn futures::Stream<Item = std::result::Result<Event, Infallible>> + Send>> =
            Box::pin(futures::stream::iter(vec![Ok::<_, Infallible>(
                Event::default()
                    .event("done")
                    .json_data(done)
                    .unwrap(),
            )]));
        return Ok(Sse::new(stream).keep_alive(KeepAlive::default()));
    }

    // Build citations
    let highlight_terms = extract_terms(&request.question);
    let highlight_refs: Vec<&str> = highlight_terms.iter().map(|s| s.as_str()).collect();
    let mut citations: Vec<Citation> = search_results
        .iter()
        .map(|r| {
            let mut citation = Citation::from_chunk(&r.chunk, r.similarity);
            citation.highlight_terms(&highlight_refs);
            if let Some(doc) = state.get_document(&r.chunk.document_id) {
                citation.enrich_with_document(&doc);
            }
            citation
        })
        .collect();

    // Dedup citations by content
    {
        let mut seen = HashSet::new();
        citations.retain(|c| {
            let key = c.snippet.chars().take(80).collect::<String>().to_lowercase();
            seen.insert(key)
        });
    }

    // Build context
    let context = crate::generation::PromptBuilder::build_context(&search_results);

    // Get the LLM stream
    let llm_stream = state
        .llm_provider()
        .generate_stream(&request.question, &context, &citations)
        .await?;

    // Capture owned copies for the async stream closure
    let question_owned = request.question.clone();
    let doc_ids: Vec<Uuid> = search_results.iter().map(|r| r.chunk.document_id).collect();
    let first_similarity = search_results.first().map(|r| r.similarity).unwrap_or(0.0);

    // Build the SSE stream: emit each LLM chunk, then a final "done" event
    let sse_stream = async_stream::stream! {
        let mut full_answer = String::new();

        // Forward LLM chunks
        let mut llm_stream = std::pin::pin!(llm_stream);
        while let Some(chunk_result) = llm_stream.next().await {
            match chunk_result {
                Ok(text) => {
                    full_answer.push_str(&text);
                    yield Ok::<_, Infallible>(
                        Event::default()
                            .event("chunk")
                            .json_data(SseChunkData { text })
                            .unwrap(),
                    );
                }
                Err(e) => {
                    tracing::error!("LLM stream error: {}", e);
                    yield Ok(Event::default()
                        .event("error")
                        .data(format!("LLM generation error: {}", e)));
                    return;
                }
            }
        }

        // Post-process: extract/link citations
        let (clean_answer, linked_citations) =
            crate::generation::citation::extract_and_link_citations(&full_answer, &mut citations);

        let processing_time_ms = start.elapsed().as_millis() as u64;

        // Store Q&A for learning
        let interaction = crate::learning::knowledge_store::QAInteraction {
            id: Uuid::new_v4(),
            question: question_owned.clone(),
            answer: clean_answer.clone(),
            citations_used: linked_citations.iter().map(|c| c.filename.clone()).collect(),
            relevance_score: first_similarity,
            feedback_score: None,
            created_at: chrono::Utc::now(),
            document_ids: doc_ids,
        };
        let interaction_id = state.knowledge_store().store_interaction(interaction);

        // Emit final done event
        let done = SseDoneData {
            answer: clean_answer,
            citations: linked_citations,
            processing_time_ms,
            interaction_id: Some(interaction_id),
        };
        yield Ok(Event::default()
            .event("done")
            .json_data(done)
            .unwrap());

        tracing::info!(
            "Stream Query completed in {}ms",
            processing_time_ms,
        );
    };

    let boxed_stream: std::pin::Pin<Box<dyn futures::Stream<Item = std::result::Result<Event, Infallible>> + Send>> =
        Box::pin(sse_stream);
    Ok(Sse::new(boxed_stream).keep_alive(KeepAlive::default()))
}
