use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct EmployeeManager;

impl EmployeeManager {
    pub async fn list(pool: &PgPool, org_id: Uuid, query: &ListEmployeesQuery) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        let rows = client.query(
            "SELECT e.*, s.name AS station_name,
                    COUNT(*) OVER() AS total_count
             FROM timekeeping.employees e
             LEFT JOIN timekeeping.stations s ON s.id = e.default_station_id
             WHERE e.organization_id = $1
               AND ($2::text IS NULL OR e.shift_assignment = $2)
               AND ($3::text IS NULL OR e.rank = $3)
               AND ($4::text IS NULL OR e.status = $4)
               AND ($5::text IS NULL OR (e.first_name || ' ' || e.last_name) ILIKE '%' || $5 || '%')
             ORDER BY e.rank, e.last_name, e.first_name
             LIMIT $6 OFFSET $7",
            &[
                &org_id,
                &query.shift.as_deref(),
                &query.rank.as_deref(),
                &query.status.as_deref(),
                &query.search.as_deref(),
                &per_page,
                &offset,
            ],
        ).await?;

        let total: i64 = rows.first().map(|r| r.get("total_count")).unwrap_or(0);
        let employees: Vec<Employee> = rows.iter().map(row_to_employee).collect();

        Ok(serde_json::json!({
            "data": employees,
            "total": total,
            "page": page,
            "per_page": per_page
        }))
    }

    pub async fn get(pool: &PgPool, id: Uuid) -> Result<Employee, BpeError> {
        let client = pool.get().await?;
        let row = client.query_opt(
            "SELECT e.* FROM timekeeping.employees e WHERE e.id = $1",
            &[&id],
        ).await?
        .ok_or_else(|| BpeError::NotFound(format!("Employee {id} not found")))?;
        Ok(row_to_employee(&row))
    }

    pub async fn create(pool: &PgPool, org_id: Uuid, req: &CreateEmployeeRequest) -> Result<Employee, BpeError> {
        validate_rank(&req.rank)?;
        if let Some(ref s) = req.shift_assignment {
            validate_shift(s)?;
        }

        let client = pool.get().await?;
        let row = client.query_one(
            "INSERT INTO timekeeping.employees
                (organization_id, first_name, last_name, rank, employee_number,
                 shift_assignment, default_station_id, phone1, phone2,
                 address_line1, city, state, zip, hire_date)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
             RETURNING *",
            &[
                &org_id, &req.first_name, &req.last_name, &req.rank,
                &req.employee_number, &req.shift_assignment.as_deref(),
                &req.default_station_id, &req.phone1, &req.phone2,
                &req.address_line1, &req.city, &req.state, &req.zip, &req.hire_date,
            ],
        ).await?;

        Ok(row_to_employee(&row))
    }

    pub async fn update(pool: &PgPool, id: Uuid, req: &UpdateEmployeeRequest) -> Result<Employee, BpeError> {
        if let Some(ref r) = req.rank {
            validate_rank(r)?;
        }
        if let Some(ref s) = req.shift_assignment {
            validate_shift(s)?;
        }

        let client = pool.get().await?;
        let row = client.query_one(
            "UPDATE timekeeping.employees SET
                first_name = COALESCE($2, first_name),
                last_name = COALESCE($3, last_name),
                rank = COALESCE($4, rank),
                employee_number = COALESCE($5, employee_number),
                shift_assignment = COALESCE($6, shift_assignment),
                default_station_id = COALESCE($7, default_station_id),
                phone1 = COALESCE($8, phone1),
                phone2 = COALESCE($9, phone2),
                address_line1 = COALESCE($10, address_line1),
                city = COALESCE($11, city),
                state = COALESCE($12, state),
                zip = COALESCE($13, zip),
                hire_date = COALESCE($14, hire_date),
                status = COALESCE($15, status),
                updated_at = now()
             WHERE id = $1
             RETURNING *",
            &[
                &id, &req.first_name, &req.last_name, &req.rank,
                &req.employee_number, &req.shift_assignment.as_deref(),
                &req.default_station_id, &req.phone1, &req.phone2,
                &req.address_line1, &req.city, &req.state, &req.zip,
                &req.hire_date, &req.status,
            ],
        ).await?;

        Ok(row_to_employee(&row))
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), BpeError> {
        let client = pool.get().await?;
        let n = client.execute(
            "UPDATE timekeeping.employees SET status = 'inactive', updated_at = now() WHERE id = $1",
            &[&id],
        ).await?;
        if n == 0 {
            return Err(BpeError::NotFound(format!("Employee {id} not found")));
        }
        Ok(())
    }

    pub async fn import(pool: &PgPool, org_id: Uuid, employees: &[ImportEmployeeEntry]) -> Result<serde_json::Value, BpeError> {
        let client = pool.get().await?;
        let mut created = 0i64;
        let mut skipped = 0i64;
        let mut errors: Vec<String> = Vec::new();

        for (i, emp) in employees.iter().enumerate() {
            if emp.first_name.is_empty() || emp.last_name.is_empty() {
                errors.push(format!("Row {}: missing name", i + 1));
                continue;
            }
            if let Err(e) = validate_rank(&emp.rank) {
                errors.push(format!("Row {}: {e}", i + 1));
                continue;
            }

            // Check for duplicate by name
            let existing = client.query_opt(
                "SELECT id FROM timekeeping.employees
                 WHERE organization_id = $1 AND UPPER(first_name) = UPPER($2) AND UPPER(last_name) = UPPER($3)",
                &[&org_id, &emp.first_name, &emp.last_name],
            ).await?;

            if existing.is_some() {
                skipped += 1;
                continue;
            }

            client.execute(
                "INSERT INTO timekeeping.employees
                    (organization_id, first_name, last_name, rank, shift_assignment,
                     phone1, phone2, address_line1, city, state, zip)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                &[
                    &org_id, &emp.first_name, &emp.last_name, &emp.rank,
                    &emp.shift_assignment.as_deref(),
                    &emp.phone1, &emp.phone2,
                    &emp.address_line1, &emp.city, &emp.state, &emp.zip,
                ],
            ).await?;
            created += 1;
        }

        Ok(serde_json::json!({
            "created": created,
            "skipped": skipped,
            "errors": errors,
            "total_submitted": employees.len()
        }))
    }
}

fn validate_rank(rank: &str) -> Result<(), BpeError> {
    const VALID_RANKS: &[&str] = &[
        "Administration", "Chief", "Division Chief", "Battalion Chief", "Captain",
        "Lieutenant", "Engineer", "Firefighter", "Reserve",
        "Technical Specialist", "Paramedic",
    ];
    if !VALID_RANKS.iter().any(|r| r.eq_ignore_ascii_case(rank)) {
        return Err(BpeError::BadRequest(format!(
            "Invalid rank '{}'. Valid: {:?}", rank, VALID_RANKS
        )));
    }
    Ok(())
}

fn validate_shift(shift: &str) -> Result<(), BpeError> {
    if !matches!(shift, "A" | "B" | "C" | "a" | "b" | "c") {
        return Err(BpeError::BadRequest(format!("Invalid shift '{}'. Must be A, B, or C", shift)));
    }
    Ok(())
}

fn row_to_employee(row: &tokio_postgres::Row) -> Employee {
    Employee {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        user_id: row.try_get("user_id").ok().flatten(),
        first_name: row.get("first_name"),
        last_name: row.get("last_name"),
        rank: row.get("rank"),
        employee_number: row.try_get("employee_number").ok().flatten(),
        shift_assignment: row.try_get("shift_assignment").ok().flatten(),
        default_station_id: row.try_get("default_station_id").ok().flatten(),
        phone1: row.try_get("phone1").ok().flatten(),
        phone2: row.try_get("phone2").ok().flatten(),
        address_line1: row.try_get("address_line1").ok().flatten(),
        city: row.try_get("city").ok().flatten(),
        state: row.try_get("state").ok().flatten(),
        zip: row.try_get("zip").ok().flatten(),
        hire_date: row.try_get("hire_date").ok().flatten(),
        status: row.get("status"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
