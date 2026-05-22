use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct ReportEngine;

impl ReportEngine {
    // ---- Template CRUD ----

    pub async fn create_template(
        pool: &PgPool,
        org_id: Option<Uuid>,
        req: &CreateReportTemplateRequest,
    ) -> Result<ReportTemplate, BpeError> {
        // Validate SQL template: must be read-only
        validate_sql(&req.sql_template)?;

        let params = req.parameters.clone().unwrap_or(serde_json::json!([]));
        let columns = req.columns.clone().unwrap_or(serde_json::json!([]));

        let client = pool.get().await?;
        let row = client
            .query_one(
                "INSERT INTO bpe.report_templates
                    (organization_id, name, description, category, sql_template, parameters, columns)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, organization_id, name, description, category, sql_template,
                           parameters, columns, is_active, created_at, updated_at",
                &[&org_id, &req.name, &req.description, &req.category, &req.sql_template, &params, &columns],
            )
            .await?;

        Ok(row_to_template(&row))
    }

    pub async fn list_templates(
        pool: &PgPool,
        org_id: Option<Uuid>,
        category: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<crate::entity::models::PaginatedResponse<ReportTemplate>, BpeError> {
        let client = pool.get().await?;
        let offset = (page - 1) * per_page;

        let count_row = client
            .query_one(
                "SELECT count(*) FROM bpe.report_templates
                 WHERE is_active = true
                   AND (organization_id IS NULL OR organization_id = $1)
                   AND ($2::text IS NULL OR category = $2)",
                &[&org_id, &category],
            )
            .await?;
        let total: i64 = count_row.get(0);

        let rows = client
            .query(
                "SELECT id, organization_id, name, description, category, sql_template,
                        parameters, columns, is_active, created_at, updated_at
                 FROM bpe.report_templates
                 WHERE is_active = true
                   AND (organization_id IS NULL OR organization_id = $1)
                   AND ($2::text IS NULL OR category = $2)
                 ORDER BY category, name
                 LIMIT $3 OFFSET $4",
                &[&org_id, &category, &per_page, &offset],
            )
            .await?;

        let data = rows.iter().map(row_to_template).collect();
        Ok(crate::entity::models::PaginatedResponse { data, page, per_page, total })
    }

    pub async fn get_template(pool: &PgPool, id: Uuid) -> Result<ReportTemplate, BpeError> {
        let client = pool.get().await?;
        let row = client
            .query_opt(
                "SELECT id, organization_id, name, description, category, sql_template,
                        parameters, columns, is_active, created_at, updated_at
                 FROM bpe.report_templates WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| BpeError::NotFound(format!("Report template {id} not found")))?;

        Ok(row_to_template(&row))
    }

    pub async fn update_template(
        pool: &PgPool,
        id: Uuid,
        req: &UpdateReportTemplateRequest,
    ) -> Result<ReportTemplate, BpeError> {
        let existing = Self::get_template(pool, id).await?;

        let name = req.name.as_deref().unwrap_or(&existing.name);
        let description = req.description.as_ref().or(existing.description.as_ref());
        let category = req.category.as_deref().unwrap_or(&existing.category);
        let sql_template = req.sql_template.as_deref().unwrap_or(&existing.sql_template);
        let parameters = req.parameters.as_ref().unwrap_or(&existing.parameters);
        let columns = req.columns.as_ref().unwrap_or(&existing.columns);
        let is_active = req.is_active.unwrap_or(existing.is_active);

        if req.sql_template.is_some() {
            validate_sql(sql_template)?;
        }

        let client = pool.get().await?;
        let row = client
            .query_one(
                "UPDATE bpe.report_templates
                 SET name=$1, description=$2, category=$3, sql_template=$4,
                     parameters=$5, columns=$6, is_active=$7, updated_at=now()
                 WHERE id=$8
                 RETURNING id, organization_id, name, description, category, sql_template,
                           parameters, columns, is_active, created_at, updated_at",
                &[&name, &description, &category, &sql_template, parameters, columns, &is_active, &id],
            )
            .await?;

        Ok(row_to_template(&row))
    }

    pub async fn delete_template(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client
            .execute("DELETE FROM bpe.report_templates WHERE id = $1", &[&id])
            .await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Report template {id} not found")));
        }
        Ok(())
    }

    // ---- Report execution ----

    /// Run a report template with parameters, scoped to the given organization.
    ///
    /// Uses proper parameterized queries: template placeholders like `$org_id`, `$param_name`
    /// are replaced with numbered PostgreSQL parameters (`$1`, `$2`, ...) and values are
    /// passed as typed parameters to prevent SQL injection.
    pub async fn run_report(
        pool: &PgPool,
        template_id: Uuid,
        org_id: Uuid,
        params: Option<&serde_json::Value>,
    ) -> Result<ReportResult, BpeError> {
        let template = Self::get_template(pool, template_id).await?;

        // Build parameterized query: collect placeholder names and their values
        let mut param_values: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
        let mut sql = template.sql_template.clone();

        // Always bind $org_id as the first parameter
        param_values.push(Box::new(org_id));
        sql = sql.replace("$org_id", &format!("${}::uuid", param_values.len()));

        // Bind named parameters from the request as typed query parameters
        if let Some(p) = params {
            if let Some(obj) = p.as_object() {
                for (key, val) in obj {
                    // Validate parameter key: alphanumeric + underscore only
                    if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        return Err(BpeError::BadRequest(format!(
                            "Invalid parameter name: '{}'. Use alphanumeric and underscores only.", key
                        )));
                    }
                    let placeholder = format!("${key}");
                    if !sql.contains(&placeholder) {
                        continue; // Skip params not referenced in the template
                    }
                    match val {
                        serde_json::Value::String(s) => {
                            param_values.push(Box::new(s.clone()));
                            sql = sql.replace(&placeholder, &format!("${}", param_values.len()));
                        }
                        serde_json::Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                param_values.push(Box::new(i));
                            } else if let Some(f) = n.as_f64() {
                                param_values.push(Box::new(f));
                            } else {
                                return Err(BpeError::BadRequest(format!(
                                    "Parameter '{key}' has unsupported number format"
                                )));
                            }
                            sql = sql.replace(&placeholder, &format!("${}", param_values.len()));
                        }
                        serde_json::Value::Bool(b) => {
                            param_values.push(Box::new(*b));
                            sql = sql.replace(&placeholder, &format!("${}", param_values.len()));
                        }
                        serde_json::Value::Null => {
                            sql = sql.replace(&placeholder, "NULL");
                        }
                        _ => {
                            let s = val.to_string();
                            param_values.push(Box::new(s));
                            sql = sql.replace(&placeholder, &format!("${}", param_values.len()));
                        }
                    }
                }
            }
        }

        // Safety: validate the final SQL template (with $N placeholders, not user values)
        validate_sql(&sql)?;

        // Enforce row limit: append LIMIT if not present
        let upper = sql.to_uppercase();
        if !upper.contains("LIMIT") {
            sql.push_str(" LIMIT 10000");
        }

        let mut client = pool.get().await?;

        // Use a transaction to isolate search_path and statement_timeout settings
        let txn = client.transaction().await.map_err(|e| {
            BpeError::Database(format!("Failed to start transaction: {e}"))
        })?;

        // Set search path to bpe and api schemas only
        txn.execute("SET LOCAL search_path TO bpe, api", &[]).await?;

        // Set statement timeout to 10 seconds for report queries
        txn.execute("SET LOCAL statement_timeout = '10s'", &[]).await?;

        // Build parameter references for the query
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            param_values.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        let rows = txn.query(&sql, &param_refs).await.map_err(|e| {
            BpeError::BadRequest(format!("Report query failed: {e}"))
        })?;

        txn.commit().await.map_err(|e| {
            BpeError::Database(format!("Failed to commit transaction: {e}"))
        })?;

        // Convert rows to JSON values
        let json_rows: Vec<serde_json::Value> = rows.iter().map(|row| {
            let mut obj = serde_json::Map::new();
            for (i, col) in row.columns().iter().enumerate() {
                let val = extract_column_value(row, i);
                obj.insert(col.name().to_string(), val);
            }
            serde_json::Value::Object(obj)
        }).collect();

        let row_count = json_rows.len();

        Ok(ReportResult {
            template_id,
            template_name: template.name,
            columns: template.columns,
            rows: json_rows,
            row_count,
            generated_at: chrono::Utc::now(),
        })
    }

    // ---- Built-in reports (no template needed) ----

    /// Dashboard summary for an organization.
    pub async fn dashboard(pool: &PgPool, org_id: Uuid) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;

        let entity_count: i64 = client
            .query_one("SELECT count(*) FROM bpe.entities WHERE organization_id = $1", &[&org_id])
            .await?
            .get(0);

        let workflow_count: i64 = client
            .query_one("SELECT count(*) FROM bpe.workflow_executions WHERE organization_id = $1", &[&org_id])
            .await?
            .get(0);

        let active_workflows: i64 = client
            .query_one(
                "SELECT count(*) FROM bpe.workflow_executions WHERE organization_id = $1 AND status IN ('running', 'paused')",
                &[&org_id],
            )
            .await?
            .get(0);

        let completed_workflows: i64 = client
            .query_one(
                "SELECT count(*) FROM bpe.workflow_executions WHERE organization_id = $1 AND status = 'completed'",
                &[&org_id],
            )
            .await?
            .get(0);

        let pending_approvals: i64 = client
            .query_one(
                "SELECT count(*) FROM bpe.approval_requests WHERE organization_id = $1 AND status = 'pending'",
                &[&org_id],
            )
            .await?
            .get(0);

        let recent_events: i64 = client
            .query_one(
                "SELECT count(*) FROM bpe.audit_events WHERE organization_id = $1 AND created_at > now() - interval '24 hours'",
                &[&org_id],
            )
            .await?
            .get(0);

        let learned_sequences: i64 = client
            .query_one(
                "SELECT count(*) FROM bpe.learned_sequences WHERE organization_id = $1 AND is_active = true",
                &[&org_id],
            )
            .await?
            .get(0);

        let definitions: i64 = client
            .query_one(
                "SELECT count(*) FROM bpe.workflow_definitions WHERE organization_id = $1 AND is_active = true",
                &[&org_id],
            )
            .await?
            .get(0);

        Ok(serde_json::json!({
            "entities": entity_count,
            "workflow_definitions": definitions,
            "workflow_executions": {
                "total": workflow_count,
                "active": active_workflows,
                "completed": completed_workflows,
            },
            "pending_approvals": pending_approvals,
            "audit_events_24h": recent_events,
            "learned_sequences": learned_sequences,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        }))
    }

    /// Workflow performance summary.
    pub async fn workflow_performance(pool: &PgPool, org_id: Uuid) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;

        let rows = client
            .query(
                "SELECT d.name, d.category,
                        count(e.id) as execution_count,
                        count(*) FILTER (WHERE e.status = 'completed') as completed,
                        count(*) FILTER (WHERE e.status = 'failed') as failed,
                        (avg(EXTRACT(EPOCH FROM (e.completed_at - e.started_at))/60)
                            FILTER (WHERE e.completed_at IS NOT NULL AND e.started_at IS NOT NULL))::float8 as avg_duration_minutes
                 FROM bpe.workflow_definitions d
                 LEFT JOIN bpe.workflow_executions e ON e.definition_id = d.id
                 WHERE d.organization_id = $1 AND d.is_active = true
                 GROUP BY d.id, d.name, d.category
                 ORDER BY count(e.id) DESC",
                &[&org_id],
            )
            .await?;

        let data: Vec<serde_json::Value> = rows.iter().map(|r| {
            let completed: i64 = r.get("completed");
            let total: i64 = r.get("execution_count");
            let success_rate = if total > 0 { completed as f64 / total as f64 } else { 0.0 };
            let avg_min: Option<f64> = r.get("avg_duration_minutes");

            serde_json::json!({
                "name": r.get::<_, String>("name"),
                "category": r.get::<_, String>("category"),
                "execution_count": total,
                "completed": completed,
                "failed": r.get::<_, i64>("failed"),
                "success_rate": success_rate,
                "avg_duration_minutes": avg_min,
            })
        }).collect();

        Ok(serde_json::json!({
            "data": data,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        }))
    }
}

// ---- Helpers ----

/// Validate that SQL is read-only — delegates to centralized validation.
fn validate_sql(sql: &str) -> Result<(), BpeError> {
    crate::validation::validate_sql_template(sql)
}

/// Extract a column value from a Row as a JSON value.
fn extract_column_value(row: &tokio_postgres::Row, idx: usize) -> serde_json::Value {
    // Try common types in order
    if let Ok(v) = row.try_get::<_, String>(idx) {
        return serde_json::Value::String(v);
    }
    if let Ok(v) = row.try_get::<_, i64>(idx) {
        return serde_json::json!(v);
    }
    if let Ok(v) = row.try_get::<_, i32>(idx) {
        return serde_json::json!(v);
    }
    if let Ok(v) = row.try_get::<_, f64>(idx) {
        return serde_json::json!(v);
    }
    if let Ok(v) = row.try_get::<_, bool>(idx) {
        return serde_json::json!(v);
    }
    if let Ok(v) = row.try_get::<_, uuid::Uuid>(idx) {
        return serde_json::Value::String(v.to_string());
    }
    if let Ok(v) = row.try_get::<_, chrono::DateTime<chrono::Utc>>(idx) {
        return serde_json::Value::String(v.to_rfc3339());
    }
    if let Ok(v) = row.try_get::<_, serde_json::Value>(idx) {
        return v;
    }
    serde_json::Value::Null
}

fn row_to_template(row: &tokio_postgres::Row) -> ReportTemplate {
    ReportTemplate {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        name: row.get("name"),
        description: row.get("description"),
        category: row.get("category"),
        sql_template: row.get("sql_template"),
        parameters: row.get("parameters"),
        columns: row.get("columns"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
