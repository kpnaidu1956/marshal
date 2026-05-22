use chrono::NaiveDate;
use crate::db::PgPool;
use crate::error::BpeError;
use super::kelly::KellySchedule;
use super::models::*;
use uuid::Uuid;

pub struct RosterManager;

impl RosterManager {
    /// Get roster for a specific date (with assignments and absences).
    pub async fn get_for_date(pool: &PgPool, org_id: Uuid, date: NaiveDate) -> Result<Option<ShiftRoster>, BpeError> {
        let client = pool.get().await?;
        let roster_row = client.query_opt(
            "SELECT r.*, e.first_name || ' ' || e.last_name AS duty_chief_name
             FROM timekeeping.shift_roster r
             LEFT JOIN timekeeping.employees e ON e.id = r.duty_chief_id
             WHERE r.organization_id = $1 AND r.roster_date = $2",
            &[&org_id, &date],
        ).await?;

        let Some(rr) = roster_row else { return Ok(None) };
        let roster_id: Uuid = rr.get("id");

        let assignments = Self::get_assignments(pool, roster_id).await?;
        let absences = Self::get_absences_for_date(pool, org_id, date).await?;

        Ok(Some(ShiftRoster {
            id: rr.get("id"),
            organization_id: rr.get("organization_id"),
            roster_date: rr.get("roster_date"),
            shift_label: rr.get("shift_label"),
            duty_chief_id: rr.try_get("duty_chief_id").ok().flatten(),
            duty_chief_name: rr.try_get("duty_chief_name").ok().flatten(),
            notes: rr.try_get("notes").ok().flatten(),
            is_auto_generated: rr.get("is_auto_generated"),
            is_locked: rr.get("is_locked"),
            assignments,
            absences,
        }))
    }

    /// Get roster range (summary per day, without full assignment details).
    pub async fn get_range(pool: &PgPool, org_id: Uuid, start: NaiveDate, end: NaiveDate) -> Result<Vec<serde_json::Value>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT r.id, r.roster_date, r.shift_label, r.is_locked,
                    e.first_name || ' ' || e.last_name AS duty_chief_name,
                    (SELECT COUNT(*) FROM timekeeping.roster_assignments a WHERE a.roster_id = r.id) AS assignment_count,
                    (SELECT COUNT(*) FROM timekeeping.absences ab WHERE ab.organization_id = r.organization_id AND ab.absence_date = r.roster_date) AS absence_count
             FROM timekeeping.shift_roster r
             LEFT JOIN timekeeping.employees e ON e.id = r.duty_chief_id
             WHERE r.organization_id = $1 AND r.roster_date BETWEEN $2 AND $3
             ORDER BY r.roster_date",
            &[&org_id, &start, &end],
        ).await?;

        Ok(rows.iter().map(|r| {
            serde_json::json!({
                "id": r.get::<_, Uuid>("id"),
                "roster_date": r.get::<_, NaiveDate>("roster_date").to_string(),
                "shift_label": r.get::<_, String>("shift_label"),
                "is_locked": r.get::<_, bool>("is_locked"),
                "duty_chief_name": r.try_get::<_, String>("duty_chief_name").ok(),
                "assignment_count": r.get::<_, i64>("assignment_count"),
                "absence_count": r.get::<_, i64>("absence_count"),
            })
        }).collect())
    }

    /// Auto-generate roster from Kelly schedule for a date range.
    pub async fn generate(pool: &PgPool, org_id: Uuid, start: NaiveDate, end: NaiveDate) -> Result<serde_json::Value, BpeError> {
        let config = KellySchedule::get_config(pool, org_id).await?;
        let client = pool.get().await?;

        // Load stations
        let station_rows = client.query(
            "SELECT id, station_number FROM timekeeping.stations
             WHERE organization_id = $1 AND is_active = true ORDER BY station_number",
            &[&org_id],
        ).await?;

        let stations: Vec<(Uuid, i32)> = station_rows.iter()
            .map(|r| (r.get("id"), r.get("station_number")))
            .collect();

        let mut created = 0i64;
        let mut skipped = 0i64;
        let mut alerts: Vec<String> = Vec::new();
        let mut date = start;

        while date <= end {
            let shift_label = KellySchedule::compute_shift_for_date(&config, date);

            // Skip if roster already exists for this date
            let existing = client.query_opt(
                "SELECT id FROM timekeeping.shift_roster WHERE organization_id = $1 AND roster_date = $2",
                &[&org_id, &date],
            ).await?;

            if existing.is_some() {
                skipped += 1;
                date = match date.succ_opt() { Some(d) => d, None => break };
                continue;
            }

            // Create roster
            let roster_row = client.query_one(
                "INSERT INTO timekeeping.shift_roster (organization_id, roster_date, shift_label)
                 VALUES ($1, $2, $3) RETURNING id",
                &[&org_id, &date, &shift_label],
            ).await?;
            let roster_id: Uuid = roster_row.get("id");

            // Find ALL shift-assigned employees (A, B, C) — all 3 shifts run each day
            let emp_rows = client.query(
                "SELECT id, default_station_id, rank, shift_assignment FROM timekeeping.employees
                 WHERE organization_id = $1 AND shift_assignment IS NOT NULL AND status = 'active'
                 ORDER BY shift_assignment, rank, last_name",
                &[&org_id],
            ).await?;

            // Check for pre-approved absences
            let absent_ids: Vec<Uuid> = client.query(
                "SELECT employee_id FROM timekeeping.absences
                 WHERE organization_id = $1 AND absence_date = $2 AND status = 'approved'",
                &[&org_id, &date],
            ).await?.iter().map(|r| r.get("employee_id")).collect();

            // Assign employees to stations (round-robin per shift if no default station)
            let mut station_idx = 0usize;
            for emp_row in &emp_rows {
                let emp_id: Uuid = emp_row.get("id");
                if absent_ids.contains(&emp_id) {
                    continue;
                }

                let station_id: Option<Uuid> = emp_row.try_get("default_station_id").ok().flatten();
                let assigned_station = station_id.or_else(|| {
                    if !stations.is_empty() {
                        let sid = stations[station_idx % stations.len()].0;
                        station_idx += 1;
                        Some(sid)
                    } else {
                        None
                    }
                });

                client.execute(
                    "INSERT INTO timekeeping.roster_assignments
                        (organization_id, roster_id, employee_id, station_id, assignment_type)
                     VALUES ($1, $2, $3, $4, 'regular')
                     ON CONFLICT (roster_id, employee_id) DO NOTHING",
                    &[&org_id, &roster_id, &emp_id, &assigned_station],
                ).await?;
            }

            // Check minimum staffing per station
            for (sid, snum) in &stations {
                let count: i64 = client.query_one(
                    "SELECT COUNT(*) AS c FROM timekeeping.roster_assignments
                     WHERE roster_id = $1 AND station_id = $2",
                    &[&roster_id, sid],
                ).await?.get("c");

                let min: i32 = client.query_one(
                    "SELECT min_staffing FROM timekeeping.stations WHERE id = $1",
                    &[sid],
                ).await?.get("min_staffing");

                if count < min as i64 {
                    alerts.push(format!(
                        "{}: Station {} has {} of {} min staff",
                        date, snum, count, min
                    ));
                }
            }

            created += 1;
            date = match date.succ_opt() { Some(d) => d, None => break };
        }

        Ok(serde_json::json!({
            "created": created,
            "skipped": skipped,
            "alerts": alerts,
            "range": { "start": start.to_string(), "end": end.to_string() }
        }))
    }

    /// Update roster metadata (duty chief, notes).
    pub async fn update(pool: &PgPool, id: Uuid, req: &UpdateRosterRequest) -> Result<(), BpeError> {
        let client = pool.get().await?;

        // Check not locked
        let locked: bool = client.query_one(
            "SELECT is_locked FROM timekeeping.shift_roster WHERE id = $1",
            &[&id],
        ).await.map_err(|_| BpeError::NotFound(format!("Roster {id} not found")))?
        .get("is_locked");

        if locked {
            return Err(BpeError::BadRequest("Roster is locked and cannot be modified".into()));
        }

        client.execute(
            "UPDATE timekeeping.shift_roster SET
                duty_chief_id = COALESCE($2, duty_chief_id),
                notes = COALESCE($3, notes),
                is_auto_generated = false,
                updated_at = now()
             WHERE id = $1",
            &[&id, &req.duty_chief_id, &req.notes],
        ).await?;

        Ok(())
    }

    /// Lock a roster (prevents further edits).
    pub async fn lock(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        client.execute(
            "UPDATE timekeeping.shift_roster SET is_locked = true, updated_at = now() WHERE id = $1",
            &[&id],
        ).await?;
        Ok(())
    }

    /// Unlock a roster (re-enables editing).
    pub async fn unlock(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client.execute(
            "UPDATE timekeeping.shift_roster SET is_locked = false, updated_at = now() WHERE id = $1",
            &[&id],
        ).await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Roster {id} not found")));
        }
        Ok(())
    }

    /// Batch update assignments for a roster.
    pub async fn update_assignments(pool: &PgPool, org_id: Uuid, roster_id: Uuid, req: &UpdateAssignmentsRequest) -> Result<Vec<RosterAssignment>, BpeError> {
        let client = pool.get().await?;

        // Check not locked
        let locked: bool = client.query_one(
            "SELECT is_locked FROM timekeeping.shift_roster WHERE id = $1",
            &[&roster_id],
        ).await.map_err(|_| BpeError::NotFound(format!("Roster {roster_id} not found")))?
        .get("is_locked");

        if locked {
            return Err(BpeError::BadRequest("Roster is locked".into()));
        }

        // Clear existing and re-insert
        client.execute(
            "DELETE FROM timekeeping.roster_assignments WHERE roster_id = $1",
            &[&roster_id],
        ).await?;

        for a in &req.assignments {
            client.execute(
                "INSERT INTO timekeeping.roster_assignments
                    (organization_id, roster_id, employee_id, station_id, assignment_type, is_cism_coverage, is_24hr, notes)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[&org_id, &roster_id, &a.employee_id, &a.station_id,
                  &a.assignment_type, &a.is_cism_coverage, &a.is_24hr, &a.notes],
            ).await?;
        }

        // Mark as manually edited
        client.execute(
            "UPDATE timekeeping.shift_roster SET is_auto_generated = false, updated_at = now() WHERE id = $1",
            &[&roster_id],
        ).await?;

        Self::get_assignments(pool, roster_id).await
    }

    async fn get_assignments(pool: &PgPool, roster_id: Uuid) -> Result<Vec<RosterAssignment>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT a.*, e.first_name || ' ' || e.last_name AS employee_name,
                    e.rank AS employee_rank, e.shift_assignment AS employee_shift,
                    s.name AS station_name
             FROM timekeeping.roster_assignments a
             JOIN timekeeping.employees e ON e.id = a.employee_id
             LEFT JOIN timekeeping.stations s ON s.id = a.station_id
             WHERE a.roster_id = $1
             ORDER BY e.shift_assignment, s.station_number, e.rank, e.last_name",
            &[&roster_id],
        ).await?;

        Ok(rows.iter().map(|r| RosterAssignment {
            id: r.get("id"),
            roster_id: r.get("roster_id"),
            employee_id: r.get("employee_id"),
            employee_name: r.try_get("employee_name").ok(),
            employee_rank: r.try_get("employee_rank").ok(),
            employee_shift: r.try_get("employee_shift").ok().flatten(),
            station_id: r.try_get("station_id").ok().flatten(),
            station_name: r.try_get("station_name").ok(),
            assignment_type: r.get("assignment_type"),
            is_cism_coverage: r.get("is_cism_coverage"),
            is_24hr: r.try_get("is_24hr").ok().unwrap_or(false),
            notes: r.try_get("notes").ok().flatten(),
        }).collect())
    }

    async fn get_absences_for_date(pool: &PgPool, org_id: Uuid, date: NaiveDate) -> Result<Vec<Absence>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT ab.*, e.first_name || ' ' || e.last_name AS employee_name
             FROM timekeeping.absences ab
             JOIN timekeeping.employees e ON e.id = ab.employee_id
             WHERE ab.organization_id = $1 AND ab.absence_date = $2
             ORDER BY ab.absence_type, e.last_name",
            &[&org_id, &date],
        ).await?;

        Ok(rows.iter().map(|r| Absence {
            id: r.get("id"),
            organization_id: r.get("organization_id"),
            employee_id: r.get("employee_id"),
            employee_name: r.try_get("employee_name").ok(),
            roster_id: r.try_get("roster_id").ok().flatten(),
            absence_date: r.get("absence_date"),
            absence_type: r.get("absence_type"),
            notes: r.try_get("notes").ok().flatten(),
            approved_by: r.try_get("approved_by").ok().flatten(),
            approved_at: r.try_get("approved_at").ok().flatten(),
            status: r.get("status"),
        }).collect())
    }
}
