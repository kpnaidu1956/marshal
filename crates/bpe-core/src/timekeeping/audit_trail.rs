use crate::db::PgPool;
use crate::error::BpeError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub organization_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_name: Option<String>,
    pub employee_id: Option<Uuid>,
    pub employee_name: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub summary: String,
    pub ip_address: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub organization_id: String,
    #[serde(default)]
    pub employee_id: Option<Uuid>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub resource_id: Option<Uuid>,
    #[serde(default)]
    pub start: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub end: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
}

pub struct TimekeepingAudit;

impl TimekeepingAudit {
    /// Log an audit trail entry. Non-blocking — errors are logged but don't propagate.
    pub async fn log(
        pool: &PgPool,
        org_id: Uuid,
        actor_user_id: Option<Uuid>,
        actor_name: Option<&str>,
        employee_id: Option<Uuid>,
        employee_name: Option<&str>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        before_state: Option<&serde_json::Value>,
        after_state: Option<&serde_json::Value>,
        summary: &str,
    ) {
        let result = Self::log_inner(
            pool, org_id, actor_user_id, actor_name, employee_id, employee_name,
            action, resource_type, resource_id, before_state, after_state, summary,
        ).await;

        if let Err(e) = result {
            tracing::warn!("Timekeeping audit log failed: {e}");
        }
    }

    async fn log_inner(
        pool: &PgPool,
        org_id: Uuid,
        actor_user_id: Option<Uuid>,
        actor_name: Option<&str>,
        employee_id: Option<Uuid>,
        employee_name: Option<&str>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        before_state: Option<&serde_json::Value>,
        after_state: Option<&serde_json::Value>,
        summary: &str,
    ) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "INSERT INTO timekeeping.audit_trail
                (organization_id, actor_user_id, actor_name, employee_id, employee_name,
                 action, resource_type, resource_id, before_state, after_state, summary)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            &[
                &org_id, &actor_user_id, &actor_name, &employee_id, &employee_name,
                &action, &resource_type, &resource_id, &before_state, &after_state, &summary,
            ],
        ).await?;
        Ok(())
    }

    /// Query audit trail with filters and pagination.
    pub async fn query(pool: &PgPool, org_id: Uuid, params: &AuditQueryParams) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;
        let page = params.page.unwrap_or(1).max(1);
        let per_page = params.per_page.unwrap_or(50).min(500);
        let offset = (page - 1) * per_page;

        let rows = client.query(
            "SELECT *, COUNT(*) OVER() AS total_count
             FROM timekeeping.audit_trail
             WHERE organization_id = $1
               AND ($2::uuid IS NULL OR employee_id = $2)
               AND ($3::text IS NULL OR action = $3)
               AND ($4::text IS NULL OR resource_type = $4)
               AND ($5::uuid IS NULL OR resource_id = $5)
               AND ($6::date IS NULL OR created_at >= $6::date::timestamptz)
               AND ($7::date IS NULL OR created_at < ($7::date + 1)::timestamptz)
             ORDER BY created_at DESC
             LIMIT $8 OFFSET $9",
            &[
                &org_id, &params.employee_id, &params.action.as_deref(),
                &params.resource_type.as_deref(), &params.resource_id,
                &params.start, &params.end,
                &per_page, &offset,
            ],
        ).await?;

        let total: i64 = rows.first().map(|r| r.get("total_count")).unwrap_or(0);
        let entries: Vec<AuditEntry> = rows.iter().map(row_to_audit_entry).collect();

        Ok(serde_json::json!({
            "data": entries,
            "total": total,
            "page": page,
            "per_page": per_page
        }))
    }

    /// Summary stats for audit trail (for dashboard).
    pub async fn summary(pool: &PgPool, org_id: Uuid, start: Option<chrono::NaiveDate>, end: Option<chrono::NaiveDate>) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT action, resource_type, COUNT(*) AS count
             FROM timekeeping.audit_trail
             WHERE organization_id = $1
               AND ($2::date IS NULL OR created_at >= $2::date::timestamptz)
               AND ($3::date IS NULL OR created_at < ($3::date + 1)::timestamptz)
             GROUP BY action, resource_type
             ORDER BY count DESC",
            &[&org_id, &start, &end],
        ).await?;

        let breakdown: Vec<serde_json::Value> = rows.iter().map(|r| {
            serde_json::json!({
                "action": r.get::<_, String>("action"),
                "resource_type": r.get::<_, String>("resource_type"),
                "count": r.get::<_, i64>("count"),
            })
        }).collect();

        let total: i64 = breakdown.iter().map(|b| b["count"].as_i64().unwrap_or(0)).sum();

        Ok(serde_json::json!({
            "total_events": total,
            "breakdown": breakdown,
        }))
    }
}

fn row_to_audit_entry(row: &tokio_postgres::Row) -> AuditEntry {
    AuditEntry {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        actor_user_id: row.try_get("actor_user_id").ok().flatten(),
        actor_name: row.try_get("actor_name").ok().flatten(),
        employee_id: row.try_get("employee_id").ok().flatten(),
        employee_name: row.try_get("employee_name").ok().flatten(),
        action: row.get("action"),
        resource_type: row.get("resource_type"),
        resource_id: row.try_get("resource_id").ok().flatten(),
        before_state: row.try_get("before_state").ok().flatten(),
        after_state: row.try_get("after_state").ok().flatten(),
        summary: row.get("summary"),
        ip_address: row.try_get::<_, std::net::IpAddr>("ip_address").ok().map(|ip| ip.to_string()),
        created_at: row.get("created_at"),
    }
}
