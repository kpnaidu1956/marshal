//! Report tools for LLM agents
//!
//! Aggregate reports: sprint metrics, workload distribution, goal progress, activity feed.

use chrono::NaiveDate;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::postgres::PgPool;
use super::{ToolResult, parse_str_opt, parse_limit};

/// Parse optional YYYY-MM-DD date from params, returning None if absent or invalid.
fn parse_date_opt(params: &Value, field: &str) -> Result<Option<NaiveDate>> {
    match params.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        Some(s) => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| Error::Validation(format!("Invalid date for {}: {} (expected YYYY-MM-DD)", field, s)))?;
            Ok(Some(d))
        }
        None => Ok(None),
    }
}

// ============================================================================
// sprint_report — task metrics for a time period
// ============================================================================

pub async fn sprint_report(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let today = chrono::Utc::now().date_naive();
    let since = parse_date_opt(params, "since")?.unwrap_or(today - chrono::Duration::days(14));
    let until = parse_date_opt(params, "until")?.unwrap_or(today);

    // Convert dates to timestamps for >= / < comparisons
    let since_ts = since.and_hms_opt(0, 0, 0).unwrap();
    let until_ts = (until + chrono::Duration::days(1)).and_hms_opt(0, 0, 0).unwrap();

    let client = pool.get().await?;

    // Aggregate metrics in a single query via CTE
    let row = client
        .query_one(
            "WITH stats AS (
                SELECT
                    COUNT(*) AS total_tasks,
                    COUNT(*) FILTER (WHERE status IN ('Completed','done')
                        AND updated_at >= $2::timestamp AND updated_at < $3::timestamp) AS completed_in_period,
                    COUNT(*) FILTER (WHERE created_at >= $2::timestamp AND created_at < $3::timestamp) AS created_in_period,
                    COUNT(*) FILTER (WHERE due_date < CURRENT_DATE
                        AND status NOT IN ('Completed','done')) AS overdue,
                    COUNT(*) FILTER (WHERE status = 'Blocked') AS blocked
                FROM api.tasks
                WHERE organization_id = $1 AND is_deleted = false
            ),
            breakdown AS (
                SELECT COALESCE(jsonb_object_agg(status, cnt), '{}'::jsonb) AS by_status
                FROM (SELECT status, COUNT(*) AS cnt
                      FROM api.tasks
                      WHERE organization_id = $1 AND is_deleted = false
                      GROUP BY status) t
            )
            SELECT s.total_tasks, s.completed_in_period, s.created_in_period,
                   s.overdue, s.blocked, b.by_status
            FROM stats s, breakdown b",
            &[org_uuid, &since_ts, &until_ts],
        )
        .await
        .map_err(|e| Error::Internal(format!("sprint_report query failed: {}", e)))?;

    let total: i64 = row.get(0);
    let completed: i64 = row.get(1);
    let created: i64 = row.get(2);
    let overdue: i64 = row.get(3);
    let blocked: i64 = row.get(4);
    let by_status: Value = row.get::<_, Value>(5);

    // Top blockers: overdue tasks with assignees
    let blockers = client
        .query(
            "SELECT t.id, t.title, t.due_date,
                    (u.first_name || ' ' || u.last_name) AS assignee_name
             FROM api.tasks t
             LEFT JOIN api.users u ON u.id = t.assigned_to
             WHERE t.organization_id = $1 AND t.is_deleted = false
               AND t.due_date < CURRENT_DATE
               AND t.status NOT IN ('Completed','done')
             ORDER BY t.due_date ASC
             LIMIT 10",
            &[org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("sprint_report blockers query failed: {}", e)))?;

    let blockers_json: Vec<Value> = blockers.iter().map(|r| {
        json!({
            "id": r.get::<_, Uuid>(0).to_string(),
            "title": r.get::<_, String>(1),
            "due_date": r.get::<_, Option<NaiveDate>>(2).map(|d| d.to_string()),
            "assignee_name": r.get::<_, Option<String>>(3),
        })
    }).collect();

    let data = json!({
        "period": { "since": since.to_string(), "until": until.to_string() },
        "total_tasks": total,
        "completed_in_period": completed,
        "created_in_period": created,
        "overdue": overdue,
        "blocked": blocked,
        "velocity": completed,
        "net_change": created - completed,
        "status_breakdown": by_status,
        "top_blockers": blockers_json,
    });

    let summary = format!(
        "Sprint report ({} to {}): {} total tasks, {} completed, {} created, {} overdue, {} blocked. Velocity: {} tasks/period.",
        since, until, total, completed, created, overdue, blocked, completed
    );

    Ok(ToolResult::ok(data, summary, 1, 0))
}

// ============================================================================
// workload_report — per-user task distribution
// ============================================================================

pub async fn workload_report(pool: &Arc<PgPool>, org_uuid: &Uuid, _params: &Value) -> Result<ToolResult> {
    let client = pool.get().await?;

    let rows = client
        .query(
            "SELECT u.id,
                    (u.first_name || ' ' || u.last_name) AS name,
                    COUNT(t.id) AS total_tasks,
                    COUNT(t.id) FILTER (WHERE t.status IN ('Completed','done')) AS completed,
                    COUNT(t.id) FILTER (WHERE t.status = 'In Progress') AS in_progress,
                    COUNT(t.id) FILTER (WHERE t.status = 'Assigned') AS assigned,
                    COUNT(t.id) FILTER (WHERE t.due_date < CURRENT_DATE
                        AND t.status NOT IN ('Completed','done')) AS overdue
             FROM api.users u
             LEFT JOIN api.tasks t ON t.assigned_to = u.id AND t.is_deleted = false AND t.organization_id = $1
             WHERE u.organization_id = $1 AND u.is_deleted = false
             GROUP BY u.id, u.first_name, u.last_name
             ORDER BY COUNT(t.id) DESC",
            &[org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("workload_report query failed: {}", e)))?;

    let mut total_across_all: i64 = 0;
    let mut max_load: i64 = 0;
    let mut min_load: i64 = i64::MAX;

    let users: Vec<Value> = rows.iter().map(|r| {
        let total: i64 = r.get(2);
        let completed: i64 = r.get(3);
        let in_progress: i64 = r.get(4);
        let assigned: i64 = r.get(5);
        let overdue: i64 = r.get(6);
        let active = total - completed;

        total_across_all += total;
        if active > max_load { max_load = active; }
        if active < min_load { min_load = active; }

        json!({
            "user_id": r.get::<_, Uuid>(0).to_string(),
            "name": r.get::<_, String>(1),
            "total_tasks": total,
            "completed": completed,
            "in_progress": in_progress,
            "assigned": assigned,
            "overdue": overdue,
            "active_tasks": active,
        })
    }).collect();

    if min_load == i64::MAX { min_load = 0; }
    let user_count = users.len();
    let avg_load = if user_count > 0 { total_across_all as f64 / user_count as f64 } else { 0.0 };

    let data = json!({
        "users": users,
        "summary_stats": {
            "total_users": user_count,
            "total_tasks": total_across_all,
            "avg_tasks_per_user": (avg_load * 10.0).round() / 10.0,
            "max_active_load": max_load,
            "min_active_load": min_load,
        },
    });

    let summary = format!(
        "Workload report: {} users, {} total tasks, avg {:.1} tasks/user. Max active: {}, Min active: {}.",
        user_count, total_across_all, avg_load, max_load, min_load
    );

    Ok(ToolResult::ok(data, summary, user_count, 0))
}

// ============================================================================
// goal_progress_report — goal hierarchy with completion and at-risk flags
// ============================================================================

pub async fn goal_progress_report(pool: &Arc<PgPool>, org_uuid: &Uuid, _params: &Value) -> Result<ToolResult> {
    let client = pool.get().await?;

    let rows = client
        .query(
            "WITH RECURSIVE goal_tree AS (
                SELECT g.id, g.title, g.status, g.parent_goal_id, g.target_date, 0 AS depth
                FROM api.goals g
                WHERE g.parent_goal_id IS NULL AND g.organization_id = $1
              UNION ALL
                SELECT g.id, g.title, g.status, g.parent_goal_id, g.target_date, gt.depth + 1
                FROM api.goals g
                JOIN goal_tree gt ON g.parent_goal_id = gt.id
                WHERE g.organization_id = $1 AND gt.depth < 20
            )
            SELECT gt.id, gt.title, gt.status, gt.parent_goal_id, gt.target_date, gt.depth,
                   COUNT(t.id) AS task_count,
                   COUNT(t.id) FILTER (WHERE t.status IN ('done','completed','Completed')) AS tasks_done
            FROM goal_tree gt
            LEFT JOIN api.tasks t ON t.goal_id = gt.id AND t.is_deleted = false
            GROUP BY gt.id, gt.title, gt.status, gt.parent_goal_id, gt.target_date, gt.depth
            ORDER BY gt.depth, gt.title",
            &[org_uuid],
        )
        .await
        .map_err(|e| Error::Internal(format!("goal_progress_report query failed: {}", e)))?;

    let today = chrono::Utc::now().date_naive();
    let mut total_tasks: i64 = 0;
    let mut total_done: i64 = 0;
    let mut at_risk_count = 0usize;

    let goals: Vec<Value> = rows.iter().map(|r| {
        let task_count: i64 = r.get(6);
        let tasks_done: i64 = r.get(7);
        let target_date: Option<NaiveDate> = r.get(4);
        let progress_pct = if task_count > 0 {
            (tasks_done as f64 / task_count as f64 * 100.0).round() as i64
        } else {
            0
        };

        // At risk: has target_date within 14 days AND less than 75% complete
        let at_risk = target_date
            .map(|td| {
                let days_remaining = (td - today).num_days();
                days_remaining <= 14 && days_remaining >= 0 && progress_pct < 75
            })
            .unwrap_or(false);

        total_tasks += task_count;
        total_done += tasks_done;
        if at_risk { at_risk_count += 1; }

        json!({
            "id": r.get::<_, Uuid>(0).to_string(),
            "title": r.get::<_, String>(1),
            "status": r.get::<_, Option<String>>(2),
            "parent_goal_id": r.get::<_, Option<Uuid>>(3).map(|u| u.to_string()),
            "target_date": target_date.map(|d| d.to_string()),
            "depth": r.get::<_, i32>(5),
            "task_count": task_count,
            "tasks_done": tasks_done,
            "progress_pct": progress_pct,
            "at_risk": at_risk,
        })
    }).collect();

    let goal_count = goals.len();
    let overall_pct = if total_tasks > 0 {
        (total_done as f64 / total_tasks as f64 * 100.0).round() as i64
    } else {
        0
    };

    let data = json!({
        "goals": goals,
        "summary_stats": {
            "total_goals": goal_count,
            "total_tasks": total_tasks,
            "total_tasks_done": total_done,
            "overall_progress_pct": overall_pct,
            "at_risk_goals": at_risk_count,
        },
    });

    let summary = format!(
        "Goal progress: {} goals, {}% overall ({}/{} tasks done). {} goals at risk.",
        goal_count, overall_pct, total_done, total_tasks, at_risk_count
    );

    Ok(ToolResult::ok(data, summary, goal_count, 0))
}

// ============================================================================
// activity_summary — recent org-wide activity feed
// ============================================================================

pub async fn activity_summary(pool: &Arc<PgPool>, org_uuid: &Uuid, params: &Value) -> Result<ToolResult> {
    let today = chrono::Utc::now().date_naive();
    let since = parse_date_opt(params, "since")?.unwrap_or(today - chrono::Duration::days(7));
    let since_ts = since.and_hms_opt(0, 0, 0).unwrap();
    let limit = parse_limit(params, 50, 200);

    // Optionally filter by event type
    let event_type_filter = parse_str_opt(params, "event_type");

    let client = pool.get().await?;

    // Comments (always available)
    let comments = if event_type_filter.is_none() || event_type_filter == Some("comment") {
        client
            .query(
                "SELECT tc.id, tc.content, tc.author_id,
                        (u.first_name || ' ' || u.last_name) AS actor_name,
                        tc.task_id, t.title AS task_title,
                        tc.created_at
                 FROM api.task_comments tc
                 JOIN api.tasks t ON t.id = tc.task_id AND t.organization_id = $1
                 LEFT JOIN api.users u ON u.id = tc.author_id
                 WHERE tc.created_at >= $2::timestamp
                 ORDER BY tc.created_at DESC
                 LIMIT $3",
                &[org_uuid, &since_ts, &limit],
            )
            .await
            .map_err(|e| Error::Internal(format!("activity_summary comments query failed: {}", e)))?
    } else {
        Vec::new()
    };

    // Activity logs (table may not exist)
    let activity_logs = if event_type_filter.is_none() || event_type_filter == Some("activity") {
        match client
            .query(
                "SELECT al.id, al.action, COALESCE(al.changes::text, '') AS detail,
                        al.changed_by, COALESCE(al.changed_by_name, '') AS actor_name,
                        al.task_id, t.title AS task_title,
                        al.created_at
                 FROM api.task_activity_logs al
                 JOIN api.tasks t ON t.id = al.task_id AND t.organization_id = $1
                 WHERE al.created_at >= $2::timestamp
                 ORDER BY al.created_at DESC
                 LIMIT $3",
                &[org_uuid, &since_ts, &limit],
            )
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("activity_summary activity_logs query failed (table may not exist): {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Merge into a single sorted timeline
    let mut events: Vec<(String, Value)> = Vec::with_capacity(comments.len() + activity_logs.len());

    for c in &comments {
        let ts = c.get::<_, chrono::DateTime<chrono::Utc>>(6).to_rfc3339();
        events.push((ts.clone(), json!({
            "id": c.get::<_, Uuid>(0).to_string(),
            "event_type": "comment",
            "content": c.get::<_, Option<String>>(1),
            "actor_id": c.get::<_, Option<Uuid>>(2).map(|u| u.to_string()),
            "actor_name": c.get::<_, Option<String>>(3),
            "task_id": c.get::<_, Uuid>(4).to_string(),
            "task_title": c.get::<_, String>(5),
            "timestamp": &ts,
        })));
    }

    for a in &activity_logs {
        let ts = a.get::<_, chrono::DateTime<chrono::Utc>>(7).to_rfc3339();
        events.push((ts.clone(), json!({
            "id": a.get::<_, Uuid>(0).to_string(),
            "event_type": a.get::<_, String>(1),
            "detail": a.get::<_, String>(2),
            "actor_id": a.get::<_, Option<Uuid>>(3).map(|u| u.to_string()),
            "actor_name": a.get::<_, String>(4),
            "task_id": a.get::<_, Uuid>(5).to_string(),
            "task_title": a.get::<_, String>(6),
            "timestamp": &ts,
        })));
    }

    // Sort descending (most recent first)
    events.sort_by(|a, b| b.0.cmp(&a.0));

    // Apply limit after merge
    events.truncate(limit as usize);

    let sorted: Vec<Value> = events.into_iter().map(|(_, v)| v).collect();
    let count = sorted.len();

    let summary = format!(
        "Activity since {}: {} events ({} comments, {} activity log entries).",
        since, count, comments.len(), activity_logs.len()
    );

    let data = json!({
        "since": since.to_string(),
        "events": sorted,
        "counts": {
            "comments": comments.len(),
            "activity_logs": activity_logs.len(),
            "total": count,
        },
    });

    Ok(ToolResult::ok(data, summary, count, 0))
}
