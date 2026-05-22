//! Search tools for LLM agents
//!
//! Text search (ILIKE) on api.* tables, semantic search via entity embeddings,
//! and cross-reference search across both document and entity embeddings.

use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use crate::providers::entity_embeddings::EntityEmbeddingStore;
use crate::providers::{EmbeddingProvider, VectorStoreProvider};
use crate::providers::vector_store::SearchFilter;
use super::{ToolResult, parse_uuid, parse_uuid_opt, parse_str_opt, parse_limit};

// ============================================================================
// search_tasks — ILIKE on title + description
// ============================================================================

pub async fn search_tasks(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let query = parse_str_opt(params, "query")
        .ok_or_else(|| Error::Validation("query is required".into()))?;
    let limit = parse_limit(params, 20, 100);

    let client = pool.get().await?;

    // Build dynamic WHERE clause for optional filters
    let mut conditions = vec![
        "t.organization_id = $1".to_string(),
        "t.is_deleted = false".to_string(),
        "(t.title ILIKE '%' || $2 || '%' OR t.description ILIKE '%' || $2 || '%')".to_string(),
    ];
    let mut sql_params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![
        Box::new(*org_uuid),
        Box::new(query.to_string()),
    ];
    let mut idx = 3u32;

    if let Some(status) = parse_str_opt(params, "status") {
        conditions.push(format!("t.status = ${}", idx));
        sql_params.push(Box::new(status.to_string()));
        idx += 1;
    }
    if let Some(priority) = parse_str_opt(params, "priority") {
        conditions.push(format!("t.priority = ${}", idx));
        sql_params.push(Box::new(priority.to_string()));
        idx += 1;
    }
    if let Some(assigned_to) = parse_uuid_opt(params, "assigned_to")? {
        conditions.push(format!("t.assigned_to = ${}", idx));
        sql_params.push(Box::new(assigned_to));
        idx += 1;
    }

    conditions.push(format!("TRUE")); // simplify trailing AND
    let limit_param = idx;
    sql_params.push(Box::new(limit));

    let sql = format!(
        "SELECT t.id, t.title, t.status, t.priority, t.due_date,
                (u.first_name || ' ' || u.last_name) AS assignee_name,
                t.assigned_to, t.goal_id
         FROM api.tasks t
         LEFT JOIN api.users u ON u.id = t.assigned_to
         WHERE {}
         ORDER BY t.updated_at DESC
         LIMIT ${}",
        conditions.join(" AND "),
        limit_param
    );

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        sql_params.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

    let rows = client.query(&sql, &param_refs).await
        .map_err(|e| Error::Internal(format!("search_tasks failed: {}", e)))?;

    let tasks: Vec<Value> = rows.iter().map(|row| {
        json!({
            "id": row.get::<_, Uuid>(0).to_string(),
            "title": row.get::<_, String>(1),
            "status": row.get::<_, String>(2),
            "priority": row.get::<_, String>(3),
            "due_date": row.get::<_, Option<chrono::NaiveDate>>(4).map(|d| d.to_string()),
            "assignee_name": row.get::<_, Option<String>>(5),
            "assigned_to": row.get::<_, Option<Uuid>>(6).map(|u| u.to_string()),
            "goal_id": row.get::<_, Option<Uuid>>(7).map(|u| u.to_string()),
        })
    }).collect();

    let count = tasks.len();
    let summary = format!("Found {} tasks matching \"{}\"", count, query);

    Ok(ToolResult::ok(json!({ "tasks": tasks }), summary, count, 0))
}

// ============================================================================
// search_users — ILIKE on first_name, last_name, email
// ============================================================================

pub async fn search_users(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let query = parse_str_opt(params, "query")
        .ok_or_else(|| Error::Validation("query is required".into()))?;
    let limit = parse_limit(params, 20, 100);

    let client = pool.get().await?;

    let rows = client.query(
        "SELECT id, (first_name || ' ' || last_name) AS name, email, title
         FROM api.users
         WHERE organization_id = $1 AND is_deleted = false
           AND (first_name ILIKE '%' || $2 || '%'
                OR last_name ILIKE '%' || $2 || '%'
                OR email ILIKE '%' || $2 || '%')
         ORDER BY last_name, first_name
         LIMIT $3",
        &[org_uuid, &query.to_string(), &limit],
    ).await.map_err(|e| Error::Internal(format!("search_users failed: {}", e)))?;

    let users: Vec<Value> = rows.iter().map(|row| {
        json!({
            "id": row.get::<_, Uuid>(0).to_string(),
            "name": row.get::<_, String>(1),
            "email": row.get::<_, Option<String>>(2),
            "title": row.get::<_, Option<String>>(3),
        })
    }).collect();

    let count = users.len();
    let summary = format!("Found {} users matching \"{}\"", count, query);

    Ok(ToolResult::ok(json!({ "users": users }), summary, count, 0))
}

// ============================================================================
// semantic_search — pgvector entity embedding similarity
// ============================================================================

pub async fn semantic_search(
    store: &Arc<EntityEmbeddingStore>,
    org_uuid: &Uuid,
    params: &Value,
) -> Result<ToolResult> {
    let query = parse_str_opt(params, "query")
        .ok_or_else(|| Error::Validation("query is required".into()))?;
    let entity_type = parse_str_opt(params, "entity_type");
    let top_k = params.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let top_k = top_k.clamp(1, 50);

    // Entity embeddings store org_id as UUID string
    let org_id_str = org_uuid.to_string();

    let results = store.search_similar(query, &org_id_str, entity_type, top_k).await?;

    let items: Vec<Value> = results.iter().map(|r| {
        json!({
            "entity_type": r.entity_type,
            "entity_id": r.entity_id.to_string(),
            "content": r.content,
            "similarity": r.similarity,
            "status": r.status,
            "priority": r.priority,
            "sentiment": r.sentiment,
            "source_tool": r.source_tool,
        })
    }).collect();

    let count = items.len();
    let type_info = entity_type.map(|t| format!(" (type: {})", t)).unwrap_or_default();
    let summary = format!("Found {} semantically similar entities{} for \"{}\"", count, type_info, query);

    Ok(ToolResult::ok(json!({ "results": items }), summary, count, 0))
}

// ============================================================================
// find_similar — Find entities similar to a given entity
// ============================================================================

pub async fn find_similar(
    store: &Arc<EntityEmbeddingStore>,
    org_uuid: &Uuid,
    params: &Value,
) -> Result<ToolResult> {
    let entity_type = parse_str_opt(params, "entity_type")
        .ok_or_else(|| Error::Validation("entity_type is required".into()))?;
    let entity_id = parse_uuid(params, "entity_id")?;
    let search_type = parse_str_opt(params, "search_type");
    let top_k = params.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let top_k = top_k.clamp(1, 50);

    let org_id_str = org_uuid.to_string();

    let results = store.search_similar_to_entity(
        entity_type, &entity_id, &org_id_str, search_type, top_k,
    ).await?;

    let items: Vec<Value> = results.iter().map(|r| {
        json!({
            "entity_type": r.entity_type,
            "entity_id": r.entity_id.to_string(),
            "content": r.content,
            "similarity": r.similarity,
            "status": r.status,
            "priority": r.priority,
            "sentiment": r.sentiment,
            "source_tool": r.source_tool,
        })
    }).collect();

    let count = items.len();
    let summary = format!(
        "Found {} entities similar to {} {}",
        count, entity_type, entity_id
    );

    Ok(ToolResult::ok(json!({ "results": items }), summary, count, 0))
}

// ============================================================================
// enriched_search — Cross-reference document + entity embeddings
// ============================================================================

pub async fn enriched_search(
    vector_store: &Arc<dyn VectorStoreProvider>,
    embedding_provider: &Arc<dyn EmbeddingProvider>,
    entity_store: &Arc<EntityEmbeddingStore>,
    org_uuid: &Uuid,
    params: &Value,
) -> Result<ToolResult> {
    let query = parse_str_opt(params, "query")
        .ok_or_else(|| Error::Validation("query is required".into()))?;
    let top_k = params.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let top_k = top_k.clamp(1, 50);
    let include = parse_str_opt(params, "include").unwrap_or("both");

    // Embed the query text once, reuse for both searches
    let query_embedding = embedding_provider.embed(query).await?;

    // rag_chunks stores org_id as slugs, entity_embeddings stores as UUIDs
    let org_slug = parse_str_opt(params, "organization_id").unwrap_or_default().to_string();
    let org_uuid_str = org_uuid.to_string();

    // Search document embeddings (rag_chunks) — uses slug
    let documents = if include == "both" || include == "documents" {
        let filter = SearchFilter::new(org_slug);
        let doc_results = vector_store.search(&query_embedding, top_k, &filter).await?;
        doc_results.iter().map(|r| {
            json!({
                "source": "document",
                "filename": r.chunk.source.filename,
                "content": r.chunk.content,
                "similarity": r.similarity,
                "page_number": r.chunk.source.page_number,
                "section_title": r.chunk.source.section_title,
                "document_id": r.chunk.document_id.to_string(),
                "chunk_index": r.chunk.chunk_index,
            })
        }).collect::<Vec<Value>>()
    } else {
        Vec::new()
    };

    // Search entity embeddings — uses UUID
    let entities = if include == "both" || include == "entities" {
        let entity_results = entity_store.search_by_embedding(
            &query_embedding, &org_uuid_str, None, top_k,
        ).await?;
        entity_results.iter().map(|r| {
            json!({
                "source": "entity",
                "entity_type": r.entity_type,
                "entity_id": r.entity_id.to_string(),
                "content": r.content,
                "similarity": r.similarity,
                "status": r.status,
                "priority": r.priority,
                "sentiment": r.sentiment,
                "source_tool": r.source_tool,
            })
        }).collect::<Vec<Value>>()
    } else {
        Vec::new()
    };

    let doc_count = documents.len();
    let entity_count = entities.len();
    let total = doc_count + entity_count;

    let summary = format!(
        "Enriched search for \"{}\": {} document chunks + {} entities = {} total results",
        query, doc_count, entity_count, total
    );

    Ok(ToolResult::ok(
        json!({
            "documents": documents,
            "entities": entities,
            "query": query,
        }),
        summary,
        total,
        0,
    ))
}
