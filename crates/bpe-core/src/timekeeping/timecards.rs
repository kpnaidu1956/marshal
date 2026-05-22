use chrono::Datelike;
use crate::db::PgPool;
use crate::error::BpeError;
use super::audit_trail::TimekeepingAudit;
use super::models::*;
use uuid::Uuid;

pub struct TimecardManager;

impl TimecardManager {
    pub async fn list_periods(pool: &PgPool, org_id: Uuid) -> Result<Vec<TimecardPeriod>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT * FROM timekeeping.timecard_periods WHERE organization_id = $1 ORDER BY period_start DESC",
            &[&org_id],
        ).await?;
        Ok(rows.iter().map(row_to_period).collect())
    }

    pub async fn create_period(pool: &PgPool, org_id: Uuid, req: &CreatePeriodRequest) -> Result<TimecardPeriod, BpeError> {
        let client = pool.get().await?;
        let row = client.query_one(
            "INSERT INTO timekeeping.timecard_periods
                (organization_id, period_start, period_end, flsa_cycle_start)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (organization_id, period_start) DO UPDATE SET
                period_end = EXCLUDED.period_end,
                flsa_cycle_start = COALESCE(EXCLUDED.flsa_cycle_start, timekeeping.timecard_periods.flsa_cycle_start),
                updated_at = now()
             RETURNING *",
            &[&org_id, &req.period_start, &req.period_end, &req.flsa_cycle_start],
        ).await?;
        Ok(row_to_period(&row))
    }

    pub async fn close_period(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "UPDATE timekeeping.timecard_periods SET status = 'closed', updated_at = now() WHERE id = $1",
            &[&id],
        ).await?;
        Ok(())
    }

    pub async fn certify(pool: &PgPool, org_id: Uuid, user_id: Uuid, req: &CertifyTimecardRequest) -> Result<(), BpeError> {
        let client = pool.get().await?;

        // If no period_id provided, auto-create the current semi-monthly period
        let period_id = match req.period_id {
            Some(pid) => pid,
            None => {
                let today = chrono::Utc::now().naive_utc().date();
                let (start, end) = if today.day() <= 15 {
                    let start = chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
                    let end = chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 15).unwrap();
                    (start, end)
                } else {
                    let start = chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 16).unwrap();
                    let last_day = chrono::NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1)
                        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(today.year() + 1, 1, 1).unwrap())
                        .pred_opt().unwrap();
                    (start, last_day)
                };

                // Find or create the period
                let existing = client.query_opt(
                    "SELECT id FROM timekeeping.timecard_periods WHERE organization_id = $1 AND period_start = $2",
                    &[&org_id, &start],
                ).await?;

                match existing {
                    Some(row) => row.get("id"),
                    None => {
                        let row = client.query_one(
                            "INSERT INTO timekeeping.timecard_periods (organization_id, period_start, period_end)
                             VALUES ($1, $2, $3)
                             ON CONFLICT (organization_id, period_start) DO UPDATE SET period_end = EXCLUDED.period_end
                             RETURNING id",
                            &[&org_id, &start, &end],
                        ).await?;
                        row.get("id")
                    }
                }
            }
        };

        client.execute(
            "INSERT INTO timekeeping.timecard_certifications
                (organization_id, employee_id, period_id, signature_text)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (employee_id, period_id) DO UPDATE SET
                certified_at = now(), signature_text = EXCLUDED.signature_text",
            &[&org_id, &req.employee_id, &period_id,
              &req.signature_text.as_deref().unwrap_or("I certify these hours are accurate")],
        ).await?;

        TimekeepingAudit::log(
            pool, org_id, Some(user_id), None,
            Some(req.employee_id), None,
            "timecard.certified", "timecard_certification", Some(period_id),
            None, None,
            &format!("Employee certified timecard for period {}", period_id),
        ).await;

        Ok(())
    }

    pub async fn list_pending_approvals(pool: &PgPool, org_id: Uuid) -> Result<Vec<serde_json::Value>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT DISTINCT ON (tc.employee_id, tc.period_id)
                    tc.employee_id, e.first_name || ' ' || e.last_name AS employee_name,
                    tc.period_id, p.period_start, p.period_end,
                    tc.certified_at,
                    (SELECT COALESCE(SUM(te.hours::float8), 0) FROM timekeeping.time_entries te
                     WHERE te.employee_id = tc.employee_id
                       AND te.work_date BETWEEN p.period_start AND p.period_end
                       AND te.status IN ('submitted', 'approved')) AS total_hours
             FROM timekeeping.timecard_certifications tc
             JOIN timekeeping.employees e ON e.id = tc.employee_id
             JOIN timekeeping.timecard_periods p ON p.id = tc.period_id
             LEFT JOIN timekeeping.timecard_approvals ta
                ON ta.employee_id = tc.employee_id AND ta.period_id = tc.period_id
             WHERE tc.organization_id = $1 AND ta.id IS NULL
             ORDER BY tc.employee_id, tc.period_id, tc.certified_at DESC",
            &[&org_id],
        ).await?;

        Ok(rows.iter().map(|r| {
            serde_json::json!({
                "employee_id": r.get::<_, Uuid>("employee_id"),
                "employee_name": r.get::<_, String>("employee_name"),
                "period_id": r.get::<_, Uuid>("period_id"),
                "period_start": r.get::<_, chrono::NaiveDate>("period_start").to_string(),
                "period_end": r.get::<_, chrono::NaiveDate>("period_end").to_string(),
                "certified_at": r.get::<_, chrono::DateTime<chrono::Utc>>("certified_at"),
                "total_hours": r.get::<_, f64>("total_hours"),
            })
        }).collect())
    }

    pub async fn decide(pool: &PgPool, org_id: Uuid, supervisor_id: Uuid, req: &TimecardDecisionRequest) -> Result<(), BpeError> {
        if !matches!(req.decision.as_str(), "approved" | "rejected") {
            return Err(BpeError::BadRequest("Decision must be 'approved' or 'rejected'".into()));
        }

        let client = pool.get().await?;
        client.execute(
            "INSERT INTO timekeeping.timecard_approvals
                (organization_id, employee_id, period_id, supervisor_id, decision, notes)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (employee_id, period_id, supervisor_id) DO UPDATE SET
                decision = EXCLUDED.decision, notes = EXCLUDED.notes, decided_at = now()",
            &[&org_id, &req.employee_id, &req.period_id, &supervisor_id, &req.decision, &req.notes],
        ).await?;

        TimekeepingAudit::log(
            pool, org_id, Some(supervisor_id), None,
            Some(req.employee_id), None,
            &format!("timecard.{}", req.decision), "timecard_approval", Some(req.period_id),
            None, Some(&serde_json::json!({ "decision": req.decision, "notes": req.notes })),
            &format!("Supervisor {} timecard for period {}", req.decision, req.period_id),
        ).await;

        // If approved, update time entries status
        if req.decision == "approved" {
            let period = client.query_one(
                "SELECT period_start, period_end FROM timekeeping.timecard_periods WHERE id = $1",
                &[&req.period_id],
            ).await?;
            let start: chrono::NaiveDate = period.get("period_start");
            let end: chrono::NaiveDate = period.get("period_end");

            client.execute(
                "UPDATE timekeeping.time_entries SET status = 'approved', updated_at = now()
                 WHERE employee_id = $1 AND work_date BETWEEN $2 AND $3 AND status = 'submitted'",
                &[&req.employee_id, &start, &end],
            ).await?;
        }

        Ok(())
    }
}

fn row_to_period(row: &tokio_postgres::Row) -> TimecardPeriod {
    TimecardPeriod {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        period_start: row.get("period_start"),
        period_end: row.get("period_end"),
        flsa_cycle_start: row.try_get("flsa_cycle_start").ok().flatten(),
        status: row.get("status"),
    }
}
