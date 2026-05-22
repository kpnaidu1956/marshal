use chrono::Datelike;
use rust_decimal::prelude::FromPrimitive;
use crate::db::PgPool;
use crate::error::BpeError;
use super::models::*;
use uuid::Uuid;

pub struct LeaveBalanceManager;

impl LeaveBalanceManager {
    pub async fn list(pool: &PgPool, org_id: Uuid, query: &LeaveBalanceQuery) -> Result<Vec<LeaveBalance>, BpeError> {
        let year = query.year.unwrap_or_else(|| chrono::Utc::now().naive_utc().date().year() as i32);
        let client = pool.get().await?;
        let rows = client.query(
            "SELECT lb.*, e.first_name || ' ' || e.last_name AS employee_name
             FROM timekeeping.leave_balances lb
             JOIN timekeeping.employees e ON e.id = lb.employee_id
             WHERE lb.organization_id = $1 AND lb.year = $2
               AND ($3::uuid IS NULL OR lb.employee_id = $3)
             ORDER BY e.last_name, e.first_name, lb.leave_type",
            &[&org_id, &year, &query.employee_id],
        ).await?;

        Ok(rows.iter().map(row_to_balance).collect())
    }

    pub async fn adjust(pool: &PgPool, org_id: Uuid, req: &AdjustLeaveBalanceRequest) -> Result<LeaveBalance, BpeError> {
        let year = req.year.unwrap_or_else(|| chrono::Utc::now().naive_utc().date().year() as i32);
        let client = pool.get().await?;

        if req.balance_hours.is_nan() || req.balance_hours.is_infinite() {
            return Err(BpeError::BadRequest("Invalid balance_hours value".into()));
        }
        let bal_dec = rust_decimal::Decimal::from_f64(req.balance_hours)
            .ok_or_else(|| BpeError::BadRequest("Invalid balance_hours value".into()))?;
        let accrual_dec = req.accrual_rate.map(|a| rust_decimal::Decimal::from_f64(a)).flatten();
        let max_dec = req.max_balance.map(|m| rust_decimal::Decimal::from_f64(m)).flatten();

        let row = client.query_one(
            "INSERT INTO timekeeping.leave_balances
                (organization_id, employee_id, leave_type, balance_hours, accrual_rate, max_balance, year)
             VALUES ($1, $2, $3, $4, COALESCE($5, 0), $6, $7)
             ON CONFLICT (employee_id, leave_type, year) DO UPDATE SET
                balance_hours = EXCLUDED.balance_hours,
                accrual_rate = COALESCE(EXCLUDED.accrual_rate, timekeeping.leave_balances.accrual_rate),
                max_balance = COALESCE(EXCLUDED.max_balance, timekeeping.leave_balances.max_balance),
                updated_at = now()
             RETURNING *, NULL::text AS employee_name",
            &[&org_id, &req.employee_id, &req.leave_type, &bal_dec,
              &accrual_dec, &max_dec, &year],
        ).await?;

        Ok(row_to_balance(&row))
    }
}

fn row_to_balance(row: &tokio_postgres::Row) -> LeaveBalance {
    let balance_dec: rust_decimal::Decimal = row.get("balance_hours");
    let accrual_dec: rust_decimal::Decimal = row.get("accrual_rate");
    let max_dec: Option<rust_decimal::Decimal> = row.try_get("max_balance").ok().flatten();

    LeaveBalance {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        employee_id: row.get("employee_id"),
        employee_name: row.try_get("employee_name").ok().flatten(),
        leave_type: row.get("leave_type"),
        balance_hours: balance_dec.to_string().parse().unwrap_or(0.0),
        accrual_rate: accrual_dec.to_string().parse().unwrap_or(0.0),
        max_balance: max_dec.map(|d| d.to_string().parse().unwrap_or(0.0)),
        year: row.get("year"),
    }
}
