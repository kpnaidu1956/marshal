use chrono::{Datelike, NaiveDate};
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct CrossValidator;

impl CrossValidator {
    /// Run cross-validation for a date range.
    /// Checks roster vs. time entries for conflicts.
    pub async fn validate(pool: &PgPool, org_id: Uuid, start: NaiveDate, end: NaiveDate) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;

        // Clear existing unresolved flags for this date range (by flag_date, not created_at)
        client.execute(
            "DELETE FROM timekeeping.validation_flags
             WHERE organization_id = $1 AND is_resolved = false
               AND flag_date >= $2 AND flag_date <= $3",
            &[&org_id, &start, &end],
        ).await?;

        let mut total_flags = 0i64;
        let mut by_type: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

        let mut date = start;
        while date <= end {
            // Get roster for this date
            let roster = client.query_opt(
                "SELECT id, shift_label FROM timekeeping.shift_roster
                 WHERE organization_id = $1 AND roster_date = $2",
                &[&org_id, &date],
            ).await?;

            let roster_id: Option<Uuid> = roster.as_ref().map(|r| r.get("id"));

            // Get assigned employees for this roster
            let assigned: Vec<Uuid> = if let Some(rid) = roster_id {
                client.query(
                    "SELECT employee_id FROM timekeeping.roster_assignments WHERE roster_id = $1",
                    &[&rid],
                ).await?.iter().map(|r| r.get("employee_id")).collect()
            } else {
                vec![]
            };

            // Get absences for this date
            let absent: Vec<Uuid> = client.query(
                "SELECT employee_id FROM timekeeping.absences
                 WHERE organization_id = $1 AND absence_date = $2",
                &[&org_id, &date],
            ).await?.iter().map(|r| r.get("employee_id")).collect();

            // Get time entries for this date
            let entries = client.query(
                "SELECT te.id, te.employee_id, te.hours::float8 AS hours, pc.category, pc.code,
                        e.first_name || ' ' || e.last_name AS emp_name
                 FROM timekeeping.time_entries te
                 JOIN timekeeping.pay_codes pc ON pc.id = te.pay_code_id
                 JOIN timekeeping.employees e ON e.id = te.employee_id
                 WHERE te.organization_id = $1 AND te.work_date = $2",
                &[&org_id, &date],
            ).await?;

            // Aggregate hours per employee for this date
            let mut emp_daily_hours: std::collections::HashMap<Uuid, (f64, String, Vec<Uuid>)> = std::collections::HashMap::new();
            for entry_row in &entries {
                let te_id: Uuid = entry_row.get("id");
                let emp_id: Uuid = entry_row.get("employee_id");
                let hours: f64 = entry_row.get("hours");
                let emp_name: String = entry_row.get("emp_name");

                let entry = emp_daily_hours.entry(emp_id).or_insert_with(|| (0.0, emp_name.clone(), Vec::new()));
                entry.0 += hours;
                entry.2.push(te_id);
            }

            // ROSTER_CONFLICT: Employee marked absent but claims work hours (deduplicated per employee)
            {
                let mut conflict_seen = std::collections::HashSet::new();
                for entry_row in &entries {
                    let te_id: Uuid = entry_row.get("id");
                    let emp_id: Uuid = entry_row.get("employee_id");
                    let category: String = entry_row.get("category");
                    let emp_name: String = entry_row.get("emp_name");

                    if absent.contains(&emp_id) && category == "work" && conflict_seen.insert(emp_id) {
                        insert_flag(&client, org_id, emp_id, Some(te_id), roster_id,
                            "roster_conflict", "warning",
                            &format!("{} claimed work hours on {} but is marked absent on the roster", emp_name, date),
                            date,
                        ).await?;
                        total_flags += 1;
                        *by_type.entry("roster_conflict".into()).or_default() += 1;
                    }
                }
            }

            // Check aggregated daily hours per employee
            let is_weekend = date.weekday() == chrono::Weekday::Sat || date.weekday() == chrono::Weekday::Sun;

            for (emp_id, (total_hours, emp_name, entry_ids)) in &emp_daily_hours {
                // HOURS_EXCEEDED: More than 24 hours in a single day
                if *total_hours > 24.0 {
                    insert_flag(&client, org_id, *emp_id, entry_ids.first().copied(), roster_id,
                        "hours_exceeded", "error",
                        &format!("{} claimed {:.1} total hours on {} (exceeds 24 hours)", emp_name, total_hours, date),
                        date,
                    ).await?;
                    total_flags += 1;
                    *by_type.entry("hours_exceeded".into()).or_default() += 1;
                }

                // EXCESS_DAILY_HOURS: More than 8 hours (standard shift) but not on roster as 24hr
                if *total_hours > 8.0 && *total_hours <= 24.0 {
                    let is_24hr = if let Some(rid) = roster_id {
                        client.query_opt(
                            "SELECT 1 FROM timekeeping.roster_assignments WHERE roster_id = $1 AND employee_id = $2 AND is_24hr = true",
                            &[&rid, emp_id],
                        ).await?.is_some()
                    } else {
                        false
                    };

                    if !is_24hr {
                        insert_flag(&client, org_id, *emp_id, entry_ids.first().copied(), roster_id,
                            "excess_daily_hours", "warning",
                            &format!("{} claimed {:.1} hours on {} (exceeds 8-hour shift)", emp_name, total_hours, date),
                            date,
                        ).await?;
                        total_flags += 1;
                        *by_type.entry("excess_daily_hours".into()).or_default() += 1;
                    }
                }

                // NOT_ON_ROSTER: Employee has time entries but is not on the roster
                let on_roster = assigned.contains(emp_id);
                if !on_roster && roster_id.is_some() {
                    insert_flag(&client, org_id, *emp_id, entry_ids.first().copied(), roster_id,
                        "not_on_roster", "warning",
                        &format!("{} entered {:.1} hours on {} but is not assigned on the roster", emp_name, total_hours, date),
                        date,
                    ).await?;
                    total_flags += 1;
                    *by_type.entry("not_on_roster".into()).or_default() += 1;

                    // WEEKEND_HOURS: Only fire if NOT already flagged as not_on_roster AND it's a weekend
                    // (avoid double-flagging — weekend_hours is for employees who ARE rostered on weekends unexpectedly)
                } else if is_weekend && on_roster {
                    // Employee is on the roster for a weekend — could be intentional (OT), flag as info
                    // Skip for now — weekend roster assignments are valid
                }

                // WEEKEND_HOURS: Non-rostered employee working on weekend (only if no roster exists at all)
                if is_weekend && !on_roster && roster_id.is_none() {
                    insert_flag(&client, org_id, *emp_id, entry_ids.first().copied(), roster_id,
                        "weekend_hours", "warning",
                        &format!("{} entered {:.1} hours on {} ({}) without weekend roster assignment", emp_name, total_hours, date, date.weekday()),
                        date,
                    ).await?;
                    total_flags += 1;
                    *by_type.entry("weekend_hours".into()).or_default() += 1;
                }
            }

            // DUPLICATE_ENTRY: Multiple entries for same employee + date + pay code
            {
                let dups = client.query(
                    "SELECT te.employee_id, pc.code, COUNT(*) AS cnt,
                            e.first_name || ' ' || e.last_name AS emp_name
                     FROM timekeeping.time_entries te
                     JOIN timekeeping.pay_codes pc ON pc.id = te.pay_code_id
                     JOIN timekeeping.employees e ON e.id = te.employee_id
                     WHERE te.organization_id = $1 AND te.work_date = $2
                     GROUP BY te.employee_id, pc.code, e.first_name, e.last_name
                     HAVING COUNT(*) > 1",
                    &[&org_id, &date],
                ).await?;

                for dup_row in &dups {
                    let emp_id: Uuid = dup_row.get("employee_id");
                    let code: String = dup_row.get("code");
                    let cnt: i64 = dup_row.get("cnt");
                    let emp_name: String = dup_row.get("emp_name");

                    insert_flag(&client, org_id, emp_id, None, roster_id,
                        "duplicate_entry", "warning",
                        &format!("{} has {} entries for {} on {} (expected 1)", emp_name, cnt, code, date),
                        date,
                    ).await?;
                    total_flags += 1;
                    *by_type.entry("duplicate_entry".into()).or_default() += 1;
                }
            }

            // MISSING_ENTRY: Employee on roster (not absent) but no time entry
            for emp_id in &assigned {
                if absent.contains(emp_id) {
                    continue;
                }
                let has_entry = entries.iter().any(|r| r.get::<_, Uuid>("employee_id") == *emp_id);
                if !has_entry {
                    let emp_name: String = client.query_one(
                        "SELECT first_name || ' ' || last_name AS name FROM timekeeping.employees WHERE id = $1",
                        &[emp_id],
                    ).await?.get("name");

                    insert_flag(&client, org_id, *emp_id, None, roster_id,
                        "missing_entry", "info",
                        &format!("{} is on roster for {} but has no time entry", emp_name, date),
                        date,
                    ).await?;
                    total_flags += 1;
                    *by_type.entry("missing_entry".into()).or_default() += 1;
                }
            }

            // Check minimum staffing (exclude absent employees from count)
            if let Some(rid) = roster_id {
                let stations = client.query(
                    "SELECT s.id, s.name, s.station_number, s.min_staffing,
                            COUNT(a.id) FILTER (WHERE a.employee_id IS NOT NULL
                                AND a.employee_id NOT IN (SELECT employee_id FROM timekeeping.absences WHERE organization_id = $2 AND absence_date = $3)
                            ) AS actual
                     FROM timekeeping.stations s
                     LEFT JOIN timekeeping.roster_assignments a ON a.station_id = s.id AND a.roster_id = $1
                     WHERE s.organization_id = $2 AND s.is_active = true
                     GROUP BY s.id, s.name, s.station_number, s.min_staffing",
                    &[&rid, &org_id, &date],
                ).await?;

                for st in &stations {
                    let actual: i64 = st.get("actual");
                    let min: i32 = st.get("min_staffing");
                    if actual < min as i64 {
                        let sname: String = st.get("name");
                        // Use first assigned employee for the flag, or skip if none
                        if let Some(&flag_emp) = assigned.first() {
                            insert_flag(&client, org_id, flag_emp, None, Some(rid),
                                "staffing_below_min", "error",
                                &format!("{} has {} of {} minimum staff on {}", sname, actual, min, date),
                                date,
                            ).await?;
                            total_flags += 1;
                            *by_type.entry("staffing_below_min".into()).or_default() += 1;
                        }
                    }
                }
            }

            date = match date.succ_opt() {
                Some(d) => d,
                None => break,
            };
        }

        Ok(serde_json::json!({
            "total_flags": total_flags,
            "by_type": by_type,
            "range": { "start": start.to_string(), "end": end.to_string() }
        }))
    }

    pub async fn list_flags(pool: &PgPool, org_id: Uuid, query: &FlagsQuery) -> Result<Vec<ValidationFlag>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT f.*, e.first_name || ' ' || e.last_name AS employee_name
             FROM timekeeping.validation_flags f
             JOIN timekeeping.employees e ON e.id = f.employee_id
             WHERE f.organization_id = $1
               AND ($2::bool IS NULL OR f.is_resolved = $2)
               AND ($3::uuid IS NULL OR f.employee_id = $3)
               AND ($4::text IS NULL OR f.flag_type = $4)
               AND ($5::date IS NULL OR f.flag_date >= $5)
               AND ($6::date IS NULL OR f.flag_date <= $6)
             ORDER BY f.flag_date DESC NULLS LAST, f.created_at DESC",
            &[&org_id, &query.resolved, &query.employee_id, &query.flag_type.as_deref(),
              &query.start, &query.end],
        ).await?;

        Ok(rows.iter().map(row_to_flag).collect())
    }

    pub async fn resolve_flag(pool: &PgPool, id: Uuid, user_id: Uuid, note: Option<&str>) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client.execute(
            "UPDATE timekeeping.validation_flags SET
                is_resolved = true, resolved_by = $2, resolved_at = now(), resolution_note = $3
             WHERE id = $1",
            &[&id, &user_id, &note],
        ).await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Flag {id} not found")));
        }
        Ok(())
    }
}

async fn insert_flag(
    client: &deadpool_postgres::Client,
    org_id: Uuid, emp_id: Uuid, te_id: Option<Uuid>, roster_id: Option<Uuid>,
    flag_type: &str, severity: &str, message: &str,
    flag_date: NaiveDate,
) -> Result<(), BpeError> {
    client.execute(
        "INSERT INTO timekeeping.validation_flags
            (organization_id, employee_id, time_entry_id, roster_id, flag_type, severity, message, flag_date)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[&org_id, &emp_id, &te_id, &roster_id, &flag_type, &severity, &message, &flag_date],
    ).await?;
    Ok(())
}

fn row_to_flag(row: &tokio_postgres::Row) -> ValidationFlag {
    ValidationFlag {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        employee_id: row.get("employee_id"),
        employee_name: row.try_get("employee_name").ok().flatten(),
        time_entry_id: row.try_get("time_entry_id").ok().flatten(),
        roster_id: row.try_get("roster_id").ok().flatten(),
        flag_type: row.get("flag_type"),
        severity: row.get("severity"),
        message: row.get("message"),
        flag_date: row.try_get("flag_date").ok().flatten(),
        is_resolved: row.get("is_resolved"),
        resolved_by: row.try_get("resolved_by").ok().flatten(),
        resolved_at: row.try_get("resolved_at").ok().flatten(),
        resolution_note: row.try_get("resolution_note").ok().flatten(),
        created_at: row.get("created_at"),
    }
}
