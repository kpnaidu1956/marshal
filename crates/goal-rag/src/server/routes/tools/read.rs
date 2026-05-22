//! Read tools for LLM agents
//!
//! All tools enforce organization_id filtering server-side.
//! Queries target the `api.*` schema (same tables PostgREST uses).

use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use super::{ToolResult, parse_uuid, parse_uuid_opt, parse_str_opt, parse_limit, parse_offset};

/// Acquire a pooled client for tool queries.
///
/// Tool queries are pure SQL (no LLM calls) and should complete in < 100ms.
/// All queries are bounded by LIMIT clauses and org_id filters. We don't set
/// statement_timeout here because pooled connections are shared with other
/// handlers (e.g., RAG queries that need 20s+ for LLM generation).
async fn get_client(pool: &Arc<PgPool>) -> Result<deadpool_postgres::Client> {
    pool.get().await
}

// ============================================================================
// get_task - Full task with assignee name, goal title, recent comments
// ============================================================================

pub async fn get_task(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let task_id = parse_uuid(params, "task_id")?;
    let client = get_client(pool).await?;

    // Single query: task + assignee + goal + recent comments via lateral join
    let rows = client
        .query(
            "SELECT t.id, t.title, t.description, t.status, t.priority,
                    t.assigned_to, (u.first_name || ' ' || u.last_name) AS assignee_name,
                    t.goal_id, g.title AS goal_title,
                    t.due_date, t.created_at, t.updated_at,
                    c.id AS comment_id, c.content AS comment_content,
                    c.author_id,
                    CASE WHEN cu.is_deleted THEN (cu.first_name || ' ' || cu.last_name || ' (Inactive)')
                         ELSE (cu.first_name || ' ' || cu.last_name) END AS comment_author_name,
                    c.created_at AS comment_created_at,
                    t.is_deleted, t.needs_reassignment
             FROM api.tasks t
             LEFT JOIN api.users u ON u.id = t.assigned_to
             LEFT JOIN api.goals g ON g.id = t.goal_id
             LEFT JOIN LATERAL (
                 SELECT tc.id, tc.content, tc.author_id, tc.created_at
                 FROM api.task_comments tc
                 WHERE tc.task_id = t.id
                 ORDER BY tc.created_at DESC
                 LIMIT 5
             ) c ON true
             LEFT JOIN api.users cu ON cu.id = c.author_id
             WHERE t.id = $1 AND t.organization_id = $2",
            &[&task_id, org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_task query failed: {}", e)))?;

    if rows.is_empty() {
        return Ok(ToolResult::ok(Value::Null, format!("Task {} not found", task_id), 0, 0));
    }

    // First row has the task data; comments are spread across rows
    let first = &rows[0];
    let title: String = first.get(1);
    let status: String = first.get(3);
    let priority: Option<String> = first.get(4);
    let assignee_name: Option<String> = first.get(6);
    let goal_title: Option<String> = first.get(8);

    let mut comments_json: Vec<Value> = Vec::new();
    for r in &rows {
        if let Some(cid) = r.get::<_, Option<Uuid>>(12) {
            comments_json.push(json!({
                "id": cid.to_string(),
                "content": r.get::<_, Option<String>>(13),
                "author_id": r.get::<_, Option<Uuid>>(14).map(|u| u.to_string()),
                "author_name": r.get::<_, Option<String>>(15),
                "created_at": r.get::<_, Option<chrono::DateTime<chrono::Utc>>>(16).map(|d| d.to_rfc3339()),
            }));
        }
    }

    let task = json!({
        "id": first.get::<_, Uuid>(0).to_string(),
        "title": &title,
        "description": first.get::<_, Option<String>>(2),
        "status": &status,
        "priority": &priority,
        "assigned_to": first.get::<_, Option<Uuid>>(5).map(|u| u.to_string()),
        "assignee_name": &assignee_name,
        "goal_id": first.get::<_, Option<Uuid>>(7).map(|u| u.to_string()),
        "goal_title": &goal_title,
        "due_date": first.get::<_, Option<chrono::NaiveDate>>(9).map(|d| d.to_string()),
        "created_at": first.get::<_, chrono::DateTime<chrono::Utc>>(10).to_rfc3339(),
        "updated_at": first.get::<_, chrono::DateTime<chrono::Utc>>(11).to_rfc3339(),
        "is_deleted": first.get::<_, bool>(17),
        "needs_reassignment": first.get::<_, bool>(18),
        "recent_comments": &comments_json,
    });

    let summary = format!(
        "Task \"{}\": {} [{}]{}{}. {} comments.",
        title,
        status,
        priority.as_deref().unwrap_or("no priority"),
        assignee_name.as_deref().map(|n| format!(", assigned to {}", n)).unwrap_or_default(),
        goal_title.as_deref().map(|g| format!(", goal: {}", g)).unwrap_or_default(),
        comments_json.len(),
    );

    Ok(ToolResult::ok(task, summary, 1, 0))
}

// ============================================================================
// get_goal - Goal with child goals and task summary
// ============================================================================

pub async fn get_goal(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let goal_id = parse_uuid(params, "goal_id")?;
    let client = get_client(pool).await?;

    // Main goal + task counts in one query via lateral aggregation
    let row = client
        .query_opt(
            "SELECT g.id, g.title, g.description, g.status,
                    g.created_by, (u.first_name || ' ' || u.last_name) AS creator_name,
                    g.parent_goal_id, g.target_date,
                    g.created_at, g.updated_at,
                    COALESCE(ts.total, 0)::bigint AS task_total,
                    COALESCE(ts.done, 0)::bigint AS task_done,
                    ts.by_status
             FROM api.goals g
             LEFT JOIN api.users u ON u.id = g.created_by
             LEFT JOIN LATERAL (
                 SELECT COALESCE(SUM(t.cnt), 0) AS total,
                        COALESCE(SUM(t.cnt) FILTER (WHERE t.status IN ('done', 'completed')), 0) AS done,
                        jsonb_object_agg(t.status, t.cnt) AS by_status
                 FROM (SELECT status, COUNT(*) AS cnt FROM api.tasks WHERE goal_id = g.id AND is_deleted = false GROUP BY status) t
             ) ts ON true
             WHERE g.id = $1 AND g.organization_id = $2",
            &[&goal_id, org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_goal query failed: {}", e)))?;

    let row = match row {
        Some(r) => r,
        None => return Ok(ToolResult::ok(Value::Null, format!("Goal {} not found", goal_id), 0, 0)),
    };

    // Child goals (small query, typically < 10 rows)
    let children = client
        .query(
            "SELECT id, title, status FROM api.goals
             WHERE parent_goal_id = $1 AND organization_id = $2
             ORDER BY created_at",
            &[&goal_id, org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_goal children query failed: {}", e)))?;

    let children_json: Vec<Value> = children.iter().map(|c| {
        json!({
            "id": c.get::<_, Uuid>(0).to_string(),
            "title": c.get::<_, String>(1),
            "status": c.get::<_, Option<String>>(2),
        })
    }).collect();

    let total_tasks: i64 = row.get(10);
    let task_done: i64 = row.get(11);
    let task_summary: Value = row.get::<_, Option<Value>>(12).unwrap_or(json!({}));

    let title: String = row.get(1);
    let status: Option<String> = row.get(3);

    let goal = json!({
        "id": row.get::<_, Uuid>(0).to_string(),
        "title": &title,
        "description": row.get::<_, Option<String>>(2),
        "status": &status,
        "created_by": row.get::<_, Option<Uuid>>(4).map(|u| u.to_string()),
        "creator_name": row.get::<_, Option<String>>(5),
        "parent_goal_id": row.get::<_, Option<Uuid>>(6).map(|u| u.to_string()),
        "target_date": row.get::<_, Option<chrono::NaiveDate>>(7).map(|d| d.to_string()),
        "created_at": row.get::<_, Option<chrono::DateTime<chrono::Utc>>>(8).map(|d| d.to_rfc3339()),
        "updated_at": row.get::<_, Option<chrono::DateTime<chrono::Utc>>>(9).map(|d| d.to_rfc3339()),
        "child_goals": &children_json,
        "task_summary": {
            "total": total_tasks,
            "done": task_done,
            "by_status": task_summary,
        },
    });

    let summary = format!(
        "Goal \"{}\": {}. {} child goals, {} tasks ({} done).",
        title,
        status.as_deref().unwrap_or("unknown"),
        children_json.len(),
        total_tasks,
        task_done,
    );

    Ok(ToolResult::ok(goal, summary, 1, 0))
}

// ============================================================================
// get_user - User with task counts and recent activity
// ============================================================================

pub async fn get_user(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let user_id = parse_uuid(params, "user_id")?;
    let client = get_client(pool).await?;

    // User + task counts in single query
    let row = client
        .query_opt(
            "SELECT u.id, (u.first_name || ' ' || u.last_name) AS name, u.email, u.title, u.created_at,
                    COALESCE(ts.total, 0)::bigint AS task_total,
                    ts.by_status,
                    u.is_deleted
             FROM api.users u
             LEFT JOIN LATERAL (
                 SELECT COALESCE(SUM(t.cnt), 0) AS total,
                        jsonb_object_agg(t.status, t.cnt) AS by_status
                 FROM (SELECT status, COUNT(*) AS cnt FROM api.tasks WHERE assigned_to = u.id AND organization_id = $2 AND is_deleted = false GROUP BY status) t
             ) ts ON true
             WHERE u.id = $1 AND u.organization_id = $2",
            &[&user_id, org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_user query failed: {}", e)))?;

    let row = match row {
        Some(r) => r,
        None => return Ok(ToolResult::ok(Value::Null, format!("User {} not found", user_id), 0, 0)),
    };

    // Recent comments (activity proxy) — small, fast subquery
    let recent = client
        .query(
            "SELECT tc.id, tc.task_id, t.title AS task_title, tc.created_at
             FROM api.task_comments tc
             JOIN api.tasks t ON t.id = tc.task_id
             WHERE tc.author_id = $1 AND t.organization_id = $2
             ORDER BY tc.created_at DESC
             LIMIT 5",
            &[&user_id, org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_user recent query failed: {}", e)))?;

    let recent_json: Vec<Value> = recent.iter().map(|r| {
        json!({
            "comment_id": r.get::<_, Uuid>(0).to_string(),
            "task_id": r.get::<_, Uuid>(1).to_string(),
            "task_title": r.get::<_, String>(2),
            "created_at": r.get::<_, chrono::DateTime<chrono::Utc>>(3).to_rfc3339(),
        })
    }).collect();

    let name: String = row.get(1);
    let total: i64 = row.get(5);
    let task_summary: Value = row.get::<_, Option<Value>>(6).unwrap_or(json!({}));

    let user = json!({
        "id": row.get::<_, Uuid>(0).to_string(),
        "name": &name,
        "email": row.get::<_, Option<String>>(2),
        "role": row.get::<_, Option<String>>(3),
        "created_at": row.get::<_, chrono::DateTime<chrono::Utc>>(4).to_rfc3339(),
        "is_deleted": row.get::<_, bool>(7),
        "task_summary": {
            "total": total,
            "by_status": task_summary,
        },
        "recent_activity": &recent_json,
    });

    let summary = format!(
        "User \"{}\": {} tasks total. Last active: {}.",
        name,
        total,
        recent_json.first()
            .and_then(|r| r.get("created_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("no recent activity"),
    );

    Ok(ToolResult::ok(user, summary, 1, 0))
}

// ============================================================================
// list_tasks - Filtered, paginated task list
// ============================================================================

pub async fn list_tasks(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let limit = parse_limit(params, 50, 200);
    let offset = parse_offset(params);
    let status_filter = parse_str_opt(params, "status");
    let priority_filter = parse_str_opt(params, "priority");
    let assigned_to = parse_uuid_opt(params, "assigned_to")?;
    let goal_id = parse_uuid_opt(params, "goal_id")?;
    let overdue = params.get("overdue").and_then(|v| v.as_bool()).unwrap_or(false);

    let client = get_client(pool).await?;

    // Build dynamic query with parameterized filters
    let mut query = String::from(
        "SELECT t.id, t.title, t.status, t.priority,
                t.assigned_to, (u.first_name || ' ' || u.last_name) AS assignee_name,
                t.goal_id, t.due_date, t.created_at, t.updated_at
         FROM api.tasks t
         LEFT JOIN api.users u ON u.id = t.assigned_to
         WHERE t.organization_id = $1 AND t.is_deleted = false"
    );
    let mut param_idx = 2u32;
    let mut param_values: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![
        Box::new(*org_uuid),
    ];

    if let Some(s) = status_filter {
        query.push_str(&format!(" AND t.status = ${}", param_idx));
        param_values.push(Box::new(s.to_string()));
        param_idx += 1;
    }
    if let Some(p) = priority_filter {
        query.push_str(&format!(" AND t.priority = ${}", param_idx));
        param_values.push(Box::new(p.to_string()));
        param_idx += 1;
    }
    if let Some(uid) = assigned_to {
        query.push_str(&format!(" AND t.assigned_to = ${}", param_idx));
        param_values.push(Box::new(uid));
        param_idx += 1;
    }
    if let Some(gid) = goal_id {
        query.push_str(&format!(" AND t.goal_id = ${}", param_idx));
        param_values.push(Box::new(gid));
        param_idx += 1;
    }
    if overdue {
        query.push_str(" AND t.due_date < CURRENT_DATE AND t.status NOT IN ('done', 'completed')");
    }

    query.push_str(&format!(
        " ORDER BY t.updated_at DESC LIMIT ${} OFFSET ${}",
        param_idx, param_idx + 1
    ));
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        param_values.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

    let rows = client
        .query(&query, &param_refs)
        .await
        .map_err(|e| Error::Internal(format!("list_tasks query failed: {}", e)))?;

    let tasks: Vec<Value> = rows.iter().map(|r| {
        json!({
            "id": r.get::<_, Uuid>(0).to_string(),
            "title": r.get::<_, String>(1),
            "status": r.get::<_, Option<String>>(2),
            "priority": r.get::<_, Option<String>>(3),
            "assigned_to": r.get::<_, Option<Uuid>>(4).map(|u| u.to_string()),
            "assignee_name": r.get::<_, Option<String>>(5),
            "goal_id": r.get::<_, Option<Uuid>>(6).map(|u| u.to_string()),
            "due_date": r.get::<_, Option<chrono::NaiveDate>>(7).map(|d| d.to_string()),
            "created_at": r.get::<_, chrono::DateTime<chrono::Utc>>(8).to_rfc3339(),
            "updated_at": r.get::<_, chrono::DateTime<chrono::Utc>>(9).to_rfc3339(),
        })
    }).collect();

    let count = tasks.len();
    // Use BTreeMap for deterministic summary ordering
    let mut status_counts = std::collections::BTreeMap::<String, usize>::new();
    for t in &tasks {
        if let Some(s) = t.get("status").and_then(|v| v.as_str()) {
            *status_counts.entry(s.to_string()).or_insert(0) += 1;
        }
    }
    let status_parts: Vec<String> = status_counts.iter()
        .map(|(k, v)| format!("{} {}", v, k))
        .collect();

    let summary = format!(
        "Found {} tasks{}. {}",
        count,
        if overdue { " (overdue)" } else { "" },
        if status_parts.is_empty() { "No results.".to_string() } else { status_parts.join(", ") },
    );

    Ok(ToolResult::ok(json!({ "tasks": tasks }), summary, count, 0))
}

// ============================================================================
// list_goals - Filtered goal list with task counts (LATERAL, no row inflation)
// ============================================================================

pub async fn list_goals(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let limit = parse_limit(params, 50, 200);
    let offset = parse_offset(params);
    let status_filter = parse_str_opt(params, "status");
    let parent_goal_id = parse_uuid_opt(params, "parent_goal_id")?;

    let client = get_client(pool).await?;

    // LATERAL subquery for task counts — avoids JOIN row inflation
    let mut query = String::from(
        "SELECT g.id, g.title, g.description, g.status,
                g.parent_goal_id, g.target_date,
                g.created_at, g.updated_at,
                COALESCE(ts.task_count, 0) AS task_count,
                COALESCE(ts.tasks_done, 0) AS tasks_done
         FROM api.goals g
         LEFT JOIN LATERAL (
             SELECT COUNT(*) AS task_count,
                    COUNT(*) FILTER (WHERE status IN ('done', 'completed')) AS tasks_done
             FROM api.tasks
             WHERE goal_id = g.id AND is_deleted = false
         ) ts ON true
         WHERE g.organization_id = $1"
    );
    let mut param_values: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![
        Box::new(*org_uuid),
    ];
    let mut param_idx = 2u32;

    if let Some(s) = status_filter {
        query.push_str(&format!(" AND g.status = ${}", param_idx));
        param_values.push(Box::new(s.to_string()));
        param_idx += 1;
    }
    if let Some(pid) = parent_goal_id {
        query.push_str(&format!(" AND g.parent_goal_id = ${}", param_idx));
        param_values.push(Box::new(pid));
        param_idx += 1;
    }

    query.push_str(&format!(
        " ORDER BY g.created_at DESC LIMIT ${} OFFSET ${}",
        param_idx, param_idx + 1
    ));
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        param_values.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

    let rows = client
        .query(&query, &param_refs)
        .await
        .map_err(|e| Error::Internal(format!("list_goals query failed: {}", e)))?;

    let goals: Vec<Value> = rows.iter().map(|r| {
        let task_count: i64 = r.get(8);
        let tasks_done: i64 = r.get(9);
        let progress = if task_count > 0 {
            (tasks_done as f64 / task_count as f64 * 100.0).round() as i64
        } else {
            0
        };

        json!({
            "id": r.get::<_, Uuid>(0).to_string(),
            "title": r.get::<_, String>(1),
            "description": r.get::<_, Option<String>>(2),
            "status": r.get::<_, Option<String>>(3),
            "parent_goal_id": r.get::<_, Option<Uuid>>(4).map(|u| u.to_string()),
            "target_date": r.get::<_, Option<chrono::NaiveDate>>(5).map(|d| d.to_string()),
            "created_at": r.get::<_, chrono::DateTime<chrono::Utc>>(6).to_rfc3339(),
            "updated_at": r.get::<_, chrono::DateTime<chrono::Utc>>(7).to_rfc3339(),
            "task_count": task_count,
            "tasks_done": tasks_done,
            "progress_pct": progress,
        })
    }).collect();

    let count = goals.len();
    let summary = format!("Found {} goals.", count);

    Ok(ToolResult::ok(json!({ "goals": goals }), summary, count, 0))
}

// ============================================================================
// get_goal_tree - Recursive goal hierarchy (JOIN, not correlated subquery)
// ============================================================================

pub async fn get_goal_tree(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let root_goal_id = parse_uuid_opt(params, "goal_id")?;
    let client = get_client(pool).await?;

    // Recursive CTE + LEFT JOIN aggregate (single query, no correlated subqueries)
    let rows = if let Some(root_id) = root_goal_id {
        client
            .query(
                "WITH RECURSIVE goal_tree AS (
                    SELECT g.id, g.title, g.status, g.parent_goal_id, 0 AS depth
                    FROM api.goals g
                    WHERE g.id = $1 AND g.organization_id = $2
                  UNION ALL
                    SELECT g.id, g.title, g.status, g.parent_goal_id, gt.depth + 1
                    FROM api.goals g
                    JOIN goal_tree gt ON g.parent_goal_id = gt.id
                    WHERE g.organization_id = $2 AND gt.depth < 20
                )
                SELECT gt.id, gt.title, gt.status, gt.parent_goal_id, gt.depth,
                       COUNT(t.id) AS task_count,
                       COUNT(t.id) FILTER (WHERE t.status IN ('done', 'completed')) AS tasks_done
                FROM goal_tree gt
                LEFT JOIN api.tasks t ON t.goal_id = gt.id AND t.is_deleted = false
                GROUP BY gt.id, gt.title, gt.status, gt.parent_goal_id, gt.depth
                ORDER BY gt.depth, gt.title",
                &[&root_id, org_uuid],
            )
            .await
            .map_err(|e| Error::Internal(format!("get_goal_tree query failed: {}", e)))?
    } else {
        // Full tree: start from roots (no parent)
        client
            .query(
                "WITH RECURSIVE goal_tree AS (
                    SELECT g.id, g.title, g.status, g.parent_goal_id, 0 AS depth
                    FROM api.goals g
                    WHERE g.parent_goal_id IS NULL AND g.organization_id = $1
                  UNION ALL
                    SELECT g.id, g.title, g.status, g.parent_goal_id, gt.depth + 1
                    FROM api.goals g
                    JOIN goal_tree gt ON g.parent_goal_id = gt.id
                    WHERE g.organization_id = $1 AND gt.depth < 20
                )
                SELECT gt.id, gt.title, gt.status, gt.parent_goal_id, gt.depth,
                       COUNT(t.id) AS task_count,
                       COUNT(t.id) FILTER (WHERE t.status IN ('done', 'completed')) AS tasks_done
                FROM goal_tree gt
                LEFT JOIN api.tasks t ON t.goal_id = gt.id AND t.is_deleted = false
                GROUP BY gt.id, gt.title, gt.status, gt.parent_goal_id, gt.depth
                ORDER BY gt.depth, gt.title",
                &[org_uuid],
            )
            .await
            .map_err(|e| Error::Internal(format!("get_goal_tree query failed: {}", e)))?
    };

    let nodes: Vec<Value> = rows.iter().map(|r| {
        let task_count: i64 = r.get(5);
        let tasks_done: i64 = r.get(6);
        json!({
            "id": r.get::<_, Uuid>(0).to_string(),
            "title": r.get::<_, String>(1),
            "status": r.get::<_, Option<String>>(2),
            "parent_goal_id": r.get::<_, Option<Uuid>>(3).map(|u| u.to_string()),
            "depth": r.get::<_, i32>(4),
            "task_count": task_count,
            "tasks_done": tasks_done,
        })
    }).collect();

    let count = nodes.len();
    let max_depth = nodes.iter()
        .filter_map(|n| n.get("depth").and_then(|d| d.as_i64()))
        .max()
        .unwrap_or(0);

    let summary = format!("Goal tree: {} nodes, {} levels deep.", count, max_depth + 1);

    Ok(ToolResult::ok(json!({ "tree": nodes }), summary, count, 0))
}

// ============================================================================
// get_task_timeline - Chronological activity for a task
// ============================================================================

pub async fn get_task_timeline(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let task_id = parse_uuid(params, "task_id")?;
    let client = get_client(pool).await?;

    // Verify task belongs to org
    let task_exists = client
        .query_opt(
            "SELECT 1 FROM api.tasks WHERE id = $1 AND organization_id = $2",
            &[&task_id, org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_task_timeline verify failed: {}", e)))?;

    if task_exists.is_none() {
        return Ok(ToolResult::ok(Value::Null, format!("Task {} not found", task_id), 0, 0));
    }

    // Comments as timeline events
    let comments = client
        .query(
            "SELECT tc.id, tc.content,
                    tc.author_id,
                    CASE WHEN u.is_deleted THEN (u.first_name || ' ' || u.last_name || ' (Inactive)')
                         ELSE (u.first_name || ' ' || u.last_name) END AS actor_name,
                    tc.created_at
             FROM api.task_comments tc
             LEFT JOIN api.users u ON u.id = tc.author_id
             WHERE tc.task_id = $1
             ORDER BY tc.created_at",
            &[&task_id],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_task_timeline comments query failed: {}", e)))?;

    // Activity logs — table may not exist; log warning instead of silently swallowing
    let activity_logs = match client
        .query(
            "SELECT id, action,
                    COALESCE(changes::text, '') AS detail,
                    changed_by,
                    COALESCE(changed_by_name, '') AS actor_name,
                    created_at
             FROM api.task_activity_logs
             WHERE task_id = $1
             ORDER BY created_at",
            &[&task_id],
        )
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            // Log warning but continue — table may not exist
            tracing::warn!("task_activity_logs query failed (table may not exist): {}", e);
            Vec::new()
        }
    };

    // Build events from comments
    let mut events: Vec<(String, Value)> = Vec::with_capacity(comments.len() + activity_logs.len());

    for c in &comments {
        let ts = c.get::<_, chrono::DateTime<chrono::Utc>>(4).to_rfc3339();
        events.push((ts.clone(), json!({
            "id": c.get::<_, Uuid>(0).to_string(),
            "event_type": "comment",
            "detail": c.get::<_, Option<String>>(1),
            "actor_id": c.get::<_, Option<Uuid>>(2).map(|u| u.to_string()),
            "actor_name": c.get::<_, Option<String>>(3),
            "timestamp": ts,
        })));
    }

    for a in &activity_logs {
        let ts = a.get::<_, chrono::DateTime<chrono::Utc>>(5).to_rfc3339();
        events.push((ts.clone(), json!({
            "id": a.get::<_, Uuid>(0).to_string(),
            "event_type": a.get::<_, String>(1),
            "detail": a.get::<_, String>(2),
            "actor_id": a.get::<_, Option<Uuid>>(3).map(|u| u.to_string()),
            "actor_name": a.get::<_, String>(4),
            "timestamp": ts,
        })));
    }

    // Sort by timestamp (string comparison works for RFC3339)
    events.sort_by(|a, b| a.0.cmp(&b.0));

    let sorted: Vec<Value> = events.into_iter().map(|(_, v)| v).collect();
    let count = sorted.len();

    let summary = format!(
        "Task timeline: {} events ({} comments, {} activity entries).",
        count, comments.len(), activity_logs.len()
    );

    Ok(ToolResult::ok(json!({ "timeline": sorted }), summary, count, 0))
}

// ============================================================================
// get_conversation - Messages in a thread
// ============================================================================

pub async fn get_conversation(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let conversation_id = parse_uuid(params, "conversation_id")?;
    let limit = parse_limit(params, 50, 200);
    let client = get_client(pool).await?;

    let rows = client
        .query(
            "SELECT id, role, content, created_at
             FROM api.chat_messages
             WHERE conversation_id = $1 AND organization_id = $2
             ORDER BY created_at
             LIMIT $3",
            &[&conversation_id, org_uuid, &limit],
        )
        .await
        .map_err(|e| Error::Internal(format!("get_conversation query failed: {}", e)))?;

    let messages: Vec<Value> = rows.iter().map(|r| {
        json!({
            "id": r.get::<_, Uuid>(0).to_string(),
            "role": r.get::<_, String>(1),
            "content": r.get::<_, Option<String>>(2),
            "created_at": r.get::<_, chrono::DateTime<chrono::Utc>>(3).to_rfc3339(),
        })
    }).collect();

    let count = messages.len();
    let summary = format!("Conversation: {} messages.", count);

    Ok(ToolResult::ok(json!({ "messages": messages }), summary, count, 0))
}
