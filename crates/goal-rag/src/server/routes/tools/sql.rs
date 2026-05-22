//! SQL escape hatch for LLM agents
//!
//! Read-only, timeout-bounded, schema-restricted SQL execution.
//! Safety layers: READ ONLY transaction, 5s timeout, api-only search_path,
//! keyword validation (defense-in-depth).

use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use super::{ToolResult, parse_str_opt};

// ============================================================================
// run_sql — read-only SQL against api.* schema
// ============================================================================

pub async fn run_sql(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let sql = parse_str_opt(params, "sql")
        .ok_or_else(|| Error::Validation("sql is required".into()))?;
    let limit = params.get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(100)
        .clamp(1, 100) as i64;

    // Strip trailing semicolons (would break subquery wrapping)
    let sql = sql.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err(Error::Validation("sql is empty".into()));
    }

    // Validate: only SELECT/WITH, no DDL/DML
    validate_sql(sql)?;

    let client = pool.get().await?;

    // Safety: read-only transaction + 5s timeout + api-only search_path
    client.batch_execute(
        "BEGIN READ ONLY; SET LOCAL statement_timeout = '5000'; SET LOCAL search_path = 'api';"
    ).await.map_err(|e| Error::Internal(format!("Failed to begin read-only txn: {}", e)))?;

    // Wrap in row_to_json for automatic type→JSON conversion by PostgreSQL.
    // $1 is always the org_uuid — LLM agents use it for organization filtering.
    let wrapped = format!(
        "SELECT row_to_json(__sq) FROM ({}) __sq LIMIT {}",
        sql, limit
    );

    let result = client
        .query(&wrapped, &[org_uuid as &(dyn tokio_postgres::types::ToSql + Sync)])
        .await;

    // Always rollback (read-only, nothing to commit)
    let _ = client.batch_execute("ROLLBACK").await;

    match result {
        Ok(rows) => {
            let json_rows: Vec<Value> = rows
                .iter()
                .map(|r| r.get::<_, Value>(0))
                .collect();
            let count = json_rows.len();

            // Extract column names from first row's keys
            let columns: Vec<String> = json_rows
                .first()
                .and_then(|v| v.as_object())
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();

            let summary = format!(
                "Query returned {} row{}, {} column{}: {}",
                count,
                if count == 1 { "" } else { "s" },
                columns.len(),
                if columns.len() == 1 { "" } else { "s" },
                if columns.is_empty() { "(none)".to_string() } else { columns.join(", ") }
            );

            Ok(ToolResult::ok(
                json!({ "rows": json_rows, "columns": columns, "truncated": count as i64 == limit }),
                summary,
                count,
                0,
            ))
        }
        Err(e) => {
            let msg = e.to_string();
            // Surface timeout errors clearly
            if msg.contains("statement timeout") {
                Err(Error::Validation("Query exceeded 5 second timeout. Simplify the query or add more filters.".into()))
            } else {
                Err(Error::Validation(format!("SQL error: {}", msg)))
            }
        }
    }
}

// ============================================================================
// SQL validation (defense-in-depth; READ ONLY txn is the primary safety net)
// ============================================================================

fn validate_sql(sql: &str) -> Result<()> {
    let upper = sql.to_uppercase();

    // Must start with SELECT or WITH
    let first_word = upper.split_whitespace().next().unwrap_or("");
    if first_word != "SELECT" && first_word != "WITH" {
        return Err(Error::Validation(
            "Only SELECT and WITH queries are allowed".into(),
        ));
    }

    // Reject DDL/DML keywords (word-boundary match)
    const FORBIDDEN: &[&str] = &[
        "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE",
        "GRANT", "REVOKE", "COPY", "EXECUTE", "CALL",
    ];
    for kw in FORBIDDEN {
        if contains_word(&upper, kw) {
            return Err(Error::Validation(format!(
                "Forbidden keyword in SQL: {}",
                kw
            )));
        }
    }

    // Reject access to non-api schemas
    let lower = sql.to_lowercase();
    const BLOCKED_SCHEMAS: &[&str] = &[
        "pg_catalog.", "information_schema.", "public.",
    ];
    for schema in BLOCKED_SCHEMAS {
        if lower.contains(schema) {
            return Err(Error::Validation(format!(
                "Access to {} schema is not allowed. Use api.* tables or unqualified names.",
                schema.trim_end_matches('.')
            )));
        }
    }

    Ok(())
}

/// Check if `haystack` (already UPPER) contains `word` at a word boundary.
/// Word boundary = start/end of string, or a non-alphanumeric, non-underscore character.
fn contains_word(haystack: &str, word: &str) -> bool {
    let h = haystack.as_bytes();
    let w = word.as_bytes();
    let wl = w.len();

    if h.len() < wl {
        return false;
    }

    for i in 0..=(h.len() - wl) {
        if &h[i..i + wl] == w {
            let before_ok =
                i == 0 || !(h[i - 1].is_ascii_alphanumeric() || h[i - 1] == b'_');
            let after_ok =
                i + wl >= h.len() || !(h[i + wl].is_ascii_alphanumeric() || h[i + wl] == b'_');
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}
