use chrono::NaiveDate;
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct ReportEngine;

impl ReportEngine {
    /// Hours worked by employee for a date range, broken down by pay code.
    pub async fn hours_report(pool: &PgPool, org_id: Uuid, query: &ReportQuery) -> Result<Vec<HoursReport>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT e.id AS employee_id, e.first_name || ' ' || e.last_name AS employee_name, e.rank,
                    pc.code AS pay_code, pc.display_name, pc.category, pc.is_overtime,
                    COALESCE(SUM(te.hours::float8), 0) AS total
             FROM timekeeping.employees e
             LEFT JOIN timekeeping.time_entries te ON te.employee_id = e.id
                AND ($2::date IS NULL OR te.work_date >= $2)
                AND ($3::date IS NULL OR te.work_date <= $3)
                AND te.status IN ('submitted', 'approved')
             LEFT JOIN timekeeping.pay_codes pc ON pc.id = te.pay_code_id
             WHERE e.organization_id = $1 AND e.status = 'active'
               AND ($4::uuid IS NULL OR e.id = $4)
             GROUP BY e.id, e.first_name, e.last_name, e.rank, pc.code, pc.display_name, pc.category, pc.is_overtime
             ORDER BY e.last_name, e.first_name, pc.code",
            &[&org_id, &query.start, &query.end, &query.employee_id],
        ).await?;

        // Group by employee (use indexmap to preserve insertion order)
        let mut map: indexmap::IndexMap<Uuid, HoursReport> = indexmap::IndexMap::new();
        for row in &rows {
            let emp_id: Uuid = row.get("employee_id");
            let entry = map.entry(emp_id).or_insert_with(|| HoursReport {
                employee_id: emp_id,
                employee_name: row.get("employee_name"),
                rank: row.get("rank"),
                total_hours: 0.0,
                regular_hours: 0.0,
                overtime_hours: 0.0,
                leave_hours: 0.0,
                by_pay_code: Vec::new(),
            });

            let hours: f64 = row.get("total");
            if hours == 0.0 { continue; }

            let category: Option<String> = row.try_get("category").ok();
            let is_ot: Option<bool> = row.try_get("is_overtime").ok();
            let code: Option<String> = row.try_get("pay_code").ok();
            let display: Option<String> = row.try_get("display_name").ok();

            entry.total_hours += hours;
            match (category.as_deref(), is_ot) {
                (Some("leave"), _) => entry.leave_hours += hours,
                (_, Some(true)) => entry.overtime_hours += hours,
                _ => entry.regular_hours += hours,
            }

            if let (Some(c), Some(d), Some(cat)) = (code, display, category) {
                entry.by_pay_code.push(PayCodeHours {
                    pay_code: c,
                    display_name: d,
                    hours,
                    category: cat,
                });
            }
        }

        Ok(map.into_values().collect())
    }

    /// Overtime summary for all employees in a date range.
    pub async fn overtime_report(pool: &PgPool, org_id: Uuid, query: &ReportQuery) -> Result<Vec<serde_json::Value>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT e.id, e.first_name || ' ' || e.last_name AS name, e.rank,
                    COALESCE(SUM(te.hours::float8) FILTER (WHERE pc.is_overtime = true), 0) AS ot_hours,
                    COALESCE(SUM(te.hours::float8) FILTER (WHERE pc.is_overtime = false AND pc.category = 'work'), 0) AS reg_hours,
                    COALESCE(SUM(te.hours::float8), 0) AS total_hours
             FROM timekeeping.employees e
             LEFT JOIN timekeeping.time_entries te ON te.employee_id = e.id
                AND ($2::date IS NULL OR te.work_date >= $2)
                AND ($3::date IS NULL OR te.work_date <= $3)
                AND te.status IN ('submitted', 'approved')
             LEFT JOIN timekeeping.pay_codes pc ON pc.id = te.pay_code_id
             WHERE e.organization_id = $1 AND e.status = 'active'
             GROUP BY e.id, e.first_name, e.last_name, e.rank
             HAVING COALESCE(SUM(te.hours::float8) FILTER (WHERE pc.is_overtime = true), 0) > 0
             ORDER BY ot_hours DESC",
            &[&org_id, &query.start, &query.end],
        ).await?;

        Ok(rows.iter().map(|r| serde_json::json!({
            "employee_id": r.get::<_, Uuid>("id"),
            "employee_name": r.get::<_, String>("name"),
            "rank": r.get::<_, String>("rank"),
            "overtime_hours": r.get::<_, f64>("ot_hours"),
            "regular_hours": r.get::<_, f64>("reg_hours"),
            "total_hours": r.get::<_, f64>("total_hours"),
        })).collect())
    }

    /// FLSA 7(k) compliance report for fire dept (28-day cycle, 212-hour threshold).
    pub async fn flsa_report(pool: &PgPool, org_id: Uuid, cycle_start: NaiveDate) -> Result<FlsaReport, BpeError> {
        let cycle_end = cycle_start + chrono::Duration::days(27);
        let threshold = 212.0_f64;

        let client = pool.get().await?;
        let rows = client.query(
            "SELECT e.id, e.first_name || ' ' || e.last_name AS name,
                    COALESCE(SUM(te.hours::float8) FILTER (WHERE pc.counts_toward_flsa = true), 0) AS flsa_hours
             FROM timekeeping.employees e
             LEFT JOIN timekeeping.time_entries te ON te.employee_id = e.id
                AND te.work_date BETWEEN $2 AND $3
                AND te.status IN ('submitted', 'approved')
             LEFT JOIN timekeeping.pay_codes pc ON pc.id = te.pay_code_id
             WHERE e.organization_id = $1 AND e.status = 'active'
             GROUP BY e.id, e.first_name, e.last_name
             ORDER BY flsa_hours DESC",
            &[&org_id, &cycle_start, &cycle_end],
        ).await?;

        let employees: Vec<FlsaEmployeeReport> = rows.iter().map(|r| {
            let flsa_hours: f64 = r.get("flsa_hours");
            let ot = (flsa_hours - threshold).max(0.0);
            FlsaEmployeeReport {
                employee_id: r.get("id"),
                employee_name: r.get("name"),
                flsa_hours,
                threshold,
                overtime_hours: ot,
                is_compliant: ot == 0.0,
            }
        }).collect();

        Ok(FlsaReport {
            cycle_start,
            cycle_end,
            threshold_hours: threshold,
            employees,
        })
    }

    /// Staffing report for a specific date.
    pub async fn staffing_report(pool: &PgPool, org_id: Uuid, date: NaiveDate) -> Result<StaffingReport, BpeError> {
        let client = pool.get().await?;

        let roster = client.query_opt(
            "SELECT id, shift_label FROM timekeeping.shift_roster
             WHERE organization_id = $1 AND roster_date = $2",
            &[&org_id, &date],
        ).await?;

        let (roster_id, shift_label) = match roster {
            Some(r) => (Some(r.get::<_, Uuid>("id")), r.get::<_, String>("shift_label")),
            None => (None, "?".to_string()),
        };

        let station_rows = client.query(
            "SELECT s.id, s.name, s.min_staffing FROM timekeeping.stations s
             WHERE s.organization_id = $1 AND s.is_active = true ORDER BY s.station_number",
            &[&org_id],
        ).await?;

        let mut stations = Vec::new();
        let mut total_on_duty = 0i32;
        let mut alerts = Vec::new();

        for sr in &station_rows {
            let sid: Uuid = sr.get("id");
            let sname: String = sr.get("name");
            let min: i32 = sr.get("min_staffing");

            let personnel: Vec<String> = if let Some(rid) = roster_id {
                client.query(
                    "SELECT e.first_name || ' ' || e.last_name AS name
                     FROM timekeeping.roster_assignments a
                     JOIN timekeeping.employees e ON e.id = a.employee_id
                     WHERE a.roster_id = $1 AND a.station_id = $2",
                    &[&rid, &sid],
                ).await?.iter().map(|r| r.get("name")).collect()
            } else {
                vec![]
            };

            let actual = personnel.len() as i32;
            total_on_duty += actual;
            let below = actual < min;
            if below {
                alerts.push(format!("{} has {} of {} minimum staff", sname, actual, min));
            }

            stations.push(StationStaffing {
                station_id: sid,
                station_name: sname,
                min_staffing: min,
                actual_staffing: actual,
                is_below_minimum: below,
                personnel,
            });
        }

        let total_absent: i64 = client.query_one(
            "SELECT COUNT(*) AS c FROM timekeeping.absences
             WHERE organization_id = $1 AND absence_date = $2",
            &[&org_id, &date],
        ).await?.get("c");

        Ok(StaffingReport {
            date,
            shift_label,
            stations,
            total_on_duty,
            total_absent: total_absent as i32,
            alerts,
        })
    }

    /// Payroll export as JSON (can be converted to CSV by frontend).
    pub async fn payroll_export(pool: &PgPool, org_id: Uuid, period_id: Uuid) -> Result<Vec<serde_json::Value>, BpeError> {
        let client = pool.get().await?;

        let period = client.query_one(
            "SELECT period_start, period_end FROM timekeeping.timecard_periods WHERE id = $1 AND organization_id = $2",
            &[&period_id, &org_id],
        ).await.map_err(|_| BpeError::NotFound("Period not found".into()))?;

        let start: NaiveDate = period.get("period_start");
        let end: NaiveDate = period.get("period_end");

        let rows = client.query(
            "SELECT e.id, e.first_name, e.last_name, e.rank, e.employee_number,
                    pc.code, pc.display_name, pc.multiplier::float8 AS multiplier,
                    SUM(te.hours::float8) AS hours
             FROM timekeeping.time_entries te
             JOIN timekeeping.employees e ON e.id = te.employee_id
             JOIN timekeeping.pay_codes pc ON pc.id = te.pay_code_id
             WHERE te.organization_id = $1
               AND te.work_date BETWEEN $2 AND $3
               AND te.status IN ('approved', 'submitted')
             GROUP BY e.id, e.first_name, e.last_name, e.rank, e.employee_number,
                      pc.code, pc.display_name, pc.multiplier
             ORDER BY e.last_name, e.first_name, pc.code",
            &[&org_id, &start, &end],
        ).await?;

        Ok(rows.iter().map(|r| serde_json::json!({
            "employee_id": r.get::<_, Uuid>("id"),
            "first_name": r.get::<_, String>("first_name"),
            "last_name": r.get::<_, String>("last_name"),
            "rank": r.get::<_, String>("rank"),
            "employee_number": r.try_get::<_, String>("employee_number").ok(),
            "pay_code": r.get::<_, String>("code"),
            "pay_code_name": r.get::<_, String>("display_name"),
            "hours": r.get::<_, f64>("hours"),
            "multiplier": r.get::<_, f64>("multiplier"),
            "period_start": start.to_string(),
            "period_end": end.to_string(),
        })).collect())
    }
}
