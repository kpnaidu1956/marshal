//! Write tools for LLM agents
//!
//! All tools enforce organization_id filtering server-side.
//! Mutations target the `api.*` schema with audit logging.
//! CDC triggers auto-fire entity embedding updates on writes.

use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use super::{ToolResult, parse_uuid, parse_uuid_opt, parse_str_opt};

// ============================================================================
// Audit logging helpers
// ============================================================================

/// Ensure agent_audit_log table exists (runs once per server lifetime)
pub async fn ensure_audit_table(pool: &Arc<PgPool>) {
    static INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    INIT.get_or_init(|| async {
        if let Ok(client) = pool.get().await {
            if let Err(e) = client.execute(
                "CREATE TABLE IF NOT EXISTS api.agent_audit_log (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    organization_id UUID NOT NULL REFERENCES api.organizations(id),
                    tool_name TEXT NOT NULL,
                    agent_id TEXT,
                    parameters JSONB NOT NULL,
                    result_summary TEXT,
                    success BOOLEAN NOT NULL,
                    error_message TEXT,
                    created_at TIMESTAMPTZ DEFAULT NOW()
                )",
                &[],
            ).await {
                tracing::warn!("Failed to create agent_audit_log table: {}", e);
            }
        }
    }).await;
}

/// Log tool execution to agent_audit_log (fire-and-forget)
async fn log_audit(
    pool: &Arc<PgPool>,
    org_uuid: &Uuid,
    tool_name: &str,
    params: &Value,
    summary: &str,
    success: bool,
    error_msg: Option<&str>,
) {
    if let Ok(client) = pool.get().await {
        if let Err(e) = client.execute(
            "INSERT INTO api.agent_audit_log
             (organization_id, tool_name, parameters, result_summary, success, error_message)
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[org_uuid, &tool_name, params, &summary, &success, &error_msg],
        ).await {
            tracing::warn!("Audit log insert failed: {}", e);
        }
    }
}

/// Log task mutation to task_activity_logs
async fn log_task_activity(
    pool: &Arc<PgPool>,
    org_uuid: &Uuid,
    task_id: &Uuid,
    action: &str,
    changes: &Value,
    changed_by: Option<&Uuid>,
    changed_by_name: Option<&str>,
) {
    if let Ok(client) = pool.get().await {
        if let Err(e) = client.execute(
            "INSERT INTO api.task_activity_logs
             (task_id, action, changes, changed_by, changed_by_name, organization_id)
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[task_id, &action, changes, &changed_by, &changed_by_name, org_uuid],
        ).await {
            tracing::warn!("Task activity log insert failed: {}", e);
        }
    }
}

// ============================================================================
// Entity embedding source_tool stamping (fire-and-forget)
// ============================================================================

/// Stamp the source_tool on an entity's embedding after a write tool mutation.
/// Runs with a small delay to let the CDC-triggered embedding complete first.
fn stamp_source_tool(pool: &Arc<PgPool>, entity_type: &str, entity_id: &Uuid, tool_name: &str) {
    let pool = Arc::clone(pool);
    let entity_type = entity_type.to_string();
    let entity_id = *entity_id;
    let tool_name = tool_name.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if let Ok(client) = pool.get().await {
            let _ = client.execute(
                "UPDATE entity_embeddings SET source_tool = $1
                 WHERE entity_type = $2 AND entity_id = $3",
                &[&tool_name, &entity_type, &entity_id],
            ).await;
        }
    });
}

// ============================================================================
// create_task
// ============================================================================

pub async fn create_task(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    ensure_audit_table(pool).await;

    let title = parse_str_opt(params, "title")
        .ok_or_else(|| Error::Validation("title is required".into()))?;
    let description = parse_str_opt(params, "description")
        .ok_or_else(|| Error::Validation("description is required".into()))?;
    let priority = parse_str_opt(params, "priority")
        .ok_or_else(|| Error::Validation("priority is required".into()))?;
    let due_date_str = parse_str_opt(params, "due_date")
        .ok_or_else(|| Error::Validation("due_date is required (YYYY-MM-DD)".into()))?;
    let created_by = parse_uuid(params, "created_by")?;

    let due_date = chrono::NaiveDate::parse_from_str(due_date_str, "%Y-%m-%d")
        .map_err(|_| Error::Validation(format!("Invalid due_date: {}. Use YYYY-MM-DD", due_date_str)))?;

    let status = parse_str_opt(params, "status");
    let assigned_to = parse_uuid_opt(params, "assigned_to")?;
    let goal_id = parse_uuid_opt(params, "goal_id")?;

    let client = pool.get().await?;

    // Validate FK: created_by exists in org
    let user_exists = client.query_opt(
        "SELECT 1 FROM api.users WHERE id = $1 AND organization_id = $2 AND is_deleted = false",
        &[&created_by, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
    if user_exists.is_none() {
        return Err(Error::Validation(format!("User {} not found in organization", created_by)));
    }

    // Validate FK: assigned_to if provided
    if let Some(ref uid) = assigned_to {
        let exists = client.query_opt(
            "SELECT 1 FROM api.users WHERE id = $1 AND organization_id = $2 AND is_deleted = false",
            &[uid, org_uuid],
        ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
        if exists.is_none() {
            return Err(Error::Validation(format!("Assigned user {} not found in organization", uid)));
        }
    }

    // Validate FK: goal_id if provided
    if let Some(ref gid) = goal_id {
        let exists = client.query_opt(
            "SELECT 1 FROM api.goals WHERE id = $1 AND organization_id = $2",
            &[gid, org_uuid],
        ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
        if exists.is_none() {
            return Err(Error::Validation(format!("Goal {} not found in organization", gid)));
        }
    }

    let status_val = status.unwrap_or("Assigned");

    let row = client.query_one(
        "INSERT INTO api.tasks (title, description, priority, status, due_date,
                                created_by, assigned_to, goal_id, organization_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, title, status, priority, due_date, created_at",
        &[&title, &description, &priority, &status_val, &due_date,
          &created_by, &assigned_to, &goal_id, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("create_task failed: {}", e)))?;

    let task_id: Uuid = row.get(0);
    let result_title: String = row.get(1);

    let task = json!({
        "id": task_id.to_string(),
        "title": &result_title,
        "status": row.get::<_, String>(2),
        "priority": row.get::<_, String>(3),
        "due_date": row.get::<_, chrono::NaiveDate>(4).to_string(),
        "created_at": row.get::<_, chrono::DateTime<chrono::Utc>>(5).to_rfc3339(),
        "assigned_to": assigned_to.map(|u| u.to_string()),
        "goal_id": goal_id.map(|u| u.to_string()),
    });

    let summary = format!("Created task \"{}\" [{}]", result_title, status_val);

    log_audit(pool, org_uuid, "create_task", params, &summary, true, None).await;
    log_task_activity(
        pool, org_uuid, &task_id, "created", params, Some(&created_by), None,
    ).await;
    stamp_source_tool(pool, "task", &task_id, "create_task");

    Ok(ToolResult::ok(task, summary, 1, 0))
}

// ============================================================================
// update_task - Dynamic SET clause, at least 1 field required
// ============================================================================

pub async fn update_task(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    ensure_audit_table(pool).await;

    let task_id = parse_uuid(params, "task_id")?;
    let client = pool.get().await?;

    let mut sets = Vec::new();
    let mut sql_params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![
        Box::new(task_id), Box::new(*org_uuid),
    ];
    let mut idx = 3u32;

    if let Some(v) = parse_str_opt(params, "title") {
        sets.push(format!("title = ${}", idx));
        sql_params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = parse_str_opt(params, "description") {
        sets.push(format!("description = ${}", idx));
        sql_params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = parse_str_opt(params, "status") {
        sets.push(format!("status = ${}", idx));
        sql_params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = parse_str_opt(params, "priority") {
        sets.push(format!("priority = ${}", idx));
        sql_params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(uid) = parse_uuid_opt(params, "assigned_to")? {
        let exists = client.query_opt(
            "SELECT 1 FROM api.users WHERE id = $1 AND organization_id = $2 AND is_deleted = false",
            &[&uid, org_uuid],
        ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
        if exists.is_none() {
            return Err(Error::Validation(format!("User {} not found in organization", uid)));
        }
        sets.push(format!("assigned_to = ${}", idx));
        sql_params.push(Box::new(uid));
        idx += 1;
    }
    if let Some(v) = parse_str_opt(params, "due_date") {
        let d = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| Error::Validation(format!("Invalid due_date: {}. Use YYYY-MM-DD", v)))?;
        sets.push(format!("due_date = ${}", idx));
        sql_params.push(Box::new(d));
        idx += 1;
    }
    if let Some(gid) = parse_uuid_opt(params, "goal_id")? {
        let exists = client.query_opt(
            "SELECT 1 FROM api.goals WHERE id = $1 AND organization_id = $2",
            &[&gid, org_uuid],
        ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
        if exists.is_none() {
            return Err(Error::Validation(format!("Goal {} not found in organization", gid)));
        }
        sets.push(format!("goal_id = ${}", idx));
        sql_params.push(Box::new(gid));
        idx += 1;
    }

    let _ = idx; // suppress unused warning

    if sets.is_empty() {
        return Err(Error::Validation("At least one field to update is required".into()));
    }

    let changed_fields: Vec<&str> = sets.iter()
        .map(|s| s.split(" =").next().unwrap_or(""))
        .collect();

    let sql = format!(
        "UPDATE api.tasks SET {} WHERE id = $1 AND organization_id = $2
         RETURNING id, title, status, priority, due_date,
                   assigned_to, goal_id, updated_at",
        sets.join(", ")
    );

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        sql_params.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

    let row = client.query_opt(&sql, &param_refs).await
        .map_err(|e| Error::Internal(format!("update_task failed: {}", e)))?;

    let row = match row {
        Some(r) => r,
        None => return Ok(ToolResult::ok(Value::Null, format!("Task {} not found", task_id), 0, 0)),
    };

    let result_title: String = row.get(1);

    let task = json!({
        "id": row.get::<_, Uuid>(0).to_string(),
        "title": &result_title,
        "status": row.get::<_, String>(2),
        "priority": row.get::<_, String>(3),
        "due_date": row.get::<_, chrono::NaiveDate>(4).to_string(),
        "assigned_to": row.get::<_, Option<Uuid>>(5).map(|u| u.to_string()),
        "goal_id": row.get::<_, Option<Uuid>>(6).map(|u| u.to_string()),
        "updated_at": row.get::<_, chrono::DateTime<chrono::Utc>>(7).to_rfc3339(),
    });

    let summary = format!("Updated task \"{}\" ({})", result_title, changed_fields.join(", "));

    log_audit(pool, org_uuid, "update_task", params, &summary, true, None).await;
    log_task_activity(pool, org_uuid, &task_id, "updated", params, None, None).await;
    stamp_source_tool(pool, "task", &task_id, "update_task");

    Ok(ToolResult::ok(task, summary, 1, 0))
}

// ============================================================================
// create_goal
// ============================================================================

pub async fn create_goal(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    ensure_audit_table(pool).await;

    let title = parse_str_opt(params, "title")
        .ok_or_else(|| Error::Validation("title is required".into()))?;

    let description = parse_str_opt(params, "description");
    let status = parse_str_opt(params, "status");
    let created_by = parse_uuid_opt(params, "created_by")?;
    let parent_goal_id = parse_uuid_opt(params, "parent_goal_id")?;

    let target_date = match parse_str_opt(params, "target_date") {
        Some(s) => Some(chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| Error::Validation(format!("Invalid target_date: {}. Use YYYY-MM-DD", s)))?),
        None => None,
    };

    let client = pool.get().await?;

    // Validate parent goal exists in org
    if let Some(ref pid) = parent_goal_id {
        let exists = client.query_opt(
            "SELECT 1 FROM api.goals WHERE id = $1 AND organization_id = $2",
            &[pid, org_uuid],
        ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
        if exists.is_none() {
            return Err(Error::Validation(format!("Parent goal {} not found in organization", pid)));
        }
    }

    let row = client.query_one(
        "INSERT INTO api.goals (title, description, status, target_date,
                                parent_goal_id, created_by, organization_id)
         VALUES ($1, $2, COALESCE($3, 'not_started'), $4, $5, $6, $7)
         RETURNING id, title, status, created_at",
        &[&title, &description, &status, &target_date,
          &parent_goal_id, &created_by, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("create_goal failed: {}", e)))?;

    let goal_id: Uuid = row.get(0);
    let result_title: String = row.get(1);

    let goal = json!({
        "id": goal_id.to_string(),
        "title": &result_title,
        "status": row.get::<_, Option<String>>(2),
        "created_at": row.get::<_, Option<chrono::DateTime<chrono::Utc>>>(3)
            .map(|d| d.to_rfc3339()),
        "parent_goal_id": parent_goal_id.map(|u| u.to_string()),
        "target_date": target_date.map(|d| d.to_string()),
    });

    let summary = format!("Created goal \"{}\"", result_title);

    log_audit(pool, org_uuid, "create_goal", params, &summary, true, None).await;
    stamp_source_tool(pool, "goal", &goal_id, "create_goal");

    Ok(ToolResult::ok(goal, summary, 1, 0))
}

// ============================================================================
// update_goal - Dynamic SET clause
// ============================================================================

pub async fn update_goal(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    ensure_audit_table(pool).await;

    let goal_id = parse_uuid(params, "goal_id")?;
    let client = pool.get().await?;

    let mut sets = Vec::new();
    let mut sql_params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![
        Box::new(goal_id), Box::new(*org_uuid),
    ];
    let mut idx = 3u32;

    if let Some(v) = parse_str_opt(params, "title") {
        sets.push(format!("title = ${}", idx));
        sql_params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = parse_str_opt(params, "description") {
        sets.push(format!("description = ${}", idx));
        sql_params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = parse_str_opt(params, "status") {
        sets.push(format!("status = ${}", idx));
        sql_params.push(Box::new(v.to_string()));
        idx += 1;
    }
    if let Some(v) = params.get("progress").and_then(|v| v.as_i64()) {
        sets.push(format!("progress = ${}", idx));
        sql_params.push(Box::new(v as i32));
        idx += 1;
    }
    if let Some(v) = parse_str_opt(params, "target_date") {
        let d = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| Error::Validation(format!("Invalid target_date: {}. Use YYYY-MM-DD", v)))?;
        sets.push(format!("target_date = ${}", idx));
        sql_params.push(Box::new(d));
        idx += 1;
    }

    let _ = idx;

    if sets.is_empty() {
        return Err(Error::Validation("At least one field to update is required".into()));
    }

    let changed_fields: Vec<&str> = sets.iter()
        .map(|s| s.split(" =").next().unwrap_or(""))
        .collect();

    let sql = format!(
        "UPDATE api.goals SET {} WHERE id = $1 AND organization_id = $2
         RETURNING id, title, status, progress, target_date, updated_at",
        sets.join(", ")
    );

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        sql_params.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

    let row = client.query_opt(&sql, &param_refs).await
        .map_err(|e| Error::Internal(format!("update_goal failed: {}", e)))?;

    let row = match row {
        Some(r) => r,
        None => return Ok(ToolResult::ok(Value::Null, format!("Goal {} not found", goal_id), 0, 0)),
    };

    let result_title: String = row.get(1);

    let goal = json!({
        "id": row.get::<_, Uuid>(0).to_string(),
        "title": &result_title,
        "status": row.get::<_, Option<String>>(2),
        "progress": row.get::<_, Option<i32>>(3),
        "target_date": row.get::<_, Option<chrono::NaiveDate>>(4).map(|d| d.to_string()),
        "updated_at": row.get::<_, Option<chrono::DateTime<chrono::Utc>>>(5)
            .map(|d| d.to_rfc3339()),
    });

    let summary = format!("Updated goal \"{}\" ({})", result_title, changed_fields.join(", "));

    log_audit(pool, org_uuid, "update_goal", params, &summary, true, None).await;
    stamp_source_tool(pool, "goal", &goal_id, "update_goal");

    Ok(ToolResult::ok(goal, summary, 1, 0))
}

// ============================================================================
// add_comment - Add comment to a task
// ============================================================================

pub async fn add_comment(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    ensure_audit_table(pool).await;

    let task_id = parse_uuid(params, "task_id")?;
    let author_id = parse_uuid(params, "author_id")?;
    let content = parse_str_opt(params, "content")
        .ok_or_else(|| Error::Validation("content is required".into()))?;
    let is_private = params.get("is_private").and_then(|v| v.as_bool()).unwrap_or(false);

    let client = pool.get().await?;

    // Validate task exists in org
    let task_exists = client.query_opt(
        "SELECT 1 FROM api.tasks WHERE id = $1 AND organization_id = $2",
        &[&task_id, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
    if task_exists.is_none() {
        return Err(Error::Validation(format!("Task {} not found in organization", task_id)));
    }

    // Validate author exists in org + get name
    let author_row = client.query_opt(
        "SELECT (first_name || ' ' || last_name), is_deleted FROM api.users
         WHERE id = $1 AND organization_id = $2",
        &[&author_id, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
    let author_row = match author_row {
        Some(r) => r,
        None => return Err(Error::Validation(format!("Author {} not found in organization", author_id))),
    };
    let author_deleted: bool = author_row.get(1);
    if author_deleted {
        return Err(Error::Validation(format!("Author {} is deactivated and cannot add comments", author_id)));
    }
    let author_name: String = author_row.get(0);

    let row = client.query_one(
        "INSERT INTO api.task_comments (task_id, author_id, content, is_private, organization_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, created_at",
        &[&task_id, &author_id, &content, &is_private, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("add_comment failed: {}", e)))?;

    let comment_id: Uuid = row.get(0);

    let comment = json!({
        "id": comment_id.to_string(),
        "task_id": task_id.to_string(),
        "author_id": author_id.to_string(),
        "author_name": &author_name,
        "content": content,
        "is_private": is_private,
        "created_at": row.get::<_, chrono::DateTime<chrono::Utc>>(1).to_rfc3339(),
    });

    let preview: String = content.chars().take(50).collect();
    let summary = format!("Comment by {}: \"{}{}\"",
        author_name, preview, if content.len() > 50 { "..." } else { "" });

    log_audit(pool, org_uuid, "add_comment", params, &summary, true, None).await;
    log_task_activity(
        pool, org_uuid, &task_id, "comment_added",
        &json!({"comment_id": comment_id.to_string(), "author": &author_name}),
        Some(&author_id), Some(&author_name),
    ).await;
    stamp_source_tool(pool, "task_comment", &comment_id, "add_comment");

    Ok(ToolResult::ok(comment, summary, 1, 0))
}

// ============================================================================
// assign_task - Shortcut to update assigned_to + activity log
// ============================================================================

pub async fn assign_task(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    ensure_audit_table(pool).await;

    let task_id = parse_uuid(params, "task_id")?;
    let user_id = parse_uuid(params, "user_id")?;

    let client = pool.get().await?;

    // Validate user exists in org + get name
    let user_row = client.query_opt(
        "SELECT (first_name || ' ' || last_name), is_deleted FROM api.users
         WHERE id = $1 AND organization_id = $2",
        &[&user_id, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("FK check failed: {}", e)))?;
    let user_row = match user_row {
        Some(r) => r,
        None => return Err(Error::Validation(format!("User {} not found in organization", user_id))),
    };
    let user_deleted: bool = user_row.get(1);
    if user_deleted {
        return Err(Error::Validation(format!("User {} is deactivated and cannot be assigned tasks", user_id)));
    }
    let user_name: String = user_row.get(0);

    let row = client.query_opt(
        "UPDATE api.tasks SET assigned_to = $1
         WHERE id = $2 AND organization_id = $3
         RETURNING id, title, status",
        &[&user_id, &task_id, org_uuid],
    ).await.map_err(|e| Error::Internal(format!("assign_task failed: {}", e)))?;

    let row = match row {
        Some(r) => r,
        None => return Ok(ToolResult::ok(Value::Null, format!("Task {} not found", task_id), 0, 0)),
    };

    let title: String = row.get(1);

    let result = json!({
        "task_id": row.get::<_, Uuid>(0).to_string(),
        "title": &title,
        "status": row.get::<_, String>(2),
        "assigned_to": user_id.to_string(),
        "assignee_name": &user_name,
    });

    let summary = format!("Assigned \"{}\" to {}", title, user_name);

    log_audit(pool, org_uuid, "assign_task", params, &summary, true, None).await;
    log_task_activity(
        pool, org_uuid, &task_id, "assigned",
        &json!({"assigned_to": user_id.to_string(), "assignee_name": &user_name}),
        None, None,
    ).await;
    stamp_source_tool(pool, "task", &task_id, "assign_task");

    Ok(ToolResult::ok(result, summary, 1, 0))
}
