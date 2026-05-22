use rust_decimal::prelude::FromPrimitive;
use crate::db::PgPool;
use crate::error::BpeError;
use super::audit_trail::TimekeepingAudit;
use super::models::*;
use uuid::Uuid;

pub struct TimeEntryManager;

impl TimeEntryManager {
    pub async fn list(pool: &PgPool, org_id: Uuid, query: &TimeEntryQuery) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        let rows = client.query(
            "SELECT te.*, e.first_name || ' ' || e.last_name AS employee_name,
                    pc.display_name AS pay_code_name, pc.code AS pay_code_code,
                    COUNT(*) OVER() AS total_count
             FROM timekeeping.time_entries te
             JOIN timekeeping.employees e ON e.id = te.employee_id
             JOIN timekeeping.pay_codes pc ON pc.id = te.pay_code_id
             WHERE te.organization_id = $1
               AND ($2::uuid IS NULL OR te.employee_id = $2)
               AND ($3::date IS NULL OR te.work_date >= $3)
               AND ($4::date IS NULL OR te.work_date <= $4)
               AND ($5::text IS NULL OR te.status = $5)
             ORDER BY te.work_date DESC, e.last_name
             LIMIT $6 OFFSET $7",
            &[&org_id, &query.employee_id, &query.start, &query.end,
              &query.status.as_deref(), &per_page, &offset],
        ).await?;

        let total: i64 = rows.first().map(|r| r.get("total_count")).unwrap_or(0);
        let entries: Vec<TimeEntry> = rows.iter().map(row_to_time_entry).collect();

        Ok(serde_json::json!({
            "data": entries,
            "total": total,
            "page": page,
            "per_page": per_page
        }))
    }

    pub async fn create(pool: &PgPool, org_id: Uuid, user_id: Uuid, req: &CreateTimeEntryRequest) -> Result<TimeEntry, BpeError> {
        if req.hours.is_nan() || req.hours.is_infinite() || req.hours <= 0.0 || req.hours > 48.0 {
            return Err(BpeError::BadRequest("Hours must be greater than 0 and at most 48".into()));
        }

        let hours_dec = rust_decimal::Decimal::from_f64(req.hours)
            .ok_or_else(|| BpeError::BadRequest("Invalid hours value".into()))?;

        let client = pool.get().await?;
        let row = client.query_one(
            "INSERT INTO timekeeping.time_entries
                (organization_id, employee_id, pay_code_id, work_date, start_time, hours, notes, entered_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING *, NULL::text AS employee_name, NULL::text AS pay_code_name, NULL::text AS pay_code_code",
            &[&org_id, &req.employee_id, &req.pay_code_id, &req.work_date,
              &req.start_time, &hours_dec, &req.notes, &user_id],
        ).await?;

        let entry = row_to_time_entry(&row);
        let after = serde_json::to_value(&entry).ok();
        TimekeepingAudit::log(
            pool, org_id, Some(user_id), None,
            Some(req.employee_id), None,
            "time_entry.created", "time_entry", Some(entry.id),
            None, after.as_ref(),
            &format!("Created time entry: {} hrs on {} ({})", req.hours, req.work_date, req.pay_code_id),
        ).await;

        Ok(entry)
    }

    pub async fn update(pool: &PgPool, id: Uuid, user_id: Uuid, req: &UpdateTimeEntryRequest) -> Result<TimeEntry, BpeError> {
        if let Some(h) = req.hours {
            if h.is_nan() || h.is_infinite() || h <= 0.0 || h > 48.0 {
                return Err(BpeError::BadRequest("Hours must be greater than 0 and at most 48".into()));
            }
        }

        let client = pool.get().await?;

        // Capture before state
        let before_row = client.query_one(
            "SELECT *, NULL::text AS employee_name, NULL::text AS pay_code_name, NULL::text AS pay_code_code
             FROM timekeeping.time_entries WHERE id = $1",
            &[&id],
        ).await.map_err(|_| BpeError::NotFound(format!("Time entry {id} not found")))?;
        let before_entry = row_to_time_entry(&before_row);
        let before = serde_json::to_value(&before_entry).ok();

        if before_entry.status != "draft" {
            return Err(BpeError::BadRequest(format!("Cannot edit entry in '{}' status. Only draft entries can be modified.", before_entry.status)));
        }

        let hours_dec = req.hours.map(|h| rust_decimal::Decimal::from_f64(h)).flatten();
        let row = client.query_one(
            "UPDATE timekeeping.time_entries SET
                pay_code_id = COALESCE($2, pay_code_id),
                work_date = COALESCE($3, work_date),
                start_time = COALESCE($4, start_time),
                hours = COALESCE($5, hours),
                notes = COALESCE($6, notes),
                updated_at = now()
             WHERE id = $1
             RETURNING *, NULL::text AS employee_name, NULL::text AS pay_code_name, NULL::text AS pay_code_code",
            &[&id, &req.pay_code_id, &req.work_date, &req.start_time, &hours_dec, &req.notes],
        ).await?;

        let entry = row_to_time_entry(&row);
        let after = serde_json::to_value(&entry).ok();

        let mut changes = Vec::new();
        if req.hours.is_some() && before_entry.hours != entry.hours {
            changes.push(format!("hours: {} → {}", before_entry.hours, entry.hours));
        }
        if req.work_date.is_some() && before_entry.work_date != entry.work_date {
            changes.push(format!("date: {} → {}", before_entry.work_date, entry.work_date));
        }
        if req.pay_code_id.is_some() && before_entry.pay_code_id != entry.pay_code_id {
            changes.push(format!("pay_code changed"));
        }
        let summary = if changes.is_empty() { "Updated time entry (no field changes)".to_string() }
            else { format!("Updated time entry: {}", changes.join(", ")) };

        TimekeepingAudit::log(
            pool, entry.organization_id, Some(user_id), None,
            Some(entry.employee_id), None,
            "time_entry.updated", "time_entry", Some(id),
            before.as_ref(), after.as_ref(), &summary,
        ).await;

        Ok(entry)
    }

    pub async fn delete(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;

        // Capture before state for audit
        let before_row = client.query_opt(
            "SELECT *, NULL::text AS employee_name, NULL::text AS pay_code_name, NULL::text AS pay_code_code
             FROM timekeeping.time_entries WHERE id = $1 AND status = 'draft'",
            &[&id],
        ).await?;
        let before_entry = before_row.as_ref().map(row_to_time_entry);
        let before = before_entry.as_ref().and_then(|e| serde_json::to_value(e).ok());

        let n = client.execute(
            "DELETE FROM timekeeping.time_entries WHERE id = $1 AND status = 'draft'",
            &[&id],
        ).await?;
        if n == 0 {
            return Err(BpeError::BadRequest("Entry not found or not in draft status".into()));
        }

        if let Some(entry) = &before_entry {
            TimekeepingAudit::log(
                pool, entry.organization_id, Some(user_id), None,
                Some(entry.employee_id), None,
                "time_entry.deleted", "time_entry", Some(id),
                before.as_ref(), None,
                &format!("Deleted time entry: {} hrs on {}", entry.hours, entry.work_date),
            ).await;
        }
        Ok(())
    }

    pub async fn submit(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;

        // Capture before state
        let before_row = client.query_opt(
            "SELECT *, NULL::text AS employee_name, NULL::text AS pay_code_name, NULL::text AS pay_code_code
             FROM timekeeping.time_entries WHERE id = $1 AND status = 'draft'",
            &[&id],
        ).await?;

        let n = client.execute(
            "UPDATE timekeeping.time_entries SET status = 'submitted', submitted_at = now(), updated_at = now()
             WHERE id = $1 AND status = 'draft'",
            &[&id],
        ).await?;
        if n == 0 {
            return Err(BpeError::BadRequest("Entry not found or not in draft status".into()));
        }

        if let Some(row) = &before_row {
            let entry = row_to_time_entry(row);
            TimekeepingAudit::log(
                pool, entry.organization_id, Some(user_id), None,
                Some(entry.employee_id), None,
                "time_entry.submitted", "time_entry", Some(id),
                None, None,
                &format!("Submitted time entry for approval: {} hrs on {}", entry.hours, entry.work_date),
            ).await;
        }
        Ok(())
    }

    pub async fn batch_create(pool: &PgPool, org_id: Uuid, user_id: Uuid, entries: &[CreateTimeEntryRequest]) -> Result<Vec<TimeEntry>, BpeError> {
        let mut results = Vec::new();
        for req in entries {
            let entry = Self::create(pool, org_id, user_id, req).await?;
            results.push(entry);
        }
        Ok(results)
    }
}

fn row_to_time_entry(row: &tokio_postgres::Row) -> TimeEntry {
    let hours_dec: rust_decimal::Decimal = row.get("hours");
    TimeEntry {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        employee_id: row.get("employee_id"),
        employee_name: row.try_get("employee_name").ok().flatten(),
        pay_code_id: row.get("pay_code_id"),
        pay_code_name: row.try_get("pay_code_name").ok().flatten(),
        pay_code: row.try_get("pay_code_code").ok().flatten(),
        work_date: row.get("work_date"),
        start_time: row.try_get("start_time").ok().flatten(),
        hours: hours_dec.to_string().parse::<f64>().unwrap_or(0.0),
        notes: row.try_get("notes").ok().flatten(),
        entered_by: row.get("entered_by"),
        status: row.get("status"),
        submitted_at: row.try_get("submitted_at").ok().flatten(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
