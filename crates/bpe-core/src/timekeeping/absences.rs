use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct AbsenceManager;

const VALID_ABSENCE_TYPES: &[&str] = &[
    "Vacation", "Sick", "LightDuty", "WorkerComp", "Trade",
    "FMLA", "Bereavement", "Military", "Personal", "CompTime",
];

impl AbsenceManager {
    pub async fn list(pool: &PgPool, org_id: Uuid, query: &AbsenceQuery) -> Result<Vec<Absence>, BpeError> {
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT ab.*, e.first_name || ' ' || e.last_name AS employee_name
             FROM timekeeping.absences ab
             JOIN timekeeping.employees e ON e.id = ab.employee_id
             WHERE ab.organization_id = $1
               AND ($2::date IS NULL OR ab.absence_date >= $2)
               AND ($3::date IS NULL OR ab.absence_date <= $3)
               AND ($4::uuid IS NULL OR ab.employee_id = $4)
               AND ($5::text IS NULL OR ab.absence_type = $5)
             ORDER BY ab.absence_date DESC, e.last_name",
            &[&org_id, &query.start, &query.end, &query.employee_id, &query.absence_type.as_deref()],
        ).await?;

        Ok(rows.iter().map(row_to_absence).collect())
    }

    pub async fn create(pool: &PgPool, org_id: Uuid, req: &CreateAbsenceRequest) -> Result<Absence, BpeError> {
        if !VALID_ABSENCE_TYPES.iter().any(|t| t.eq_ignore_ascii_case(&req.absence_type)) {
            return Err(BpeError::BadRequest(format!("Invalid absence type '{}'. Valid: {:?}", req.absence_type, VALID_ABSENCE_TYPES)));
        }

        let client = pool.get().await?;

        // Link to roster if one exists for this date
        let roster_id: Option<Uuid> = client.query_opt(
            "SELECT id FROM timekeeping.shift_roster WHERE organization_id = $1 AND roster_date = $2",
            &[&org_id, &req.absence_date],
        ).await?.map(|r| r.get("id"));

        let row = client.query_one(
            "INSERT INTO timekeeping.absences
                (organization_id, employee_id, roster_id, absence_date, absence_type, notes)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING *, NULL::text AS employee_name",
            &[&org_id, &req.employee_id, &roster_id, &req.absence_date, &req.absence_type, &req.notes],
        ).await?;

        Ok(row_to_absence(&row))
    }

    pub async fn approve(pool: &PgPool, id: Uuid, approver_id: Uuid, approve: bool) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let status = if approve { "approved" } else { "denied" };
        let n = client.execute(
            "UPDATE timekeeping.absences SET status = $2, approved_by = $3, approved_at = now(), updated_at = now()
             WHERE id = $1",
            &[&id, &status, &approver_id],
        ).await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Absence {id} not found")));
        }
        Ok(())
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client.execute("DELETE FROM timekeeping.absences WHERE id = $1", &[&id]).await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Absence {id} not found")));
        }
        Ok(())
    }
}

fn row_to_absence(row: &tokio_postgres::Row) -> Absence {
    Absence {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        employee_id: row.get("employee_id"),
        employee_name: row.try_get("employee_name").ok().flatten(),
        roster_id: row.try_get("roster_id").ok().flatten(),
        absence_date: row.get("absence_date"),
        absence_type: row.get("absence_type"),
        notes: row.try_get("notes").ok().flatten(),
        approved_by: row.try_get("approved_by").ok().flatten(),
        approved_at: row.try_get("approved_at").ok().flatten(),
        status: row.get("status"),
    }
}
